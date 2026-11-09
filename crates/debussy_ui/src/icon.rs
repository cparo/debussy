//! The control icons: compiled-in SVG artwork for toolbar buttons
//! (`docs/architecture/user-interface.md` "Icons"), each parsed once.

use std::sync::LazyLock;

use iced::widget::svg;

/// A control icon drawn on a toolbar button in place of a text label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Show lanes as waveforms.
    Waveform,
    /// Show lanes as spectrograms.
    Spectrogram,
    /// Show the bars and beats grid over the lanes.
    Grid,
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Cut the selection.
    Cut,
    /// Copy the selection.
    Copy,
    /// Paste the clipboard.
    Paste,
    /// Start playback.
    Play,
    /// Pause playback.
    Pause,
    /// Stop playback.
    Stop,
    /// Record.
    Record,
    /// Turn the click on or off.
    Click,
    /// Switch to the light tone.
    Light,
    /// Switch to the dark tone.
    Dark,
    /// Open a ≡ menu.
    Menu,
}

/// Declares the artwork file of each icon, and a lazily parsed handle for it.
macro_rules! artwork {
    ($($icon:ident: $handle:ident = $file:literal;)*) => {
        $(static $handle: LazyLock<svg::Handle> =
            LazyLock::new(|| svg::Handle::from_memory(Icon::$icon.source()));)*

        impl Icon {
            /// The artwork's SVG source, compiled in.
            fn source(self) -> &'static [u8] {
                match self {
                    $(Icon::$icon => include_bytes!(concat!("../assets/icons/", $file)),)*
                }
            }

            /// The parsed artwork, ready for an `svg` widget.
            pub fn handle(self) -> svg::Handle {
                match self {
                    $(Icon::$icon => LazyLock::force(&$handle).clone(),)*
                }
            }
        }
    };
}

artwork! {
    Waveform: WAVEFORM = "waveform.svg";
    Spectrogram: SPECTROGRAM = "spectrogram.svg";
    Grid: GRID = "grid.svg";
    Undo: UNDO = "undo.svg";
    Redo: REDO = "redo.svg";
    Cut: CUT = "cut.svg";
    Copy: COPY = "copy.svg";
    Paste: PASTE = "paste.svg";
    Play: PLAY = "play.svg";
    Pause: PAUSE = "pause.svg";
    Stop: STOP = "stop.svg";
    Record: RECORD = "record.svg";
    Click: CLICK = "click.svg";
    Light: LIGHT = "light.svg";
    Dark: DARK = "dark.svg";
    Menu: MENU = "menu.svg";
}

impl Icon {
    /// What the control does, shown as its tooltip.
    pub fn name(self) -> &'static str {
        match self {
            Icon::Waveform => "Waveform",
            Icon::Spectrogram => "Spectrogram",
            Icon::Grid => "Bars and Beats",
            Icon::Undo => "Undo",
            Icon::Redo => "Redo",
            Icon::Cut => "Cut",
            Icon::Copy => "Copy",
            Icon::Paste => "Paste",
            Icon::Play => "Play",
            Icon::Pause => "Pause",
            Icon::Stop => "Stop",
            Icon::Record => "Record",
            Icon::Click => "Click",
            Icon::Light => "Light Mode",
            Icon::Dark => "Dark Mode",
            Icon::Menu => "Menu",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Icon; 16] = [
        Icon::Waveform,
        Icon::Spectrogram,
        Icon::Grid,
        Icon::Undo,
        Icon::Redo,
        Icon::Cut,
        Icon::Copy,
        Icon::Paste,
        Icon::Play,
        Icon::Pause,
        Icon::Stop,
        Icon::Record,
        Icon::Click,
        Icon::Light,
        Icon::Dark,
        Icon::Menu,
    ];

    /// Every icon's artwork is a single-colour SVG with no text, as tinting requires.
    #[test]
    fn test_handle_artwork_is_single_colour_without_text() -> Result<(), Box<dyn std::error::Error>>
    {
        for icon in ALL {
            let source = std::str::from_utf8(icon.source())?;
            assert!(source.contains("viewBox=\"0 0 24 24\""), "{icon:?} viewBox");
            assert!(!source.contains("<text"), "{icon:?} holds text");
            let colours = source
                .split(['"', '\''])
                .filter(|s| s.starts_with('#'))
                .filter(|s| !matches!(*s, "#000" | "#fff"));
            assert_eq!(colours.count(), 0, "{icon:?} uses a second colour");
        }
        Ok(())
    }
}
