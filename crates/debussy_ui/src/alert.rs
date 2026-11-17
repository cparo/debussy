//! [`Alert`]: a modal message for something the user asked for that was refused, such as an
//! import, with one "OK" button (`docs/architecture/user-interface.md` "Files and devices").

use iced::widget::text::Wrapping;
use iced::widget::text_editor::{self, Content};
use iced::widget::{Space, column, row, text};
use iced::{Element, Length};

use crate::app::Message;
use crate::export_dialog::Key;
use crate::node_dialog::DialogMessage;
use crate::theme;
use crate::view::{Face, tool};

/// The alert's width, in logical pixels.
const WIDTH: f32 = 420.0;
/// The most items listed; the rest are counted.
pub(crate) const MAX_ITEMS: usize = 8;

/// A refused request: what was refused, why, and the items it concerns, such as skipped files.
#[derive(Debug, Clone)]
pub struct Alert {
    /// What was refused, as a heading: "Kit not added".
    pub title: String,
    /// Why, and what to do about it, in a sentence or two.
    pub message: String,
    /// The items it concerns, each with its reason; at most [`MAX_ITEMS`] are shown.
    pub items: Vec<String>,
    /// The message and items as shown: wrapped, read-only text that can be selected and copied.
    body: Content,
}

impl Alert {
    /// An alert titled `title` saying `message`, with no items.
    pub(crate) fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Alert::with_items(title, message, Vec::new())
    }

    /// An alert titled `title` saying `message`, listing `items`.
    pub(crate) fn with_items(
        title: impl Into<String>,
        message: impl Into<String>,
        items: Vec<String>,
    ) -> Self {
        let message = message.into();
        let body = Content::with_text(&body_text(&message, &items));
        Alert {
            title: title.into(),
            message,
            items,
            body,
        }
    }

    /// The text shown under the title: the message, then each item on its own line, the first
    /// [`MAX_ITEMS`] and a count of the rest.
    pub fn text(&self) -> String {
        self.body.text()
    }

    /// Takes the keyboard while the alert is open: Enter, Space or Escape dismisses it, and the
    /// other focus keys and the window's shortcuts do nothing, so ⌘C copies only the alert's
    /// selected text (the text editor copies it itself).
    pub(crate) fn key(message: &Message) -> Key {
        match message {
            Message::ClearRange | Message::Dialog(DialogMessage::FocusPress) => {
                Key::Press(Message::DismissAlert)
            }
            Message::Dialog(m) if m.is_focus_key() => Key::Taken,
            Message::Cut
            | Message::Copy
            | Message::Paste
            | Message::Undo
            | Message::Redo
            | Message::DeleteRange
            | Message::OpenQuantize
            | Message::NewProject
            | Message::OpenProject => Key::Taken,
            _ => Key::Ignored,
        }
    }

    /// Applies a selection or copy in the body; edits are ignored, so it stays as written.
    pub(crate) fn act(&mut self, action: text_editor::Action) {
        if !action.is_edit() {
            self.body.perform(action);
        }
    }

    /// The alert: its title, then its body wrapped to the alert's width, then "OK".
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let body = iced::widget::text_editor(&self.body)
            .on_action(Message::AlertAction)
            .wrapping(Wrapping::WordOrGlyph)
            .size(14)
            .padding(0)
            .style(theme::plain_editor);
        let ok = row![
            Space::new().width(Length::Fill),
            tool(Face::Label("OK"), Some(Message::DismissAlert), true),
        ];
        iced::widget::container(column![text(&self.title).size(18), body, ok].spacing(14))
            .padding(16)
            .width(Length::Fixed(WIDTH))
            .style(theme::popup)
            .into()
    }
}

/// The body of an alert saying `message` about `items`.
fn body_text(message: &str, items: &[String]) -> String {
    let mut body = message.to_owned();
    if !items.is_empty() {
        body.push('\n');
    }
    for item in items.iter().take(MAX_ITEMS) {
        body.push_str("\n• ");
        body.push_str(item);
    }
    if items.len() > MAX_ITEMS {
        body.push_str(&format!("\n…and {} more", items.len() - MAX_ITEMS));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_body_text_lists_the_first_items_and_counts_the_rest() {
        let items: Vec<String> = (1..=10).map(|n| format!("file {n}: too long")).collect();
        let body = body_text("Some files could not be added.", &items);
        assert!(body.starts_with("Some files could not be added.\n\n• file 1: too long"));
        assert!(body.contains("• file 8: too long"));
        assert!(!body.contains("file 9"));
        assert!(body.ends_with("…and 2 more"));
        assert_eq!(body_text("Just this.", &[]), "Just this.");
    }

    #[test]
    fn test_key_takes_the_window_shortcuts() {
        assert_eq!(Alert::key(&Message::Copy), Key::Taken);
        assert_eq!(Alert::key(&Message::DeleteRange), Key::Taken);
        assert_eq!(
            Alert::key(&Message::ClearRange),
            Key::Press(Message::DismissAlert)
        );
        assert_eq!(Alert::key(&Message::Tick), Key::Ignored);
    }

    #[test]
    fn test_act_ignores_edits() {
        let mut alert = Alert::new("Kit not added", "No audio files were found.");
        alert.act(text_editor::Action::Edit(text_editor::Edit::Insert('x')));
        alert.act(text_editor::Action::SelectAll);
        assert_eq!(alert.text(), "No audio files were found.");
    }
}
