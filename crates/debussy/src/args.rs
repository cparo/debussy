//! The command line.

use std::path::PathBuf;

use anyhow::{Result, bail};
use import_export::Mp3Bitrate;

/// What the user asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    /// The project bundle to open, created if missing; `None` opens the start screen.
    pub project: Option<PathBuf>,
    /// The sample rate for a new project.
    pub sample_rate: u32,
    /// Files to import as new tracks before opening.
    pub imports: Vec<PathBuf>,
    /// Export the mix to this file and exit instead of opening the window.
    pub export: Option<PathBuf>,
    /// Export the mono mix.
    pub mono: bool,
    /// The bit rate of an MP3 export, if given; 320 kbit/s if not.
    pub bitrate: Option<Mp3Bitrate>,
}

/// The usage text.
pub const USAGE: &str = "\
usage: debussy [PROJECT.debussy [--rate HZ] [--import FILE]...
                [--export FILE [--mono] [--bitrate KBPS]]]

  With no project, opens the start screen to create or open one.
  Opens PROJECT.debussy, creating it at --rate (default 48000) if it does not exist.
  --import FILE   import WAV, FLAC, AAC or MP3 as a new track at the start (repeatable)
  --export FILE   render the whole project to FILE (.wav 24-bit, .flac 24-bit, .m4a AAC,
                  .mp3 if LAME is installed: `brew install lame`) and exit without opening
                  the window
  --mono          with --export, write the mono mix
  --bitrate KBPS  with an .mp3 --export, 128, 256 or 320 (the default)";

/// Parses the arguments after the program name.
///
/// # Errors
///
/// A message naming the bad argument.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut project = None;
    let mut sample_rate = 48_000;
    let mut imports = Vec::new();
    let mut export = None;
    let mut mono = false;
    let mut bitrate = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| match args.next() {
            Some(v) => Ok(v),
            None => bail!("{name} needs a value\n\n{USAGE}"),
        };
        match arg.as_str() {
            "--rate" => sample_rate = value("--rate")?.parse()?,
            "--import" => imports.push(PathBuf::from(value("--import")?)),
            "--export" => export = Some(PathBuf::from(value("--export")?)),
            "--mono" => mono = true,
            "--bitrate" => {
                let kbps = value("--bitrate")?;
                match kbps.parse().ok().and_then(Mp3Bitrate::from_kbps) {
                    Some(b) => bitrate = Some(b),
                    None => bail!("--bitrate {kbps}: use 128, 256 or 320\n\n{USAGE}"),
                }
            }
            "-h" | "--help" => bail!("{USAGE}"),
            flag if flag.starts_with("--") => bail!("unknown option {flag}\n\n{USAGE}"),
            path if project.is_none() => project = Some(PathBuf::from(path)),
            extra => bail!("unexpected argument {extra}\n\n{USAGE}"),
        }
    }
    if project.is_none() && (!imports.is_empty() || export.is_some()) {
        bail!("--import and --export need a project\n\n{USAGE}");
    }
    let mp3 = export
        .as_ref()
        .and_then(|p| p.extension())
        .is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
    if bitrate.is_some() && !mp3 {
        bail!("--bitrate needs an .mp3 --export\n\n{USAGE}");
    }
    Ok(Args {
        project,
        sample_rate,
        imports,
        export,
        mono,
        bitrate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn test_parse_reads_every_option() -> Result<()> {
        let args = parse(strings(&[
            "Song.debussy",
            "--rate",
            "44100",
            "--import",
            "a.wav",
            "--import",
            "b.mp3",
            "--export",
            "mix.flac",
            "--mono",
        ]))?;
        assert_eq!(args.project, Some(PathBuf::from("Song.debussy")));
        assert_eq!(args.sample_rate, 44_100);
        assert_eq!(
            args.imports,
            vec![PathBuf::from("a.wav"), PathBuf::from("b.mp3")]
        );
        assert_eq!(args.export, Some(PathBuf::from("mix.flac")));
        assert!(args.mono);
        Ok(())
    }

    #[test]
    fn test_parse_refuses_bad_input() {
        for bad in [
            &["a", "b"][..],
            &["--import", "a.wav"],
            &["--export", "mix.wav"],
            &["a", "--rate"],
            &["a", "--bogus"],
            &["a", "--rate", "x"],
            &["a", "--export", "mix.mp3", "--bitrate", "192"],
            &["a", "--export", "mix.mp3", "--bitrate"],
            &["a", "--export", "mix.wav", "--bitrate", "320"],
            &["a", "--bitrate", "320"],
        ] {
            assert!(parse(strings(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn test_parse_bitrate_goes_with_an_mp3_export() -> Result<()> {
        let args = parse(strings(&["a", "--export", "Mix.MP3", "--bitrate", "128"]))?;
        assert_eq!(args.bitrate, Some(Mp3Bitrate::Kbps128));
        let args = parse(strings(&["a", "--export", "mix.mp3"]))?;
        assert_eq!(args.bitrate, None);
        Ok(())
    }

    #[test]
    fn test_parse_without_arguments_opens_the_start_screen() -> Result<()> {
        let args = parse(strings(&[]))?;
        assert_eq!(args.project, None);
        assert_eq!(args.sample_rate, 48_000);
        assert!(args.imports.is_empty() && args.export.is_none());
        Ok(())
    }
}
