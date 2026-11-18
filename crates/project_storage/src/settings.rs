//! [`AppSettings`]: the application's own settings, kept per machine outside any project
//! (`docs/architecture/settings.md` "Application settings", ADR-0059). Read as untrusted input,
//! like a project file, and written crash-safely.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The settings format version this build writes. Version 1 files are read too, with the click
/// preferences at their defaults (ADR-0083), version 1 and 2 files with the export preferences at
/// theirs (ADR-0130), and version 1 to 3 files with no MIDI output settings (ADR-0162).
pub const SETTINGS_VERSION: u32 = 4;

/// The most MIDI output ports whose settings are kept; the rest are dropped on load.
pub const MAX_MIDI_OUTPUTS: usize = 64;

/// The largest MIDI output latency kept, in milliseconds; a larger or negative one is dropped.
pub const MAX_MIDI_LATENCY_MS: f32 = 1000.0;

/// The quietest click level, in decibels.
pub const MIN_CLICK_LEVEL_DB: f32 = -24.0;
/// The loudest click level, in decibels.
pub const MAX_CLICK_LEVEL_DB: f32 = 6.0;

/// The largest settings file read, in bytes.
const MAX_BYTES: u64 = 64 * 1024;

/// The longest device id kept, in bytes.
const MAX_DEVICE_BYTES: usize = 1024;

/// The highest bit rate kept, in kbit/s; a higher one reads as the default.
const MAX_KBPS: u32 = 1000;

/// An export format (`docs/architecture/import-export.md` "Formats").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormatName {
    /// WAV.
    #[default]
    Wav,
    /// FLAC.
    Flac,
    /// AAC in an `.m4a` file.
    Aac,
    /// MP3.
    Mp3,
}

/// An export's sample format, for WAV and FLAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportDepth {
    /// 16-bit integer.
    Int16,
    /// 24-bit integer.
    Int24,
    /// 32-bit float.
    Float32,
}

/// The last export's settings, each format keeping its own (ADR-0130).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPreferences {
    /// The format.
    pub format: ExportFormatName,
    /// WAV's sample format.
    pub wav_depth: ExportDepth,
    /// FLAC's sample format; never [`ExportDepth::Float32`].
    pub flac_depth: ExportDepth,
    /// AAC's bit rate, in kbit/s.
    pub aac_kbps: u32,
    /// MP3's bit rate, in kbit/s.
    pub mp3_kbps: u32,
    /// Whether to export the mono mix.
    pub mono: bool,
}

impl Default for ExportPreferences {
    /// 24-bit stereo WAV; 24-bit FLAC, 256 kbit/s AAC and 320 kbit/s MP3 when picked.
    fn default() -> Self {
        ExportPreferences {
            format: ExportFormatName::Wav,
            wav_depth: ExportDepth::Int24,
            flac_depth: ExportDepth::Int24,
            aac_kbps: 256,
            mp3_kbps: 320,
            mono: false,
        }
    }
}

impl ExportPreferences {
    /// These preferences with each field a reader cannot use put back to its default: a FLAC
    /// depth of 32-bit float, or a bit rate of 0 or above [`MAX_KBPS`].
    fn sanitized(self) -> ExportPreferences {
        let defaults = ExportPreferences::default();
        let kbps = |k: u32, default: u32| {
            if (1..=MAX_KBPS).contains(&k) {
                k
            } else {
                default
            }
        };
        ExportPreferences {
            flac_depth: if self.flac_depth == ExportDepth::Float32 {
                defaults.flac_depth
            } else {
                self.flac_depth
            },
            aac_kbps: kbps(self.aac_kbps, defaults.aac_kbps),
            mp3_kbps: kbps(self.mp3_kbps, defaults.mp3_kbps),
            ..self
        }
    }
}

/// One MIDI output port's settings (`docs/architecture/engine.md` "MIDI output").
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MidiOutSettings {
    /// How long the port and the instrument on it take to sound a note, in milliseconds: 0 to
    /// [`MAX_MIDI_LATENCY_MS`]. Notes are sent this much earlier.
    pub latency_ms: f32,
}

impl MidiOutSettings {
    /// Whether a reader can use it: a finite latency within range.
    fn is_valid(&self) -> bool {
        self.latency_ms.is_finite() && (0.0..=MAX_MIDI_LATENCY_MS).contains(&self.latency_ms)
    }
}

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
    /// The last export's settings.
    pub export: ExportPreferences,
    /// Each MIDI output port's settings, by endpoint id; at most [`MAX_MIDI_OUTPUTS`].
    pub midi_outputs: BTreeMap<String, MidiOutSettings>,
}

impl Default for AppSettings {
    /// The system default device, the Click sound at 0 dB, and the default export.
    fn default() -> Self {
        AppSettings {
            device: None,
            click_sound: ClickSound::default(),
            click_level_db: 0.0,
            export: ExportPreferences::default(),
            midi_outputs: BTreeMap::new(),
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
    /// Since version 3.
    #[serde(default)]
    export: Option<ExportPreferences>,
    /// Since version 4.
    #[serde(default)]
    midi_outputs: Option<BTreeMap<String, MidiOutSettings>>,
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
        // Version 1 had no click preferences, 1 and 2 no export preferences, and 1 to 3 no MIDI
        // output settings.
        let version = saved.format_version;
        if !(1..=SETTINGS_VERSION).contains(&version)
            || (version < 4 && saved.midi_outputs.is_some())
            || (version < 3 && saved.export.is_some())
        {
            return None;
        }
        let (click_sound, click_level_db) = match version {
            1 if saved.click_sound.is_none() && saved.click_level_db.is_none() => {
                (defaults.click_sound, defaults.click_level_db)
            }
            1 => return None,
            _ => (saved.click_sound?, saved.click_level_db?),
        };
        let export = match version {
            1 | 2 => defaults.export,
            _ => saved.export?.sanitized(),
        };
        let midi_outputs = match version {
            1..=3 => BTreeMap::new(),
            _ => sanitized_midi_outputs(saved.midi_outputs?),
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
            export,
            midi_outputs,
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
            export: Some(self.export),
            midi_outputs: Some(sanitized_midi_outputs(self.midi_outputs.clone())),
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

/// `outputs` without the entries a reader cannot use (an empty or overlong id, or an unusable
/// latency), and without those beyond the first [`MAX_MIDI_OUTPUTS`].
fn sanitized_midi_outputs(
    outputs: BTreeMap<String, MidiOutSettings>,
) -> BTreeMap<String, MidiOutSettings> {
    outputs
        .into_iter()
        .filter(|(id, settings)| {
            !id.is_empty() && id.len() <= MAX_DEVICE_BYTES && settings.is_valid()
        })
        .take(MAX_MIDI_OUTPUTS)
        .collect()
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
            export: ExportPreferences {
                format: ExportFormatName::Mp3,
                wav_depth: ExportDepth::Float32,
                flac_depth: ExportDepth::Int16,
                aac_kbps: 128,
                mp3_kbps: 256,
                mono: true,
            },
            midi_outputs: BTreeMap::from([(
                "CoreMIDI:42".to_owned(),
                MidiOutSettings { latency_ms: 4.5 },
            )]),
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
        // Version 2 keeps its click and takes the default export.
        std::fs::write(
            &path,
            r#"{"format_version":2,"click_sound":"block","click_level_db":-3.0}"#,
        )?;
        let two = AppSettings::load(&path);
        assert_eq!(
            (two.click_sound, two.export),
            (ClickSound::Block, ExportPreferences::default())
        );
        // Unusable export fields fall back one by one.
        std::fs::write(
            &path,
            r#"{"format_version":4,"click_sound":"click","click_level_db":0.0,"export":{"format":"flac","wav_depth":"int16","flac_depth":"float32","aac_kbps":0,"mp3_kbps":99999,"mono":true},"midi_outputs":{}}"#,
        )?;
        let odd = AppSettings::load(&path).export;
        assert_eq!(
            odd,
            ExportPreferences {
                format: ExportFormatName::Flac,
                wav_depth: ExportDepth::Int16,
                mono: true,
                ..ExportPreferences::default()
            }
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
            r#"{"format_version":5,"device":"x"}"#,
            r#"{"format_version":3,"click_sound":"click","click_level_db":0.0,"export":{"format":"wav","wav_depth":"int16","flac_depth":"int16","aac_kbps":1,"mp3_kbps":1,"mono":true},"midi_outputs":{}}"#,
            r#"{"format_version":4,"click_sound":"click","click_level_db":0.0,"export":{"format":"wav","wav_depth":"int16","flac_depth":"int16","aac_kbps":1,"mp3_kbps":1,"mono":true}}"#,
            r#"{"format_version":4,"click_sound":"click","click_level_db":0.0,"export":{"format":"wav","wav_depth":"int16","flac_depth":"int16","aac_kbps":1,"mp3_kbps":1,"mono":true},"midi_outputs":{"a":{"latency_ms":1.0,"x":2}}}"#,
            r#"{"format_version":2,"device":"x"}"#,
            r#"{"format_version":3,"click_sound":"click","click_level_db":0.0}"#,
            r#"{"format_version":2,"click_sound":"click","click_level_db":0.0,"export":{}}"#,
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

    #[test]
    fn test_settings_load_migrates_version_three_and_drops_bad_midi_outputs()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("debussy-midi-out-{}", std::process::id()));
        let path = dir.join("settings.json");
        std::fs::create_dir_all(&dir)?;
        let export = r#""export":{"format":"wav","wav_depth":"int16","flac_depth":"int16","aac_kbps":1,"mp3_kbps":1,"mono":true}"#;
        std::fs::write(
            &path,
            format!(
                r#"{{"format_version":3,"click_sound":"stick","click_level_db":0.0,{export}}}"#
            ),
        )?;
        let three = AppSettings::load(&path);
        assert_eq!(three.click_sound, ClickSound::Stick);
        assert!(three.midi_outputs.is_empty());
        let long = "x".repeat(MAX_DEVICE_BYTES + 1);
        let many: String = (0..MAX_MIDI_OUTPUTS + 5)
            .map(|i| format!(r#","p{i:03}":{{"latency_ms":1.0}}"#))
            .collect();
        std::fs::write(
            &path,
            format!(
                r#"{{"format_version":4,"click_sound":"stick","click_level_db":0.0,{export},"midi_outputs":{{"":{{"latency_ms":1.0}},"{long}":{{"latency_ms":1.0}},"a":{{"latency_ms":-1.0}},"b":{{"latency_ms":2000.0}},"c":{{"latency_ms":12.5}}{many}}}}}"#
            ),
        )?;
        let four = AppSettings::load(&path);
        assert_eq!(four.click_sound, ClickSound::Stick);
        assert_eq!(four.midi_outputs.len(), MAX_MIDI_OUTPUTS);
        assert_eq!(
            four.midi_outputs.get("c"),
            Some(&MidiOutSettings { latency_ms: 12.5 })
        );
        assert!(!four.midi_outputs.contains_key("a"));
        assert!(!four.midi_outputs.contains_key("b"));
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn test_settings_save_drops_unusable_midi_outputs() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("debussy-midi-save-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut settings = AppSettings::default();
        settings
            .midi_outputs
            .insert("ok".to_owned(), MidiOutSettings { latency_ms: 3.0 });
        settings.midi_outputs.insert(
            "nan".to_owned(),
            MidiOutSettings {
                latency_ms: f32::NAN,
            },
        );
        settings.save(&path)?;
        assert_eq!(
            AppSettings::load(&path)
                .midi_outputs
                .keys()
                .collect::<Vec<_>>(),
            ["ok"]
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
