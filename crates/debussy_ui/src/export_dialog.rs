//! The Export dialog: the format and its settings, chosen before the save dialog asks where
//! (`docs/architecture/user-interface.md` "Files and devices", ADR-0130).

use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::theme;
use crate::view::{self, Face, group, tool};

/// The AAC bit rates offered, in kilobits per second, lowest first.
pub const AAC_BITRATES: [u32; 3] = [128, 192, 256];
/// The MP3 bit rates offered, in kilobits per second, lowest first.
pub const MP3_BITRATES: [u32; 3] = [128, 256, 320];
/// The dialog's width, in logical pixels.
const WIDTH: f32 = 440.0;

/// An export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// WAV, written in-house.
    Wav,
    /// FLAC.
    Flac,
    /// AAC, in an `.m4a` file.
    Aac,
    /// MP3, offered only when the host has LAME (ADR-0129).
    Mp3,
}

impl ExportKind {
    /// The file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            ExportKind::Wav => "wav",
            ExportKind::Flac => "flac",
            ExportKind::Aac => "m4a",
            ExportKind::Mp3 => "mp3",
        }
    }

    /// The name shown for it.
    pub fn label(self) -> &'static str {
        match self {
            ExportKind::Wav => "WAV",
            ExportKind::Flac => "FLAC",
            ExportKind::Aac => "AAC",
            ExportKind::Mp3 => "MP3",
        }
    }
}

/// A WAV or FLAC export's sample format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportDepth {
    /// 16-bit integer, dithered.
    Int16,
    /// 24-bit integer, dithered.
    Int24,
    /// 32-bit float; WAV only.
    Float32,
}

/// What to export: the format, every format's own setting, and the channels. Switching format
/// keeps the others' settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportSettings {
    /// The format.
    pub format: ExportKind,
    /// WAV's sample format.
    pub wav_depth: ExportDepth,
    /// FLAC's sample format: 16- or 24-bit.
    pub flac_depth: ExportDepth,
    /// AAC's bit rate: one of [`AAC_BITRATES`].
    pub aac_kbps: u32,
    /// MP3's bit rate: one of [`MP3_BITRATES`].
    pub mp3_kbps: u32,
    /// Whether to export the mono mix, `(L + R) × 0.7071`.
    pub mono: bool,
}

impl Default for ExportSettings {
    /// 24-bit stereo WAV; 24-bit FLAC, 256 kbit/s AAC and 320 kbit/s MP3 when picked.
    fn default() -> Self {
        ExportSettings {
            format: ExportKind::Wav,
            wav_depth: ExportDepth::Int24,
            flac_depth: ExportDepth::Int24,
            aac_kbps: 256,
            mp3_kbps: 320,
            mono: false,
        }
    }
}

impl ExportSettings {
    /// These settings with anything not offered put back to its default: MP3 when `mp3` is
    /// false, a 32-bit float FLAC, or a bit rate not on the lists.
    pub fn offered(self, mp3: bool) -> ExportSettings {
        let defaults = ExportSettings::default();
        ExportSettings {
            format: if self.format == ExportKind::Mp3 && !mp3 {
                defaults.format
            } else {
                self.format
            },
            flac_depth: if self.flac_depth == ExportDepth::Float32 {
                defaults.flac_depth
            } else {
                self.flac_depth
            },
            aac_kbps: if AAC_BITRATES.contains(&self.aac_kbps) {
                self.aac_kbps
            } else {
                defaults.aac_kbps
            },
            mp3_kbps: if MP3_BITRATES.contains(&self.mp3_kbps) {
                self.mp3_kbps
            } else {
                defaults.mp3_kbps
            },
            ..self
        }
    }
}

/// The open Export dialog.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExportDialog {
    /// The settings being chosen.
    pub(crate) settings: ExportSettings,
    /// Whether MP3 is offered.
    mp3: bool,
}

/// What a key message did to the dialog.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Key {
    /// Not a key the dialog takes; handle the message as usual.
    Ignored,
    /// The dialog took it; nothing else happens.
    Taken,
    /// The dialog took it and pressed a button: handle this message instead.
    Press(Message),
}

impl ExportDialog {
    /// The dialog, opening on `settings`, offering MP3 if `mp3`.
    pub(crate) fn new(settings: ExportSettings, mp3: bool) -> ExportDialog {
        ExportDialog {
            settings: settings.offered(mp3),
            mp3,
        }
    }

    /// Takes the keyboard while the dialog is open: Enter presses Export…, Escape cancels, and
    /// the other focus keys do nothing.
    pub(crate) fn key(&self, message: &Message) -> Key {
        match message {
            Message::ClearRange => Key::Press(Message::CancelExport),
            Message::Dialog(crate::node_dialog::DialogMessage::FocusPress) => {
                Key::Press(Message::ConfirmExport)
            }
            Message::Dialog(m) if m.is_focus_key() => Key::Taken,
            _ => Key::Ignored,
        }
    }

    /// Changes the settings to `settings`, as far as they are offered.
    pub(crate) fn set(&mut self, settings: ExportSettings) {
        self.settings = settings.offered(self.mp3);
    }

    /// The dialog: a row each for the format, its setting and the channels, then Cancel and
    /// Export….
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let s = self.settings;
        let choose = |label: &'static str, changed: ExportSettings, lit: bool| {
            (
                Face::Label(label),
                Some(Message::ExportSettings(changed)),
                lit,
            )
        };
        let kinds = [
            ExportKind::Wav,
            ExportKind::Flac,
            ExportKind::Aac,
            ExportKind::Mp3,
        ];
        let formats = kinds
            .into_iter()
            .filter(|&k| k != ExportKind::Mp3 || self.mp3)
            .map(|k| choose(k.label(), ExportSettings { format: k, ..s }, s.format == k))
            .collect();
        let (setting, options): (&str, Vec<_>) = match s.format {
            ExportKind::Wav => (
                "Bit depth",
                [
                    (ExportDepth::Int16, "16-bit"),
                    (ExportDepth::Int24, "24-bit"),
                    (ExportDepth::Float32, "32-bit float"),
                ]
                .into_iter()
                .map(|(d, label)| {
                    choose(
                        label,
                        ExportSettings { wav_depth: d, ..s },
                        s.wav_depth == d,
                    )
                })
                .collect(),
            ),
            ExportKind::Flac => (
                "Bit depth",
                [
                    (ExportDepth::Int16, "16-bit"),
                    (ExportDepth::Int24, "24-bit"),
                ]
                .into_iter()
                .map(|(d, label)| {
                    choose(
                        label,
                        ExportSettings { flac_depth: d, ..s },
                        s.flac_depth == d,
                    )
                })
                .collect(),
            ),
            ExportKind::Aac => (
                "Bit rate",
                AAC_BITRATES
                    .into_iter()
                    .zip(["128 kbit/s", "192 kbit/s", "256 kbit/s"])
                    .map(|(k, label)| {
                        choose(label, ExportSettings { aac_kbps: k, ..s }, s.aac_kbps == k)
                    })
                    .collect(),
            ),
            ExportKind::Mp3 => (
                "Bit rate",
                MP3_BITRATES
                    .into_iter()
                    .zip(["128 kbit/s", "256 kbit/s", "320 kbit/s"])
                    .map(|(k, label)| {
                        choose(label, ExportSettings { mp3_kbps: k, ..s }, s.mp3_kbps == k)
                    })
                    .collect(),
            ),
        };
        let channels = vec![
            choose("Stereo", ExportSettings { mono: false, ..s }, !s.mono),
            choose("Mono", ExportSettings { mono: true, ..s }, s.mono),
        ];
        let line = |label: &'static str, buttons| {
            row![
                text(label).size(view::TOOL_TEXT),
                Space::new().width(Length::Fill),
                group(buttons),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
        };
        let buttons = row![
            Space::new().width(Length::Fill),
            tool(Face::Label("Cancel"), Some(Message::CancelExport), false),
            tool(Face::Label("Export…"), Some(Message::ConfirmExport), true),
        ]
        .spacing(6);
        let body = column![
            text("Export mix").size(18),
            line("Format", formats),
            line(setting, options),
            line("Channels", channels),
            buttons,
        ]
        .spacing(16);
        container(body)
            .padding(16)
            .width(Length::Fixed(WIDTH))
            .style(theme::popup)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_dialog::DialogMessage;

    #[test]
    fn test_offered_drops_what_is_not_offered() {
        let odd = ExportSettings {
            format: ExportKind::Mp3,
            flac_depth: ExportDepth::Float32,
            aac_kbps: 320,
            mp3_kbps: 192,
            ..ExportSettings::default()
        };
        assert_eq!(
            odd.offered(false),
            ExportSettings {
                format: ExportKind::Wav,
                ..ExportSettings::default()
            }
        );
        assert_eq!(odd.offered(true).format, ExportKind::Mp3);
    }

    #[test]
    fn test_key_enter_exports_and_escape_cancels() {
        let dialog = ExportDialog::new(ExportSettings::default(), true);
        assert_eq!(
            dialog.key(&Message::Dialog(DialogMessage::FocusPress)),
            Key::Press(Message::ConfirmExport)
        );
        assert_eq!(
            dialog.key(&Message::ClearRange),
            Key::Press(Message::CancelExport)
        );
        assert_eq!(
            dialog.key(&Message::Dialog(DialogMessage::FocusNext)),
            Key::Taken
        );
        assert_eq!(dialog.key(&Message::Tick), Key::Ignored);
    }

    #[test]
    fn test_set_keeps_each_formats_setting() {
        let mut dialog = ExportDialog::new(ExportSettings::default(), false);
        dialog.set(ExportSettings {
            format: ExportKind::Flac,
            flac_depth: ExportDepth::Int16,
            ..dialog.settings
        });
        dialog.set(ExportSettings {
            format: ExportKind::Mp3,
            ..dialog.settings
        });
        // MP3 is not offered, so the format goes back to the default; FLAC keeps its depth.
        assert_eq!(dialog.settings.format, ExportKind::Wav);
        assert_eq!(dialog.settings.flac_depth, ExportDepth::Int16);
    }
}
