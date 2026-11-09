//! Menus: `iced` has no menu bar, so a menu is a column of flat buttons floated over the window
//! while it is open, with the rest of the window catching a click that closes it
//! (`docs/architecture/user-interface.md` "Layout").

use iced::widget::{Space, button, column, container, rule, svg, text};
use iced::{Element, Length};
use project_model::TrackId;

use crate::app::Message;
use crate::icon::Icon;
use crate::theme;

/// The width of a dropdown menu, in logical pixels.
pub(crate) const MENU_WIDTH: f32 = 220.0;

/// Which menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    /// The ≡ application menu in the top bar.
    App,
    /// A track header's ≡ menu.
    Track(TrackId),
    /// The "+" below the tracks.
    AddTrack,
    /// The "+" at the board's top right.
    AddNode,
}

/// A line in a menu.
pub(crate) enum Entry {
    /// A choice: greyed out when it has no message.
    Item(String, Option<Message>),
    /// A dividing line.
    Separator,
}

impl Entry {
    /// A choice that is available when `message` is.
    pub(crate) fn item(label: impl Into<String>, message: Option<Message>) -> Entry {
        Entry::Item(label.into(), message)
    }
}

/// The menu holding `entries`.
pub(crate) fn dropdown<'a>(entries: Vec<Entry>) -> Element<'a, Message> {
    let lines = entries.into_iter().map(|entry| match entry {
        Entry::Item(label, message) => button(text(label).size(14))
            .on_press_maybe(message)
            .width(Length::Fill)
            .padding([4, 10])
            .style(theme::flat)
            .into(),
        Entry::Separator => container(rule::horizontal(1)).padding([4, 6]).into(),
    });
    container(column(lines).spacing(1))
        .padding(4)
        .width(Length::Fixed(MENU_WIDTH))
        .style(theme::popup)
        .into()
}

/// A ≡ button opening `menu`: an `icon`-pixel menu icon centred in `padding` on every side.
pub(crate) fn burger<'a>(menu: Menu, icon: f32, padding: f32) -> Element<'a, Message> {
    button(
        svg(Icon::Menu.handle())
            .width(icon)
            .height(icon)
            .style(theme::flat_icon),
    )
    .on_press(Message::OpenMenu(menu))
    .padding(padding)
    .style(theme::flat)
    .into()
}

/// A gap of `width` pixels in a bar.
pub(crate) fn gap<'a>(width: f32) -> Element<'a, Message> {
    Space::new().width(Length::Fixed(width)).into()
}
