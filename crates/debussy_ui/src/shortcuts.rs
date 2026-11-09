//! Keyboard shortcuts (`docs/architecture/user-interface.md` "Range selection and shortcuts").

use iced::keyboard::{Event, Key, key::Named};

use crate::app::Message;

/// The message a key press stands for, if any. The window sees only presses no widget took, so
/// keys typed into a text field, or Delete on a patch board selection, never arrive here.
pub fn shortcut(event: Event) -> Option<Message> {
    let Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    if modifiers.alt() {
        return None;
    }
    match key.as_ref() {
        Key::Named(Named::Escape) if modifiers.is_empty() => Some(Message::ClearRange),
        Key::Named(Named::Delete | Named::Backspace) if modifiers.is_empty() => {
            Some(Message::DeleteRange)
        }
        Key::Character(c) if modifiers.command() => match c.to_lowercase().as_str() {
            "x" => Some(Message::Cut),
            "c" => Some(Message::Copy),
            "v" => Some(Message::Paste),
            "z" if modifiers.shift() => Some(Message::Redo),
            "z" => Some(Message::Undo),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::{Location, Modifiers, key};

    fn press(key: Key, modifiers: Modifiers) -> Event {
        Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        }
    }

    fn character(c: &str) -> Key {
        Key::Character(c.into())
    }

    #[test]
    fn test_shortcut_edit_keys_need_command() {
        let command = Modifiers::COMMAND;
        assert_eq!(shortcut(press(character("x"), command)), Some(Message::Cut));
        assert_eq!(
            shortcut(press(character("c"), command)),
            Some(Message::Copy)
        );
        assert_eq!(
            shortcut(press(character("v"), command)),
            Some(Message::Paste)
        );
        assert_eq!(
            shortcut(press(character("z"), command)),
            Some(Message::Undo)
        );
        assert_eq!(
            shortcut(press(character("Z"), command | Modifiers::SHIFT)),
            Some(Message::Redo)
        );
        assert_eq!(shortcut(press(character("x"), Modifiers::empty())), None);
        assert_eq!(
            shortcut(press(character("x"), command | Modifiers::ALT)),
            None
        );
    }

    #[test]
    fn test_shortcut_delete_and_escape_take_no_modifiers() {
        let none = Modifiers::empty();
        for named in [Named::Delete, Named::Backspace] {
            assert_eq!(
                shortcut(press(Key::Named(named), none)),
                Some(Message::DeleteRange)
            );
            assert_eq!(shortcut(press(Key::Named(named), Modifiers::COMMAND)), None);
        }
        assert_eq!(
            shortcut(press(Key::Named(Named::Escape), none)),
            Some(Message::ClearRange)
        );
    }
}
