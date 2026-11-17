//! [`Alert`]: a modal message for something the user asked for that was refused, such as an
//! import, with one "OK" button (`docs/architecture/user-interface.md` "Files and devices").

use iced::widget::{Space, column, row, text};
use iced::{Element, Length};

use crate::app::Message;
use crate::export_dialog::Key;
use crate::node_dialog::DialogMessage;
use crate::theme;
use crate::view::{Face, selectable, tool};

/// The alert's width, in logical pixels.
const WIDTH: f32 = 420.0;
/// The most items listed; the rest are counted.
pub(crate) const MAX_ITEMS: usize = 8;

/// A refused request: what was refused, why, and the items it concerns, such as skipped files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// What was refused, as a heading: "Kit not added".
    pub title: String,
    /// Why, and what to do about it, in a sentence or two.
    pub message: String,
    /// The items it concerns, each with its reason; at most [`MAX_ITEMS`] are shown.
    pub items: Vec<String>,
}

impl Alert {
    /// An alert titled `title` saying `message`, with no items.
    pub(crate) fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Alert {
            title: title.into(),
            message: message.into(),
            items: Vec::new(),
        }
    }

    /// Takes the keyboard while the alert is open: Enter, Space or Escape dismisses it, and the
    /// other focus keys do nothing.
    pub(crate) fn key(message: &Message) -> Key {
        match message {
            Message::ClearRange | Message::Dialog(DialogMessage::FocusPress) => {
                Key::Press(Message::DismissAlert)
            }
            Message::Dialog(m) if m.is_focus_key() => Key::Taken,
            _ => Key::Ignored,
        }
    }

    /// The alert: its title, message and items, selectable so they can be copied, then "OK".
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let mut body =
            column![text(&self.title).size(18), selectable(&self.message, 14.0),].spacing(10);
        for item in self.items.iter().take(MAX_ITEMS) {
            body = body.push(selectable(&format!("• {item}"), 13.0));
        }
        if self.items.len() > MAX_ITEMS {
            let more = self.items.len() - MAX_ITEMS;
            body = body.push(text(format!("…and {more} more")).size(13));
        }
        body = body.push(
            row![
                Space::new().width(Length::Fill),
                tool(Face::Label("OK"), Some(Message::DismissAlert), true),
            ]
            .spacing(6),
        );
        iced::widget::container(body.spacing(14))
            .padding(16)
            .width(Length::Fixed(WIDTH))
            .style(theme::popup)
            .into()
    }
}
