//! The palette: signal colours shared by both tones, chrome tokens per tone, the `iced`
//! [`Theme`] for each, and the widget styles built from them (`docs/architecture/
//! user-interface.md` "Layout").

use iced::widget::{button, container};
use iced::{Background, Border, Color, Theme, border};

/// Audio signals: waveforms, audio ports and wires. The same in both tones.
pub const AUDIO: Color = Color::from_rgb(1.0, 0.5, 0.0);

/// The playhead: the lane cursor and the position bar.
pub const CURSOR: Color = Color::from_rgb(0.9, 0.2, 0.2);

/// Light or dark chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    /// Light chrome.
    Light,
    /// Dark chrome.
    #[default]
    Dark,
}

impl Tone {
    /// The other tone.
    pub fn toggled(self) -> Tone {
        match self {
            Tone::Light => Tone::Dark,
            Tone::Dark => Tone::Light,
        }
    }
}

/// The chrome colours of one tone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    /// The top and bottom bars, track headers and the port gutter.
    pub chrome: Color,
    /// Empty pane space and the patch board.
    pub pane: Color,
    /// Lanes and node bodies.
    pub lane: Color,
    /// Text on chrome, panes and lanes.
    pub text: Color,
    /// A button's face.
    pub face: Color,
    /// Text on a button's face.
    pub face_text: Color,
    /// Outlines.
    pub line: Color,
    /// The position bar's channel.
    pub channel: Color,
}

const DARK: Tokens = Tokens {
    chrome: Color::from_rgb(0.2, 0.2, 0.2),
    pane: Color::from_rgb(0.4, 0.4, 0.4),
    lane: Color::BLACK,
    text: Color::WHITE,
    // Halfway between `chrome` and black.
    face: Color::from_rgb(0.1, 0.1, 0.1),
    face_text: Color::WHITE,
    line: Color::from_rgb(0.1, 0.1, 0.1),
    channel: Color::from_rgb(0.45, 0.45, 0.45),
};

const LIGHT: Tokens = Tokens {
    chrome: Color::from_rgb(0.86, 0.86, 0.86),
    pane: Color::from_rgb(0.74, 0.74, 0.74),
    lane: Color::WHITE,
    text: Color::from_rgb(0.08, 0.08, 0.08),
    face: Color::WHITE,
    face_text: Color::BLACK,
    line: Color::from_rgb(0.55, 0.55, 0.55),
    channel: Color::from_rgb(0.62, 0.62, 0.62),
};

/// The tokens of `tone`.
pub fn tokens(tone: Tone) -> &'static Tokens {
    match tone {
        Tone::Light => &LIGHT,
        Tone::Dark => &DARK,
    }
}

/// The tone a theme was built for.
pub fn tone_of(theme: &Theme) -> Tone {
    if theme.extended_palette().is_dark {
        Tone::Dark
    } else {
        Tone::Light
    }
}

/// The `iced` theme for `tone`.
pub fn theme(tone: Tone) -> Theme {
    let t = tokens(tone);
    Theme::custom(
        match tone {
            Tone::Light => "Debussy light",
            Tone::Dark => "Debussy dark",
        },
        iced::theme::Palette {
            background: t.chrome,
            text: t.text,
            primary: AUDIO,
            success: Color::from_rgb(0.3, 0.7, 0.3),
            warning: Color::from_rgb(0.95, 0.75, 0.2),
            danger: CURSOR,
        },
    )
}

/// A container filled with `color`, in the theme's text colour.
fn fill(theme: &Theme, color: fn(&Tokens) -> Color) -> container::Style {
    let t = tokens(tone_of(theme));
    container::Style {
        background: Some(Background::Color(color(t))),
        text_color: Some(t.text),
        ..container::Style::default()
    }
}

/// The top and bottom bars, headers and gutter.
pub fn chrome(theme: &Theme) -> container::Style {
    fill(theme, |t| t.chrome)
}

/// Empty pane space.
pub fn pane(theme: &Theme) -> container::Style {
    fill(theme, |t| t.pane)
}

/// A dropdown menu or dialog: chrome with an outline and a shadow.
pub fn popup(theme: &Theme) -> container::Style {
    let t = tokens(tone_of(theme));
    container::Style {
        background: Some(Background::Color(t.chrome)),
        text_color: Some(t.text),
        border: Border {
            color: t.line,
            width: 1.0,
            radius: border::radius(6.0),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: iced::Vector::new(0.0, 3.0),
            blur_radius: 10.0,
        },
        ..container::Style::default()
    }
}

/// A toolbar button; `selected` marks the active side of a toggle.
pub fn tool(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tokens(tone_of(theme));
        let face = if selected { AUDIO } else { t.face };
        let ink = if selected { Color::BLACK } else { t.face_text };
        let (background, text) = match status {
            button::Status::Disabled => (Color { a: 0.35, ..face }, Color { a: 0.45, ..ink }),
            button::Status::Hovered => (mix(face, ink, 0.08), ink),
            button::Status::Pressed => (mix(face, ink, 0.18), ink),
            button::Status::Active => (face, ink),
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: text,
            border: Border {
                color: t.line,
                width: 1.0,
                radius: border::radius(6.0),
            },
            ..button::Style::default()
        }
    }
}

/// A flat button on chrome: menu items and the ≡ buttons.
pub fn flat(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(tone_of(theme));
    let (background, text) = match status {
        button::Status::Disabled => (None, Color { a: 0.4, ..t.text }),
        button::Status::Hovered | button::Status::Pressed => {
            (Some(Background::Color(mix(t.chrome, t.text, 0.15))), t.text)
        }
        button::Status::Active => (None, t.text),
    };
    button::Style {
        background,
        text_color: text,
        border: Border {
            radius: border::radius(4.0),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

/// `a` moved towards `b` by `amount` (0 to 1).
pub fn mix(a: Color, b: Color, amount: f32) -> Color {
    let k = amount.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * k,
        g: a.g + (b.g - a.g) * k,
        b: a.b + (b.b - a.b) * k,
        a: a.a + (b.a - a.a) * k,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_round_trips_its_tone() {
        assert_eq!(tone_of(&theme(Tone::Dark)), Tone::Dark);
        assert_eq!(tone_of(&theme(Tone::Light)), Tone::Light);
        assert_eq!(Tone::Dark.toggled(), Tone::Light);
    }

    #[test]
    fn test_mix_clamps() {
        assert_eq!(mix(Color::BLACK, Color::WHITE, 2.0), Color::WHITE);
        assert_eq!(mix(Color::BLACK, Color::WHITE, -1.0), Color::BLACK);
    }

    #[test]
    fn test_tool_dark_unselected_is_mid_grey_with_white_text() {
        let dark = theme(Tone::Dark);
        let unselected = tool(false)(&dark, button::Status::Active);
        assert_eq!(
            unselected.background,
            Some(Background::Color(mix(DARK.chrome, Color::BLACK, 0.5)))
        );
        assert_eq!(unselected.text_color, Color::WHITE);
        assert_eq!(
            tool(true)(&dark, button::Status::Active).text_color,
            Color::BLACK
        );
    }
}
