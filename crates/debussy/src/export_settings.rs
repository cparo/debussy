//! The Export dialog's settings: as remembered in the app settings, and as the export format
//! they choose (ADR-0130).

use debussy_ui::{ExportDepth, ExportKind, ExportSettings};
use import_export::{ExportFormat, Mp3Bitrate, WavDepth};
use platform_interfaces::FlacDepth;
use project_storage::{ExportDepth as SavedDepth, ExportFormatName, ExportPreferences};

/// The dialog's settings from the remembered `saved` ones. The dialog drops any it does not
/// offer.
pub fn from_saved(saved: ExportPreferences) -> ExportSettings {
    let depth = |d| match d {
        SavedDepth::Int16 => ExportDepth::Int16,
        SavedDepth::Int24 => ExportDepth::Int24,
        SavedDepth::Float32 => ExportDepth::Float32,
    };
    ExportSettings {
        format: match saved.format {
            ExportFormatName::Wav => ExportKind::Wav,
            ExportFormatName::Flac => ExportKind::Flac,
            ExportFormatName::Aac => ExportKind::Aac,
            ExportFormatName::Mp3 => ExportKind::Mp3,
        },
        wav_depth: depth(saved.wav_depth),
        flac_depth: depth(saved.flac_depth),
        aac_kbps: saved.aac_kbps,
        mp3_kbps: saved.mp3_kbps,
        mono: saved.mono,
    }
}

/// How `settings` are remembered.
pub fn to_saved(settings: &ExportSettings) -> ExportPreferences {
    let depth = |d| match d {
        ExportDepth::Int16 => SavedDepth::Int16,
        ExportDepth::Int24 => SavedDepth::Int24,
        ExportDepth::Float32 => SavedDepth::Float32,
    };
    ExportPreferences {
        format: match settings.format {
            ExportKind::Wav => ExportFormatName::Wav,
            ExportKind::Flac => ExportFormatName::Flac,
            ExportKind::Aac => ExportFormatName::Aac,
            ExportKind::Mp3 => ExportFormatName::Mp3,
        },
        wav_depth: depth(settings.wav_depth),
        flac_depth: depth(settings.flac_depth),
        aac_kbps: settings.aac_kbps,
        mp3_kbps: settings.mp3_kbps,
        mono: settings.mono,
    }
}

/// The export format `settings` choose. A FLAC depth of 32-bit float, or an MP3 bit rate not
/// offered, falls back to the default (24-bit, 320 kbit/s).
pub fn to_format(settings: &ExportSettings) -> ExportFormat {
    match settings.format {
        ExportKind::Wav => ExportFormat::Wav(match settings.wav_depth {
            ExportDepth::Int16 => WavDepth::Int16,
            ExportDepth::Int24 => WavDepth::Int24,
            ExportDepth::Float32 => WavDepth::Float32,
        }),
        ExportKind::Flac => ExportFormat::Flac(match settings.flac_depth {
            ExportDepth::Int16 => FlacDepth::Int16,
            ExportDepth::Int24 | ExportDepth::Float32 => FlacDepth::Int24,
        }),
        ExportKind::Aac => ExportFormat::Aac {
            bitrate_kbps: settings.aac_kbps,
        },
        ExportKind::Mp3 => ExportFormat::Mp3 {
            bitrate: Mp3Bitrate::from_kbps(settings.mp3_kbps).unwrap_or(Mp3Bitrate::Kbps320),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_saved_and_back_round_trips() {
        let settings = ExportSettings {
            format: ExportKind::Mp3,
            wav_depth: ExportDepth::Float32,
            flac_depth: ExportDepth::Int16,
            aac_kbps: 192,
            mp3_kbps: 128,
            mono: true,
        };
        assert_eq!(from_saved(to_saved(&settings)), settings);
        assert_eq!(
            from_saved(ExportPreferences::default()),
            ExportSettings::default()
        );
    }

    #[test]
    fn test_to_format_picks_each_formats_setting() {
        let settings = ExportSettings {
            wav_depth: ExportDepth::Int16,
            aac_kbps: 128,
            mp3_kbps: 256,
            ..ExportSettings::default()
        };
        assert_eq!(to_format(&settings), ExportFormat::Wav(WavDepth::Int16));
        let with = |format| ExportSettings { format, ..settings };
        assert_eq!(
            to_format(&with(ExportKind::Flac)),
            ExportFormat::Flac(FlacDepth::Int24)
        );
        assert_eq!(
            to_format(&with(ExportKind::Aac)),
            ExportFormat::Aac { bitrate_kbps: 128 }
        );
        assert_eq!(
            to_format(&with(ExportKind::Mp3)),
            ExportFormat::Mp3 {
                bitrate: Mp3Bitrate::Kbps256
            }
        );
    }
}
