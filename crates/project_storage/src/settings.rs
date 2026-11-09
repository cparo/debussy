//! [`AppSettings`]: the application's own settings, kept per machine outside any project
//! (`docs/architecture/project.md` "Application settings", ADR-0059). Read as untrusted input,
//! like a project file, and written crash-safely.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The settings format version this build writes. Version 1 files are read too, with the click
/// preferences at their defaults (ADR-0083).
pub const SETTINGS_VERSION: u32 = 2;

/// The quietest click level, in decibels.
pub const MIN_CLICK_LEVEL_DB: f32 = -24.0;
/// The loudest click level, in decibels.
pub const MAX_CLICK_LEVEL_DB: f32 = 6.0;

/// The largest settings file read, in bytes.
const MAX_BYTES: u64 = 64 * 1024;

/// The longest device id kept, in bytes.
const MAX_DEVICE_BYTES: usize = 1024;

/// The click's sound (`docs/architecture/recording.md` "Click track").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickSound {
    /// A wood block.
    Block,
    /// A click.
    #[default]
    Click,
    /// A drumstick.
    Stick,
}

/// What the application remembers between launches.
#[derive(Debug, Clone, PartialEq)]
pub struct AppSettings {
    /// The audio device last chosen, by its platform id; `None` for the system default.
    pub device: Option<String>,
    /// The click's sound.
    pub click_sound: ClickSound,
    /// The click's level, in decibels: [`MIN_CLICK_LEVEL_DB`] to [`MAX_CLICK_LEVEL_DB`].
    pub click_level_db: f32,
}

impl Default for AppSettings {
    /// The system default device, and the Click sound at 0 dB.
    fn default() -> Self {
        AppSettings {
            device: None,
            click_sound: ClickSound::default(),
            click_level_db: 0.0,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    format_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    device: Option<String>,
    /// Since version 2.
    #[serde(default)]
    click_sound: Option<ClickSound>,
    /// Since version 2.
    #[serde(default)]
    click_level_db: Option<f32>,
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
        let defaults = AppSettings::default();
        let (click_sound, click_level_db) = match saved.format_version {
            // Version 1 had no click preferences.
            1 if saved.click_sound.is_none() && saved.click_level_db.is_none() => {
                (defaults.click_sound, defaults.click_level_db)
            }
            SETTINGS_VERSION => (saved.click_sound?, saved.click_level_db?),
            _ => return None,
        };
        let level_ok = click_level_db.is_finite()
            && (MIN_CLICK_LEVEL_DB..=MAX_CLICK_LEVEL_DB).contains(&click_level_db);
        let device = saved
            .device
            .filter(|d| !d.is_empty() && d.len() <= MAX_DEVICE_BYTES);
        Some(AppSettings {
            device,
            click_sound,
            click_level_db: if level_ok {
                click_level_db
            } else {
                defaults.click_level_db
            },
        })
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
            click_sound: Some(self.click_sound),
            click_level_db: Some(self.click_level_db),
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
            click_sound: ClickSound::Stick,
            click_level_db: -7.5,
        };
        settings.save(&path)?;
        assert_eq!(AppSettings::load(&path), settings);
        // Version 1 keeps its device and takes the default click.
        std::fs::write(&path, r#"{"format_version":1,"device":"x"}"#)?;
        let migrated = AppSettings::load(&path);
        assert_eq!(migrated.device.as_deref(), Some("x"));
        assert_eq!(
            (migrated.click_sound, migrated.click_level_db),
            (ClickSound::Click, 0.0)
        );
        // A level out of range falls back to 0 dB, keeping the rest.
        std::fs::write(
            &path,
            r#"{"format_version":2,"click_sound":"block","click_level_db":40.0}"#,
        )?;
        let loud = AppSettings::load(&path);
        assert_eq!(
            (loud.click_sound, loud.click_level_db),
            (ClickSound::Block, 0.0)
        );
        for bad in [
            "not json",
            r#"{"format_version":3,"device":"x"}"#,
            r#"{"format_version":2,"device":"x"}"#,
            r#"{"format_version":2,"click_sound":"bell","click_level_db":0.0}"#,
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
