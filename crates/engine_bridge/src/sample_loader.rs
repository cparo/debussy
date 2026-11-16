//! [`Samples`]: a sampler's zone files read whole into memory off the audio thread
//! (`docs/architecture/instruments.md` "Sampler", ADR-0154).

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

use project_model::MAX_ZONE_SECONDS;
use wav_files::{Channels, WavReader};

/// Zone files read from a bundle's `audio/` directory, cached by file name. Files there never
/// change once written, so a cached file is never reread.
#[derive(Debug, Default)]
pub struct Samples {
    /// The bundle's `audio/` directory; `None` reads nothing and leaves every zone silent.
    dir: Option<PathBuf>,
    cache: HashMap<String, Arc<[f32]>>,
    warnings: Vec<String>,
}

impl Samples {
    /// Reads zone files from `dir`, a bundle's `audio/` directory.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Samples {
            dir: Some(dir.into()),
            ..Samples::default()
        }
    }

    /// Reads nothing: every zone is silent, without a warning.
    pub fn none() -> Self {
        Samples::default()
    }

    /// New samples reading from the same directory as these, with nothing cached yet.
    pub fn same_directory(&self) -> Self {
        Samples {
            dir: self.dir.clone(),
            ..Samples::default()
        }
    }

    /// The samples of mono file `name` at `rate` Hz, read once; `None` with a warning kept for
    /// [`Samples::take_warnings`] if it is missing, unreadable, not mono or at another rate.
    /// Reads the file and allocates: call it off the audio thread.
    pub(crate) fn load(&mut self, name: &str, rate: u32) -> Option<Arc<[f32]>> {
        if let Some(samples) = self.cache.get(name) {
            return Some(Arc::clone(samples));
        }
        let dir = self.dir.as_ref()?;
        match read(dir.join(name), rate) {
            Ok(samples) => {
                self.cache.insert(name.to_owned(), Arc::clone(&samples));
                Some(samples)
            }
            Err(reason) => {
                self.warnings
                    .push(format!("sample {name:?} is silent: {reason}"));
                None
            }
        }
    }

    /// Forgets every cached file not in `used`.
    pub(crate) fn retain(&mut self, used: &BTreeSet<&str>) {
        self.cache.retain(|name, _| used.contains(name.as_str()));
    }

    /// The warnings gathered since the last call, oldest first.
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/// Every frame of the mono WAV at `path`, which must be at `rate` Hz, cut to
/// [`MAX_ZONE_SECONDS`].
fn read(path: PathBuf, rate: u32) -> Result<Arc<[f32]>, String> {
    let file = File::open(&path).map_err(|e| e.to_string())?;
    let mut reader = WavReader::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if spec.channels != Channels::Mono {
        return Err("the file is not mono".to_owned());
    }
    if spec.sample_rate != rate {
        return Err(format!(
            "the file is at {} Hz, not the project's {rate} Hz",
            spec.sample_rate
        ));
    }
    let max = u64::from(MAX_ZONE_SECONDS) * u64::from(rate);
    let frames = usize::try_from(reader.frames().min(max)).map_err(|e| e.to_string())?;
    let mut samples = vec![0.0; frames];
    let mut done = 0;
    while done < frames {
        let read = reader
            .read(&mut [&mut samples[done..]])
            .map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        done += read;
    }
    samples.truncate(done);
    Ok(samples.into())
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::path::Path;

    use wav_files::{SampleFormat, Spec, WavWriter};

    use super::*;

    fn scratch(name: &str) -> Result<PathBuf, Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!(
            "engine-bridge-samples-{}-{name}",
            std::process::id()
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn write(path: &Path, channels: &[&[f32]], rate: u32) -> Result<(), Box<dyn Error>> {
        let spec = Spec {
            channels: if channels.len() == 2 {
                Channels::Stereo
            } else {
                Channels::Mono
            },
            sample_rate: rate,
            format: SampleFormat::Float32,
        };
        let mut writer = WavWriter::new(File::create(path)?, spec)?;
        writer.write(channels)?;
        writer.finish()?;
        Ok(())
    }

    #[test]
    fn test_load_reads_once_and_caches() -> Result<(), Box<dyn Error>> {
        let dir = scratch("cache")?;
        write(&dir.join("a.wav"), &[&[0.5, -0.25, 0.125]], 48_000)?;
        let mut samples = Samples::new(&dir);
        let first = samples.load("a.wav", 48_000).ok_or("not loaded")?;
        assert_eq!(&*first, &[0.5, -0.25, 0.125]);
        std::fs::remove_file(dir.join("a.wav"))?;
        let again = samples.load("a.wav", 48_000).ok_or("not cached")?;
        assert!(Arc::ptr_eq(&first, &again));
        samples.retain(&BTreeSet::new());
        assert!(samples.load("a.wav", 48_000).is_none());
        Ok(())
    }

    #[test]
    fn test_load_warns_for_missing_stereo_and_wrong_rate() -> Result<(), Box<dyn Error>> {
        let dir = scratch("warn")?;
        write(&dir.join("stereo.wav"), &[&[0.1], &[0.2]], 48_000)?;
        write(&dir.join("rate.wav"), &[&[0.1]], 44_100)?;
        let mut samples = Samples::new(&dir);
        for name in ["missing.wav", "stereo.wav", "rate.wav"] {
            assert!(samples.load(name, 48_000).is_none(), "{name}");
        }
        assert_eq!(samples.take_warnings().len(), 3);
        assert!(samples.take_warnings().is_empty());
        assert!(Samples::none().load("missing.wav", 48_000).is_none());
        Ok(())
    }
}
