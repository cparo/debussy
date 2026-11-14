//! The MP3 bit rate dialog, shown after an `.mp3` export path is chosen
//! (`docs/architecture/user-interface.md` "Files and devices", ADR-0129).

use std::path::PathBuf;

use iced::widget::{Space, column, container, row, text};
use iced::{Element, Length};

use crate::app::Message;
use crate::node_dialog::DialogMessage;
use crate::theme;
use crate::view::{Face, tool};

/// The MP3 bit rates offered, in kilobits per second, lowest first.
pub const MP3_BITRATES: [u32; 3] = [128, 256, 320];
/// The buttons' labels, one per bit rate.
const LABELS: [&str; 3] = ["128 kbit/s", "256 kbit/s", "320 kbit/s"];
/// The focus index of the Cancel button, after the bit rates.
const CANCEL: usize = MP3_BITRATES.len();
/// The button focused when the dialog opens: the highest bit rate.
const DEFAULT_FOCUS: usize = MP3_BITRATES.len() - 1;
/// The dialog's width, in logical pixels.
const WIDTH: f32 = 380.0;

/// An MP3 export waiting for its bit rate.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExportDialog {
    /// Where the export goes.
    pub(crate) path: PathBuf,
    /// Whether to export the mono mix.
    pub(crate) mono: bool,
    /// The focused button: a bit rate's index, or [`CANCEL`]. What Enter or Space presses.
    focus: usize,
}

/// What a key message did to the dialog.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Key {
    /// Not a key the dialog takes; handle the message as usual.
    Ignored,
    /// The dialog took it (a focus move); nothing else happens.
    Taken,
    /// The dialog took it and pressed a button: handle this message instead.
    Press(Message),
}

impl ExportDialog {
    /// The dialog for exporting to `path`, as the mono mix if `mono`, with the highest bit rate
    /// focused.
    pub(crate) fn new(path: PathBuf, mono: bool) -> ExportDialog {
        ExportDialog {
            path,
            mono,
            focus: DEFAULT_FOCUS,
        }
    }

    /// Takes the keyboard's messages while the dialog is open: Tab and Shift-Tab move the
    /// focus, Enter or Space presses the focused button, and Escape cancels.
    pub(crate) fn key(&mut self, message: &Message) -> Key {
        let buttons = CANCEL + 1;
        match message {
            Message::ClearRange => Key::Press(Message::CancelExport),
            Message::Dialog(DialogMessage::FocusNext) => {
                self.focus = (self.focus + 1) % buttons;
                Key::Taken
            }
            Message::Dialog(DialogMessage::FocusPrevious) => {
                self.focus = (self.focus + buttons - 1) % buttons;
                Key::Taken
            }
            Message::Dialog(DialogMessage::FocusPress) => Key::Press(
                MP3_BITRATES
                    .get(self.focus)
                    .map_or(Message::CancelExport, |&kbps| Message::ExportMp3(kbps)),
            ),
            Message::Dialog(m) if m.is_focus_key() => Key::Taken,
            _ => Key::Ignored,
        }
    }

    /// The dialog: a button per bit rate, and Cancel. The focused button is lit.
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let mut buttons = row![Space::new().width(Length::Fill)].spacing(6);
        buttons = buttons.push(tool(
            Face::Label("Cancel"),
            Some(Message::CancelExport),
            self.focus == CANCEL,
        ));
        for (index, (&kbps, &label)) in MP3_BITRATES.iter().zip(&LABELS).enumerate() {
            buttons = buttons.push(tool(
                Face::Label(label),
                Some(Message::ExportMp3(kbps)),
                self.focus == index,
            ));
        }
        container(column![text("MP3 bit rate").size(14), buttons].spacing(14))
            .padding(16)
            .width(Length::Fixed(WIDTH))
            .style(theme::popup)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_presses_320_by_default_and_tab_moves_the_focus() {
        let mut dialog = ExportDialog::new(PathBuf::from("mix.mp3"), false);
        let press = Message::Dialog(DialogMessage::FocusPress);
        assert_eq!(dialog.key(&press), Key::Press(Message::ExportMp3(320)));
        assert_eq!(
            dialog.key(&Message::Dialog(DialogMessage::FocusNext)),
            Key::Taken
        );
        assert_eq!(dialog.key(&press), Key::Press(Message::CancelExport));
        // Tab wraps round to the first bit rate; Shift-Tab back to Cancel.
        let _ = dialog.key(&Message::Dialog(DialogMessage::FocusNext));
        assert_eq!(dialog.key(&press), Key::Press(Message::ExportMp3(128)));
        let _ = dialog.key(&Message::Dialog(DialogMessage::FocusPrevious));
        assert_eq!(dialog.key(&press), Key::Press(Message::CancelExport));
    }

    #[test]
    fn test_key_escape_cancels_and_other_messages_pass() {
        let mut dialog = ExportDialog::new(PathBuf::from("mix.mp3"), true);
        assert_eq!(
            dialog.key(&Message::ClearRange),
            Key::Press(Message::CancelExport)
        );
        assert_eq!(dialog.key(&Message::Tick), Key::Ignored);
    }
}
