//! The palette: shared signal colours, chrome and audio tokens per tone, the `iced`
//! [`Theme`] for each, and the widget styles built from them (`docs/architecture/
//! user-interface.md` "Layout").

use iced::widget::{button, container, svg};
use iced::{Background, Border, Color, Theme, border};

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

/// The chrome and audio colours of one tone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    /// The top and bottom bars and track headers.
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
    /// Audio signals (waveforms, audio ports and wires) and a selected button's face.
    pub audio: Color,
    /// Text on an `audio` fill.
    pub audio_text: Color,
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
    audio: Color::from_rgb(1.0, 0.5, 0.0),
    audio_text: Color::BLACK,
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
    audio: Color::from_rgb8(0x99, 0xBB, 0xFF),
    audio_text: Color::BLACK,
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
            primary: t.audio,
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

/// The top and bottom bars and headers.
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

/// Where a toolbar button sits: alone, or at one position in a [`strip`] of grouped buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A button on its own, with its own outline.
    Alone,
    /// The leftmost button of a strip.
    First,
    /// A button between two others in a strip.
    Middle,
    /// The rightmost button of a strip.
    Last,
}

impl Place {
    /// The corner radii of a button here, rounding only the strip's outer corners by `r`.
    fn radius(self, r: f32) -> border::Radius {
        match self {
            Place::Alone => border::radius(r),
            Place::First => border::left(r),
            Place::Middle => border::radius(0.0),
            Place::Last => border::right(r),
        }
    }
}

/// The logo and the tone toggle's icon: drawn in the accent of a selected toolbar button,
/// straight onto the bar.
pub fn accent_icon(theme: &Theme, _status: svg::Status) -> svg::Style {
    svg::Style {
        color: Some(tokens(tone_of(theme)).audio),
    }
}

/// The outer corner radius of a toolbar button or strip.
const TOOL_RADIUS: f32 = 6.0;

/// A toolbar button's face and ink colours, before its status changes them.
fn tool_colours(t: &Tokens, selected: bool) -> (Color, Color) {
    if selected {
        (t.audio, t.audio_text)
    } else {
        (t.face, t.face_text)
    }
}

/// A disabled toolbar button's ink.
fn disabled_ink(ink: Color) -> Color {
    Color { a: 0.45, ..ink }
}

/// A toolbar button's icon: tinted with the ink its text label would have, or greyed out
/// towards the button's face when the button is disabled.
pub fn tool_icon(selected: bool, enabled: bool) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |theme, _status| {
        let (face, ink) = tool_colours(tokens(tone_of(theme)), selected);
        svg::Style {
            color: Some(if enabled { ink } else { mix(face, ink, 0.4) }),
        }
    }
}

/// A toolbar button at `place` showing text; `selected` marks the active side of a toggle.
/// Disabled, its face fades as well as its text.
pub fn tool(selected: bool, place: Place) -> impl Fn(&Theme, button::Status) -> button::Style {
    tool_style(selected, place, true)
}

/// A toolbar button's style; `fade_disabled` fades a disabled button's face. A button showing an
/// icon passes `false`: disabled, its face is unchanged and only its icon greys out
/// ([`tool_icon`]).
pub fn tool_style(
    selected: bool,
    place: Place,
    fade_disabled: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tokens(tone_of(theme));
        let (face, ink) = tool_colours(t, selected);
        let (background, text) = match status {
            button::Status::Disabled if fade_disabled => {
                (Color { a: 0.35, ..face }, disabled_ink(ink))
            }
            button::Status::Disabled => (face, disabled_ink(ink)),
            button::Status::Hovered => (mix(face, ink, 0.08), ink),
            button::Status::Pressed => (mix(face, ink, 0.18), ink),
            button::Status::Active => (face, ink),
        };
        // A strip draws the outline and seams; its buttons fill the space inside its 1 px border.
        let border = match place {
            Place::Alone => Border {
                color: t.line,
                width: 1.0,
                radius: place.radius(TOOL_RADIUS),
            },
            _ => Border {
                color: t.line,
                width: 0.0,
                radius: place.radius(TOOL_RADIUS - 1.0),
            },
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: text,
            border,
            ..button::Style::default()
        }
    }
}

/// The strip behind a group of toolbar buttons: its outline, and the seams between them.
pub fn strip(theme: &Theme) -> container::Style {
    let t = tokens(tone_of(theme));
    container::Style {
        background: Some(Background::Color(t.line)),
        border: Border {
            color: t.line,
            width: 1.0,
            radius: border::radius(TOOL_RADIUS),
        },
        ..container::Style::default()
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
    fn test_accent_icon_takes_the_selected_button_accent() {
        for tone in [Tone::Light, Tone::Dark] {
            let accent = tool(true, Place::Alone)(&theme(tone), button::Status::Active).background;
            let icon = accent_icon(&theme(tone), svg::Status::Idle).color;
            assert_eq!(icon.map(Background::Color), accent);
        }
    }

    #[test]
    fn test_tool_dark_unselected_is_mid_grey_with_white_text() {
        let dark = theme(Tone::Dark);
        let unselected = tool(false, Place::Alone)(&dark, button::Status::Active);
        assert_eq!(
            unselected.background,
            Some(Background::Color(mix(DARK.chrome, Color::BLACK, 0.5)))
        );
        assert_eq!(unselected.text_color, Color::WHITE);
        assert_eq!(
            tool(true, Place::Alone)(&dark, button::Status::Active).text_color,
            DARK.audio_text
        );
    }

    #[test]
    fn test_tokens_audio_differs_by_tone() {
        assert_ne!(LIGHT.audio, DARK.audio);
        let light = tool(true, Place::Alone)(&theme(Tone::Light), button::Status::Active);
        assert_eq!(light.background, Some(Background::Color(LIGHT.audio)));
        assert_eq!(light.text_color, LIGHT.audio_text);
    }

    #[test]
    fn test_tool_place_sets_corner_radii() {
        let dark = theme(Tone::Dark);
        let style = |place| tool(false, place)(&dark, button::Status::Active).border;
        assert_eq!(style(Place::Alone).radius, border::radius(6.0));
        assert_eq!(style(Place::Alone).width, 1.0);
        assert_eq!(style(Place::First).radius, border::left(5.0));
        assert_eq!(style(Place::Middle).radius, border::radius(0.0));
        assert_eq!(style(Place::Last).radius, border::right(5.0));
        for place in [Place::First, Place::Middle, Place::Last] {
            assert_eq!(style(place).width, 0.0);
        }
    }
}
