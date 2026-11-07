//! [`AppSettings`]: the application's own settings, kept per machine outside any project
//! (`docs/architecture/project.md` "Application settings", ADR-0059). Read as untrusted input,
//! like a project file, and written crash-safely.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The settings format version this build writes.
pub const SETTINGS_VERSION: u32 = 1;

/// The largest settings file read, in bytes.
const MAX_BYTES: u64 = 64 * 1024;

/// The longest device id kept, in bytes.
const MAX_DEVICE_BYTES: usize = 1024;

/// What the application remembers between launches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppSettings {
    /// The audio device last chosen, by its platform id; `None` for the system default.
    pub device: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    format_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    device: Option<String>,
}

impl AppSettings {
    /// The settings at `path`. A missing, unreadable, newer or invalid file gives the defaults:
    /// settings are a convenience, and losing them must not stop the application starting.
    pub fn load(path: &Path) -> AppSettings {
        Self::try_load(path).unwrap_or_default()
    }

    fn try_load(path: &Path) -> Option<AppSettings> {
        let mut text = String::new();
        File::open(path)
            .ok()?
            .take(MAX_BYTES + 1)
            .read_to_string(&mut text)
            .ok()?;
        if text.len() as u64 > MAX_BYTES {
            return None;
        }
        let saved: Saved = serde_json::from_str(&text).ok()?;
        if saved.format_version != SETTINGS_VERSION {
            return None;
        }
        let device = saved
            .device
            .filter(|d| !d.is_empty() && d.len() <= MAX_DEVICE_BYTES);
        Some(AppSettings { device })
    }

    /// Writes the settings to `path`, making its directory if needed: to `<path>.tmp`, synced,
    /// then renamed over the old file.
    ///
    /// # Errors
    ///
    /// [`Error::Io`], [`Error::Json`].
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let io = |source| Error::Io {
            path: path.to_owned(),
            source,
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(io)?;
        }
        let saved = Saved {
            format_version: SETTINGS_VERSION,
            device: self.device.clone(),
        };
        let text = serde_json::to_vec_pretty(&saved).map_err(|source| Error::Json {
            path: path.to_owned(),
            source,
        })?;
        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        let mut file = File::create(&temp).map_err(io)?;
        file.write_all(&text).map_err(io)?;
        file.sync_all().map_err(io)?;
        drop(file);
        fs::rename(&temp, path).map_err(io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_round_trip_and_fall_back_to_defaults() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = std::env::temp_dir().join(format!("debussy-settings-{}", std::process::id()));
        let path = dir.join("nested").join("settings.json");
        assert_eq!(AppSettings::load(&path), AppSettings::default());
        let settings = AppSettings {
            device: Some("CoreAudio:BuiltInSpeakerDevice".to_owned()),
        };
        settings.save(&path)?;
        assert_eq!(AppSettings::load(&path), settings);
        for bad in [
            "not json",
            r#"{"format_version":2,"device":"x"}"#,
            r#"{"format_version":1,"device":"x","extra":1}"#,
        ] {
            std::fs::write(&path, bad)?;
            assert_eq!(AppSettings::load(&path), AppSettings::default(), "{bad}");
        }
        std::fs::write(&path, " ".repeat(MAX_BYTES as usize + 1))?;
        assert_eq!(AppSettings::load(&path), AppSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
