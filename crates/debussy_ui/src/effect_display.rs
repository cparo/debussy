//! The display at the top of an effect's dialog: a picture of what the effect does, drawn by
//! the [`EffectDisplay`] registered for its kind, or blank charcoal grey for a kind with none
//! (`docs/architecture/effect-displays.md` "Effect displays"). The picture is drawn again only
//! when what it shows or its size changes (ADR-0095).

use std::cell::RefCell;

use iced::widget::canvas::{Cache, Geometry, Program};
use iced::widget::{canvas, container};
use iced::{Element, Fill, Length, Rectangle, Renderer, Size, Theme, mouse};
use signal_graph::ParameterInfo;

use crate::app::Message;
use crate::raster::draw_tiled;
use crate::response_display::ResponseDisplay;
use crate::theme::{self, Tone};

/// The display area's height, in logical pixels; its width fills the dialog.
pub(crate) const DISPLAY_HEIGHT: f32 = 320.0;

/// One of an effect's parameters, as a display is given it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayParameter<'a> {
    /// The parameter's name, such as `"band 1 gain"`.
    pub name: &'a str,
    /// Its current value: the one being dragged, else the project's.
    pub value: f32,
    /// Its unit, such as `"dB"` or `"Hz"`; empty for none.
    pub unit: &'a str,
}

/// An image a display drew: `width` × `height` pixels, row by row from the top left, four
/// bytes (red, green, blue, alpha; not premultiplied) each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Pixels {
    /// `rgba` as a `width` × `height` image; `None` unless it holds exactly four bytes per
    /// pixel.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Pixels> {
        let expected = u64::from(width) * u64::from(height) * 4;
        (u64::try_from(rgba.len()).ok() == Some(expected)).then_some(Pixels {
            width,
            height,
            rgba,
        })
    }

    /// The width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, four bytes each, row by row from the top left.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// What a display draws for, besides the parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayContext<'a> {
    /// The effect's kind (the `kind` of `NodeKind::Effect`), such as `"low-pass"`.
    pub kind: &'a str,
    /// The tone of the chrome around the display.
    pub tone: Tone,
    /// The project's sample rate, in Hz.
    pub sample_rate: u32,
    /// The picture's width in physical pixels.
    pub width: u32,
    /// The picture's height in physical pixels; [`DISPLAY_HEIGHT`] logical pixels at the
    /// display's scale.
    pub height: u32,
}

/// Draws the display of one kind of effect. It runs on the interface thread whenever its
/// parameters, the tone, the sample rate or the display's size change, so it should be quick.
pub trait EffectDisplay: Sync {
    /// The picture of an effect whose parameters stand at `parameters` (in the effect's own
    /// order), `context.width` × `context.height` physical pixels; it is stretched to the
    /// display area if its size differs. `None` leaves the area blank.
    fn draw(
        &self,
        parameters: &[DisplayParameter<'_>],
        context: &DisplayContext<'_>,
    ) -> Option<Pixels>;
}

/// The displays, by effect kind (the `kind` of `NodeKind::Effect`). Register a new display
/// here.
const DISPLAYS: &[(&str, &dyn EffectDisplay)] = &[
    ("high-pass", &ResponseDisplay),
    ("low-pass", &ResponseDisplay),
    ("parametric-eq", &ResponseDisplay),
];

/// The display registered for effects of `kind`, if any.
pub(crate) fn display_for(kind: &str) -> Option<&'static dyn EffectDisplay> {
    find(DISPLAYS, kind)
}

/// The display for `kind` in `displays`.
fn find(
    displays: &'static [(&str, &'static dyn EffectDisplay)],
    kind: &str,
) -> Option<&'static dyn EffectDisplay> {
    displays.iter().find(|(k, _)| *k == kind).map(|(_, d)| *d)
}

/// The display area for an effect of `kind` whose parameters `infos` stand at `values`, in a
/// project at `sample_rate`, at `scale` physical pixels per logical pixel: the picture of the
/// display registered for `kind`, or charcoal grey. It fills the width it is given.
pub(crate) fn display_area<'a>(
    kind: &str,
    infos: &'a [ParameterInfo],
    values: Vec<f32>,
    tone: Tone,
    sample_rate: u32,
    scale: f32,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = match display_for(kind) {
        Some(display) => canvas(DisplayView {
            display,
            inputs: Inputs {
                kind: kind.to_owned(),
                values,
                tone,
                sample_rate,
            },
            infos,
            scale,
        })
        .width(Fill)
        .height(Fill)
        .into(),
        None => iced::widget::Space::new().into(),
    };
    container(body)
        .width(Fill)
        .height(Length::Fixed(DISPLAY_HEIGHT))
        .style(theme::blank_display)
        .into()
}

/// What a display's picture depends on, besides its size.
#[derive(Debug, Clone, PartialEq)]
struct Inputs {
    /// The effect's kind.
    kind: String,
    /// The parameters' values, in the effect's order.
    values: Vec<f32>,
    /// The tone of the chrome around the display.
    tone: Tone,
    /// The project's sample rate, in Hz.
    sample_rate: u32,
}

/// What the cached picture was drawn from: its inputs and its size in physical pixels.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    inputs: Inputs,
    width: u32,
    height: u32,
}

/// The canvas state of a display area: its picture, kept until the key it was drawn from
/// changes.
#[derive(Default)]
struct DisplayState {
    cache: Cache,
    key: RefCell<Option<Key>>,
}

/// Draws a registered display's picture into a canvas, once per change of its inputs or size.
struct DisplayView<'a> {
    /// The display registered for the effect's kind.
    display: &'static dyn EffectDisplay,
    /// What the picture shows.
    inputs: Inputs,
    /// The parameters' metadata, in the effect's order.
    infos: &'a [ParameterInfo],
    /// The display's physical pixels per logical pixel.
    scale: f32,
}

impl DisplayView<'_> {
    /// The key of a picture `size` logical pixels big.
    fn key(&self, size: Size) -> Key {
        Key {
            inputs: self.inputs.clone(),
            width: physical(size.width, self.scale),
            height: physical(size.height, self.scale),
        }
    }

    /// Forgets `state`'s picture if it was drawn from other inputs or at another size than
    /// `size`'s; returns whether it did.
    fn refresh(&self, state: &DisplayState, size: Size) -> bool {
        let key = self.key(size);
        let mut kept = state.key.borrow_mut();
        if kept.as_ref() == Some(&key) {
            return false;
        }
        state.cache.clear();
        *kept = Some(key);
        true
    }

    /// The display's picture, `width` × `height` physical pixels.
    fn picture(&self, width: u32, height: u32) -> Option<Pixels> {
        let parameters: Vec<DisplayParameter<'_>> = self
            .infos
            .iter()
            .zip(&self.inputs.values)
            .map(|(info, &value)| DisplayParameter {
                name: &info.name,
                value,
                unit: &info.unit,
            })
            .collect();
        let context = DisplayContext {
            kind: &self.inputs.kind,
            tone: self.inputs.tone,
            sample_rate: self.inputs.sample_rate,
            width,
            height,
        };
        self.display.draw(&parameters, &context)
    }
}

impl Program<Message> for DisplayView<'_> {
    type State = DisplayState;

    fn draw(
        &self,
        state: &DisplayState,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let size = bounds.size();
        self.refresh(state, size);
        let key = self.key(size);
        let geometry = state.cache.draw(renderer, size, |frame| {
            if let Some(p) = self.picture(key.width, key.height) {
                draw_tiled(
                    frame,
                    Rectangle::with_size(size),
                    p.width,
                    p.height,
                    &p.rgba,
                );
            }
        });
        vec![geometry]
    }
}

/// `logical` pixels at `scale`, rounded to whole physical pixels; at least one.
fn physical(logical: f32, scale: f32) -> u32 {
    // At least 1.0 (`max` also turns NaN into 1.0); `as` saturates above u32's range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = (logical * scale).round().max(1.0) as u32;
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A display that fills its picture with one grey per tone, and records nothing.
    struct Flat;

    impl EffectDisplay for Flat {
        fn draw(
            &self,
            parameters: &[DisplayParameter<'_>],
            context: &DisplayContext<'_>,
        ) -> Option<Pixels> {
            let (width, height) = (context.width, context.height);
            let grey = match context.tone {
                Tone::Light => 200,
                Tone::Dark => 40,
            };
            let first = parameters.first()?;
            assert_eq!((first.name, first.unit), ("gain", "dB"));
            let count = usize::try_from(width * height).ok()?;
            Pixels::new(width, height, [grey, grey, grey, 255].repeat(count))
        }
    }

    static TEST_DISPLAYS: &[(&str, &dyn EffectDisplay)] = &[("flat", &Flat)];

    #[test]
    fn test_pixels_new_rejects_a_wrong_length() {
        assert!(Pixels::new(2, 3, vec![0; 24]).is_some());
        assert!(Pixels::new(2, 3, vec![0; 23]).is_none());
        assert!(Pixels::new(0, 0, Vec::new()).is_some());
    }

    #[test]
    fn test_find_matches_kind_only() {
        assert!(find(TEST_DISPLAYS, "flat").is_some());
        assert!(find(TEST_DISPLAYS, "eq").is_none());
        assert!(display_for("").is_none());
        for kind in ["high-pass", "low-pass", "parametric-eq"] {
            assert!(display_for(kind).is_some(), "{kind}");
        }
    }

    #[test]
    fn test_display_draws_at_physical_size() -> Result<(), Box<dyn std::error::Error>> {
        let display = find(TEST_DISPLAYS, "flat").ok_or("not registered")?;
        let parameters = [DisplayParameter {
            name: "gain",
            value: -3.0,
            unit: "dB",
        }];
        let context = DisplayContext {
            kind: "flat",
            tone: Tone::Light,
            sample_rate: 48_000,
            width: physical(100.0, 2.0),
            height: physical(DISPLAY_HEIGHT, 2.0),
        };
        let picture = display.draw(&parameters, &context).ok_or("no picture")?;
        assert_eq!((picture.width(), picture.height()), (200, 640));
        assert_eq!(picture.rgba.first(), Some(&200));
        Ok(())
    }

    #[test]
    fn test_physical_rounds_and_keeps_one_pixel() {
        assert_eq!(physical(512.0, 1.5), 768);
        assert_eq!(physical(0.0, 2.0), 1);
        assert_eq!(physical(f32::NAN, 1.0), 1);
    }

    #[test]
    fn test_display_area_builds_with_and_without_a_display() {
        let infos = [ParameterInfo::new("gain", -24.0, 24.0, 0.0).with_unit("dB")];
        let _blank = display_area("unknown", &infos, vec![0.0], Tone::Dark, 48_000, 2.0);
        let infos = [
            ParameterInfo::new("cutoff", 20.0, 20_000.0, 80.0),
            ParameterInfo::new("order", 1.0, 8.0, 2.0),
        ];
        let _drawn = display_area(
            "high-pass",
            &infos,
            vec![80.0, 2.0],
            Tone::Dark,
            48_000,
            2.0,
        );
    }

    fn flat_view(infos: &[ParameterInfo], gain: f32, tone: Tone) -> DisplayView<'_> {
        DisplayView {
            display: &Flat,
            inputs: Inputs {
                kind: "flat".to_owned(),
                values: vec![gain],
                tone,
                sample_rate: 48_000,
            },
            infos,
            scale: 2.0,
        }
    }

    #[test]
    fn test_refresh_keeps_the_picture_until_an_input_changes() {
        let infos = [ParameterInfo::new("gain", -24.0, 24.0, 0.0).with_unit("dB")];
        let state = DisplayState::default();
        let size = Size::new(300.0, DISPLAY_HEIGHT);
        let view = flat_view(&infos, -3.0, Tone::Dark);
        assert!(view.refresh(&state, size), "first draw");
        assert!(!view.refresh(&state, size), "same inputs");
        assert!(!flat_view(&infos, -3.0, Tone::Dark).refresh(&state, size));
        assert!(
            flat_view(&infos, -2.0, Tone::Dark).refresh(&state, size),
            "value"
        );
        assert!(
            flat_view(&infos, -2.0, Tone::Light).refresh(&state, size),
            "tone"
        );
        let view = flat_view(&infos, -2.0, Tone::Light);
        assert!(
            view.refresh(&state, Size::new(301.0, DISPLAY_HEIGHT)),
            "size"
        );
        let mut rate = flat_view(&infos, -2.0, Tone::Light);
        rate.inputs.sample_rate = 44_100;
        assert!(
            rate.refresh(&state, Size::new(301.0, DISPLAY_HEIGHT)),
            "sample rate"
        );
    }

    #[test]
    fn test_picture_is_drawn_at_physical_size() -> Result<(), Box<dyn std::error::Error>> {
        let infos = [ParameterInfo::new("gain", -24.0, 24.0, 0.0).with_unit("dB")];
        let view = flat_view(&infos, -3.0, Tone::Light);
        let key = view.key(Size::new(100.0, DISPLAY_HEIGHT));
        let picture = view.picture(key.width, key.height).ok_or("no picture")?;
        assert_eq!((picture.width(), picture.height()), (200, 640));
        Ok(())
    }
}
