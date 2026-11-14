//! MP3 encoding through a `lame` program the user installed, run as a separate process
//! (`docs/architecture/import-export.md` "MP3 export", ADR-0129).
//!
//! Nothing is linked: raw 16-bit PCM goes to lame's standard input and the MP3 stream comes back
//! on its standard output, which is copied into a file Debussy opened. Lame is never given a
//! path, and no shell is involved.

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use platform_interfaces::{Encoder, Error};

/// The most of lame's standard error kept for an error message: its last 4 KiB.
const STDERR_LIMIT: usize = 4096;
/// The most of `lame --version`'s output read by the probe.
const VERSION_LIMIT: u64 = 4096;
/// How long the probe waits for `lame --version` before giving up on it.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// How often the probe checks whether `lame --version` has exited.
const PROBE_POLL: Duration = Duration::from_millis(10);
/// The highest sample rate MP3 holds; projects above it are resampled by lame.
const MP3_MAX_RATE: u32 = 48_000;

/// A `lame` executable that answered the probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lame {
    path: PathBuf,
}

impl Lame {
    /// The first of `candidates` that is an executable file and whose `--version` output names
    /// LAME; `None` if none is.
    pub fn probe(candidates: &[PathBuf]) -> Option<Lame> {
        candidates
            .iter()
            .find(|path| is_executable(path) && answers_version(path))
            .map(|path| Lame { path: path.clone() })
    }

    /// Where the executable is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Creates `output` and starts lame encoding `channels` channels (1 or 2) of 16-bit samples
    /// at `sample_rate` into it, at `kbps` kilobits per second. Projects above 48 kHz are
    /// resampled to 44.1 kHz (multiples of 44.1 kHz) or 48 kHz.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] if the file cannot be created or lame cannot be started. No file is left
    /// behind.
    pub fn encoder(
        &self,
        output: &Path,
        kbps: u32,
        sample_rate: u32,
        channels: u16,
    ) -> Result<LameEncoder, Error> {
        let file = File::create(output)?;
        let spawned = Command::new(&self.path)
            .args(arguments(kbps, sample_rate, channels))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => {
                drop(file);
                let _ = std::fs::remove_file(output);
                return Err(error.into());
            }
        };
        let pipes = (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (Some(stdin), Some(mut stdout), Some(mut stderr)) = pipes else {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(output);
            return Err(Error::Platform("lame's pipes did not open".into()));
        };
        // Both outputs are drained on their own threads, so lame never blocks on a full pipe
        // while Debussy blocks writing its input.
        let stdout = std::thread::spawn(move || -> std::io::Result<()> {
            let mut writer = BufWriter::new(file);
            std::io::copy(&mut stdout, &mut writer)?;
            writer.flush()
        });
        let stderr = std::thread::spawn(move || tail(&mut stderr, STDERR_LIMIT));
        Ok(LameEncoder {
            running: Some(Running {
                child,
                stdin: Some(stdin),
                stdout,
                stderr,
            }),
            output: output.to_owned(),
            bytes: Vec::new(),
        })
    }
}

/// Lame's arguments: raw signed little-endian 16-bit input at `sample_rate` with `channels`
/// channels, constant `kbps`, standard input to standard output.
fn arguments(kbps: u32, sample_rate: u32, channels: u16) -> Vec<String> {
    let mut args: Vec<String> = [
        "--quiet",
        "-r",
        "--signed",
        "--little-endian",
        "--bitwidth",
        "16",
        "-s",
    ]
    .iter()
    .map(|&s| s.to_owned())
    .collect();
    args.push(kilohertz(sample_rate));
    args.push("-m".to_owned());
    args.push(if channels == 1 { "m" } else { "j" }.to_owned());
    args.push("-b".to_owned());
    args.push(kbps.to_string());
    if sample_rate > MP3_MAX_RATE {
        let target = if sample_rate.is_multiple_of(44_100) {
            44_100
        } else {
            MP3_MAX_RATE
        };
        args.push("--resample".to_owned());
        args.push(kilohertz(target));
    }
    args.push("-".to_owned());
    args.push("-".to_owned());
    args
}

/// `rate` in kilohertz as lame reads it: `44.1`, `48`, `88.2`.
fn kilohertz(rate: u32) -> String {
    format!("{}", f64::from(rate) / 1000.0)
}

/// Whether `path` is a file that may be executed.
fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

/// Whether `path --version` exits successfully within [`PROBE_TIMEOUT`] and prints `LAME`.
fn answers_version(path: &Path) -> bool {
    let Ok(mut child) = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return false;
    };
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stdout.take(VERSION_LIMIT).read_to_end(&mut text);
        text
    });
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(PROBE_POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let text = reader.join().unwrap_or_default();
    status.is_some_and(|s| s.success()) && String::from_utf8_lossy(&text).contains("LAME")
}

/// The last `limit` bytes `reader` yields before it ends or fails.
fn tail(reader: &mut impl Read, limit: usize) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut chunk = [0u8; 1024];
    while let Ok(n) = reader.read(&mut chunk) {
        if n == 0 {
            break;
        }
        kept.extend_from_slice(chunk.get(..n).unwrap_or_default());
        if kept.len() > limit {
            kept.drain(..kept.len() - limit);
        }
    }
    kept
}

/// A started lame process and the threads draining it.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: JoinHandle<std::io::Result<()>>,
    stderr: JoinHandle<Vec<u8>>,
}

impl Running {
    /// Closes lame's input, waits for it to exit and for its outputs to be drained, and returns
    /// its exit status, whether its output reached the file, and its standard error.
    fn wait(mut self) -> (std::io::Result<ExitStatus>, std::io::Result<()>, String) {
        drop(self.stdin.take());
        let status = self.child.wait();
        let copied = self.stdout.join().unwrap_or_else(|_| {
            Err(std::io::Error::other(
                "the thread copying lame's output panicked",
            ))
        });
        let stderr = self.stderr.join().unwrap_or_default();
        (
            status,
            copied,
            String::from_utf8_lossy(&stderr).trim().to_owned(),
        )
    }

    /// Stops lame and waits for it, discarding what it produced.
    fn abort(mut self) -> String {
        let _ = self.child.kill();
        self.wait().2
    }
}

/// An MP3 encoder running lame; made by [`Lame::encoder`]. Dropped before
/// [`Encoder::finish`], or on any failure, it stops lame and removes the partial file.
pub struct LameEncoder {
    running: Option<Running>,
    output: PathBuf,
    /// Reused for each block's 16-bit samples.
    bytes: Vec<u8>,
}

impl LameEncoder {
    /// Stops lame, removes the partial file, and returns an error describing `what` with
    /// lame's standard error.
    fn fail(&mut self, what: String) -> Error {
        let stderr = self.running.take().map(Running::abort).unwrap_or_default();
        let _ = std::fs::remove_file(&self.output);
        Error::Platform(with_stderr(what, &stderr))
    }
}

/// `what`, followed by lame's `stderr` if it said anything.
fn with_stderr(what: String, stderr: &str) -> String {
    if stderr.is_empty() {
        what
    } else {
        format!("{what}: {stderr}")
    }
}

impl Encoder for LameEncoder {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), Error> {
        self.bytes.clear();
        // The samples are already dithered onto the 16-bit grid, so this only rescales them.
        self.bytes.extend(interleaved.iter().flat_map(|&s| {
            let step = if s.is_nan() {
                0.0
            } else {
                (s * 32_768.0).round().clamp(-32_768.0, 32_767.0)
            };
            (step as i16).to_le_bytes()
        }));
        let written = match self.running.as_mut().and_then(|r| r.stdin.as_mut()) {
            Some(stdin) => stdin.write_all(&self.bytes),
            None => return Err(Error::Platform("lame has already stopped".into())),
        };
        written.map_err(|error| self.fail(format!("lame stopped reading: {error}")))
    }

    fn finish(mut self: Box<Self>) -> Result<(), Error> {
        let Some(running) = self.running.take() else {
            return Err(Error::Platform("lame has already stopped".into()));
        };
        let (status, copied, stderr) = running.wait();
        let failure = match (status, copied) {
            (Ok(status), Ok(())) if status.success() => return Ok(()),
            (Ok(status), Ok(())) => Error::Platform(with_stderr(format!("lame {status}"), &stderr)),
            (Err(error), _) | (_, Err(error)) => Error::Io(error),
        };
        let _ = std::fs::remove_file(&self.output);
        Err(failure)
    }
}

impl Drop for LameEncoder {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.abort();
            let _ = std::fs::remove_file(&self.output);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::error::Error as StdError;
    use std::os::unix::fs::PermissionsExt;

    type TestResult = Result<(), Box<dyn StdError>>;

    fn scratch(name: &str) -> Result<PathBuf, Box<dyn StdError>> {
        let dir = std::env::temp_dir().join(format!("lame-{}-{name}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Writes an executable shell script named `lame` holding `body` into `dir`.
    fn fake(dir: &Path, body: &str) -> Result<PathBuf, Box<dyn StdError>> {
        let path = dir.join("lame");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        Ok(path)
    }

    #[test]
    fn test_probe_takes_the_first_candidate_that_says_lame() -> TestResult {
        let dir = scratch("probe")?;
        let liar = dir.join("liar");
        std::fs::create_dir_all(&liar)?;
        let liar = fake(&liar, "echo 'SOX version 14'")?;
        let real = fake(
            &dir,
            "echo 'LAME 64bits version 3.100 (http://lame.sf.net)'",
        )?;
        let missing = dir.join("missing").join("lame");
        let found = Lame::probe(&[missing, liar, real.clone()]);
        assert_eq!(found.map(|l| l.path().to_owned()), Some(real));
        Ok(())
    }

    #[test]
    fn test_probe_rejects_a_missing_failing_or_silent_binary() -> TestResult {
        let dir = scratch("probe-none")?;
        assert_eq!(Lame::probe(&[dir.join("absent")]), None);
        let failing = fake(&dir, "echo LAME; exit 1")?;
        assert_eq!(Lame::probe(&[failing]), None);
        let quiet = fake(&dir, "exit 0")?;
        assert_eq!(Lame::probe(std::slice::from_ref(&quiet)), None);
        // A file without execute permission is not run at all.
        std::fs::write(&quiet, "#!/bin/sh\necho LAME\n")?;
        std::fs::set_permissions(&quiet, std::fs::Permissions::from_mode(0o644))?;
        assert_eq!(Lame::probe(&[quiet]), None);
        Ok(())
    }

    #[test]
    fn test_encoder_pipes_16_bit_samples_through() -> TestResult {
        let dir = scratch("cat")?;
        let lame = Lame {
            path: fake(&dir, "exec cat")?,
        };
        let output = dir.join("mix.mp3");
        let mut encoder = Box::new(lame.encoder(&output, 320, 48_000, 2)?);
        let frames = 3000;
        let block: Vec<f32> = (0..2 * 100).map(|n| n as f32 / 32_768.0).collect();
        for _ in 0..frames / 100 {
            encoder.write(&block)?;
        }
        encoder.finish()?;
        let bytes = std::fs::read(&output)?;
        assert_eq!(bytes.len(), frames * 2 * 2);
        assert_eq!(bytes.get(..6), Some(&[0, 0, 1, 0, 2, 0][..]));
        Ok(())
    }

    #[test]
    fn test_encoder_full_scale_and_nan_are_clamped() -> TestResult {
        let dir = scratch("clamp")?;
        let lame = Lame {
            path: fake(&dir, "exec cat")?,
        };
        let output = dir.join("mix.mp3");
        let mut encoder = Box::new(lame.encoder(&output, 128, 44_100, 1)?);
        encoder.write(&[1.0, -1.0, f32::NAN])?;
        encoder.finish()?;
        let bytes = std::fs::read(&output)?;
        let want: Vec<u8> = [32_767i16, -32_768, 0]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        assert_eq!(bytes, want);
        Ok(())
    }

    #[test]
    fn test_encoder_passes_format_bitrate_and_resample() -> TestResult {
        let dir = scratch("args")?;
        let args = dir.join("args.txt");
        let lame = Lame {
            path: fake(
                &dir,
                &format!("printf '%s\\n' \"$@\" > '{}'; exec cat", args.display()),
            )?,
        };
        let output = dir.join("mix.mp3");
        let encoder = Box::new(lame.encoder(&output, 256, 96_000, 1)?);
        encoder.finish()?;
        let got = std::fs::read_to_string(&args)?;
        let want = "--quiet -r --signed --little-endian --bitwidth 16 -s 96 -m m -b 256 \
                    --resample 48 - -";
        assert_eq!(got.split_whitespace().collect::<Vec<_>>().join(" "), want);
        assert_eq!(
            arguments(320, 88_200, 2).get(7..),
            Some(
                &[
                    "88.2",
                    "-m",
                    "j",
                    "-b",
                    "320",
                    "--resample",
                    "44.1",
                    "-",
                    "-"
                ]
                .map(String::from)[..]
            )
        );
        assert!(!arguments(320, 48_000, 2).contains(&"--resample".to_owned()));
        Ok(())
    }

    #[test]
    fn test_encoder_failure_reports_stderr_and_removes_the_file() -> TestResult {
        let dir = scratch("fail")?;
        let lame = Lame {
            path: fake(
                &dir,
                "cat > /dev/null; echo 'Unsupported data format' >&2; exit 3",
            )?,
        };
        let output = dir.join("mix.mp3");
        let mut encoder = Box::new(lame.encoder(&output, 320, 48_000, 2)?);
        assert!(output.exists());
        encoder.write(&[0.0; 512])?;
        let message = encoder.finish().err().ok_or("lame failed")?.to_string();
        assert!(message.contains("Unsupported data format"), "{message}");
        assert!(!output.exists());
        Ok(())
    }

    #[test]
    fn test_encoder_that_stops_reading_fails_the_write() -> TestResult {
        let dir = scratch("early")?;
        let lame = Lame {
            path: fake(&dir, "echo 'out of memory' >&2; exit 1")?,
        };
        let output = dir.join("mix.mp3");
        let mut encoder = Box::new(lame.encoder(&output, 320, 48_000, 2)?);
        // Lame's input pipe fills and breaks long before this much is written.
        let block = vec![0.0f32; 65_536];
        let mut result = Ok(());
        for _ in 0..64 {
            result = encoder.write(&block);
            if result.is_err() {
                break;
            }
        }
        let message = match result {
            Err(error) => error.to_string(),
            Ok(()) => encoder.finish().err().ok_or("lame failed")?.to_string(),
        };
        assert!(message.contains("out of memory"), "{message}");
        assert!(!output.exists());
        Ok(())
    }

    #[test]
    fn test_encoder_dropped_unfinished_removes_the_file() -> TestResult {
        let dir = scratch("drop")?;
        let lame = Lame {
            path: fake(&dir, "exec cat")?,
        };
        let output = dir.join("mix.mp3");
        let mut encoder = lame.encoder(&output, 320, 48_000, 2)?;
        encoder.write(&[0.5; 64])?;
        drop(encoder);
        assert!(!output.exists());
        Ok(())
    }
}
