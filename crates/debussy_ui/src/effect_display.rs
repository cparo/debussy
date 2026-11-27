//! The display at the top of an effect's or instrument's dialog: a picture of the curve an
//! effect's parameter values ask for (`audio_effects::curve`) or of the note an instrument's play
//! (`instruments::note_curve`), drawn by the display of that kind of picture, or blank charcoal
//! grey for a kind with none (`docs/architecture/effect-displays.md` "Effect displays"). The
//! picture is drawn again only when the curve, the tone or its size changes (ADR-0095).

use std::cell::RefCell;

use audio_effects::Curve;
use iced::widget::canvas::{Cache, Geometry, Program};
use iced::widget::{canvas, container};
use iced::{Element, Fill, Length, Rectangle, Renderer, Size, Theme, mouse};
use instruments::{Note, NoteCurve};

use crate::app::Message;
use crate::raster::draw_tiled;
use crate::theme::{self, Tone};
use crate::{
    amplifier_display, gain_display, impulse_display, note_display, response_display,
    transfer_display,
};

/// The display area's height, in logical pixels; its width fills the dialog.
pub(crate) const DISPLAY_HEIGHT: f32 = 320.0;

/// An instrument's note display's height, in logical pixels: half an effect's.
pub(crate) const SYNTH_DISPLAY_HEIGHT: f32 = DISPLAY_HEIGHT / 2.0;

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

/// What a display draws for, besides the curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayContext {
    /// The tone of the chrome around the display.
    pub tone: Tone,
    /// The picture's width in physical pixels.
    pub width: u32,
    /// The picture's height in physical pixels: `logical_height` at the display's scale.
    pub height: u32,
    /// The display's height in logical pixels: [`DISPLAY_HEIGHT`], or
    /// [`SYNTH_DISPLAY_HEIGHT`] for an instrument's note.
    pub logical_height: u32,
}

/// What a display area pictures: an effect's curve or an instrument's note.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Picture {
    /// The curve an effect's parameter values ask for.
    Effect(Curve),
    /// The note an instrument's parameter values play.
    Note(NoteCurve),
}

impl Picture {
    /// The picture of node kind `kind` at `values`, asking the instruments for an `instrument`
    /// and the effects otherwise; `None` for a kind with no picture.
    fn of(
        kind: &str,
        instrument: bool,
        values: &[f32],
        sample_rate: u32,
        frames_per_whole_note: f64,
    ) -> Option<Picture> {
        if instrument {
            instruments::note_curve(kind, values, sample_rate).map(Picture::Note)
        } else {
            audio_effects::curve(kind, values, sample_rate, frames_per_whole_note)
                .map(Picture::Effect)
        }
    }

    /// The display area's height for this picture, in logical pixels.
    fn height(&self) -> f32 {
        match self {
            Picture::Effect(_) => DISPLAY_HEIGHT,
            Picture::Note(_) => SYNTH_DISPLAY_HEIGHT,
        }
    }
}

/// The picture of `curve`, `context.width` × `context.height` physical pixels, drawn by the
/// display of its kind of curve. It runs on the interface thread whenever the curve, the tone
/// or the display's size change, so each display should be quick. `None` leaves the area blank.
pub(crate) fn draw(curve: &Curve, context: &DisplayContext) -> Option<Pixels> {
    match curve {
        Curve::Response(response) => response_display::render(response, context),
        Curve::Transfer(transfer) => transfer_display::render(transfer, context),
        Curve::Shaper(shaper) => amplifier_display::render(shaper, context),
        Curve::Impulse(impulse) => impulse_display::render(impulse, context),
        Curve::Spectrogram(spectrogram) => gain_display::render(spectrogram, context),
    }
}

/// The default values of `kind`'s parameters with `changes` set by name, for tests.
#[cfg(test)]
pub(crate) fn values_with(kind: &str, changes: &[(&str, f32)]) -> Option<Vec<f32>> {
    let infos = audio_effects::parameter_infos(kind)?;
    Some(
        infos
            .iter()
            .map(|info| {
                changes
                    .iter()
                    .find(|(name, _)| *name == info.name)
                    .map_or(info.default, |&(_, value)| value)
            })
            .collect(),
    )
}

/// The display area for a node of `kind` (an instrument's if `instrument`, else an effect's)
/// whose parameters stand at `values` (in the node's order), in a project at `sample_rate` whose
/// whole note is `frames_per_whole_note` frames, at `scale` physical pixels per logical pixel:
/// the picture of the curve or note they give, or charcoal grey. It fills the width it is given;
/// it is [`SYNTH_DISPLAY_HEIGHT`] tall for a note and [`DISPLAY_HEIGHT`] otherwise.
pub(crate) fn display_area<'a>(
    kind: &str,
    instrument: bool,
    values: &[f32],
    tone: Tone,
    sample_rate: u32,
    frames_per_whole_note: f64,
    scale: f32,
) -> Element<'a, Message> {
    let picture = Picture::of(kind, instrument, values, sample_rate, frames_per_whole_note);
    let height = picture.as_ref().map_or(DISPLAY_HEIGHT, Picture::height);
    let body: Element<'a, Message> = match picture {
        Some(picture) => canvas(DisplayView {
            picture,
            tone,
            scale,
        })
        .width(Fill)
        .height(Fill)
        .into(),
        None => iced::widget::Space::new().into(),
    };
    container(body)
        .width(Fill)
        .height(Length::Fixed(height))
        .style(theme::blank_display)
        .into()
}

/// What the cached picture was drawn from: the picture, the tone and its size in physical
/// pixels.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    picture: Picture,
    tone: Tone,
    width: u32,
    height: u32,
}

/// The canvas state of a display area: its picture, kept until the key it was drawn from
/// changes, and the last note rendered, kept until its curve changes, so a resize or a change
/// of tone does not render it again.
#[derive(Default)]
struct DisplayState {
    cache: Cache,
    key: RefCell<Option<Key>>,
    note: RefCell<Option<(NoteCurve, Note)>>,
}

/// Draws a picture into a canvas, once per change of the picture, the tone or the size.
struct DisplayView {
    /// What the picture shows.
    picture: Picture,
    /// The tone of the chrome around the display.
    tone: Tone,
    /// The display's physical pixels per logical pixel.
    scale: f32,
}

impl DisplayView {
    /// The key of a picture `size` logical pixels big.
    fn key(&self, size: Size) -> Key {
        Key {
            picture: self.picture.clone(),
            tone: self.tone,
            width: physical(size.width, self.scale),
            height: physical(size.height, self.scale),
        }
    }

    /// Forgets `state`'s picture if it was drawn from another curve or tone or at another size
    /// than `size`'s; returns whether it did.
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

    /// The display's picture, `width` × `height` physical pixels, rendering a note only if
    /// `state` does not hold it already.
    fn picture(&self, state: &DisplayState, width: u32, height: u32) -> Option<Pixels> {
        // The display heights are whole logical pixels.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let context = DisplayContext {
            tone: self.tone,
            width,
            height,
            logical_height: self.picture.height() as u32,
        };
        match &self.picture {
            Picture::Effect(curve) => draw(curve, &context),
            Picture::Note(curve) => {
                let mut kept = state.note.borrow_mut();
                if kept.as_ref().is_none_or(|(c, _)| c != curve) {
                    *kept = Some((curve.clone(), curve.render()));
                }
                let (_, note) = kept.as_ref()?;
                note_display::render(curve, note, &context)
            }
        }
    }
}

impl Program<Message> for DisplayView {
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
            if let Some(p) = self.picture(state, key.width, key.height) {
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

    /// The curve of `kind` at its defaults with `changes` by name, at `sample_rate`.
    fn curve_of(kind: &str, changes: &[(&str, f32)], sample_rate: u32) -> Option<Curve> {
        audio_effects::curve(kind, &values_with(kind, changes)?, sample_rate, 96_000.0)
    }

    /// The view of an equaliser with its high-pass on at `cutoff` Hz.
    fn view(cutoff: f32, tone: Tone, sample_rate: u32) -> Option<DisplayView> {
        let changes = [("high-pass enabled", 1.0), ("high-pass cutoff", cutoff)];
        Some(DisplayView {
            picture: Picture::Effect(curve_of("parametric-eq", &changes, sample_rate)?),
            tone,
            scale: 2.0,
        })
    }

    #[test]
    fn test_pixels_new_rejects_a_wrong_length() {
        assert!(Pixels::new(2, 3, vec![0; 24]).is_some());
        assert!(Pixels::new(2, 3, vec![0; 23]).is_none());
        assert!(Pixels::new(0, 0, Vec::new()).is_some());
    }

    #[test]
    fn test_draw_every_kind_with_a_curve_at_physical_size() -> Result<(), Box<dyn std::error::Error>>
    {
        let context = DisplayContext {
            tone: Tone::Light,
            width: physical(100.0, 2.0),
            height: physical(DISPLAY_HEIGHT, 2.0),
            logical_height: 320,
        };
        for kind in audio_effects::EFFECTS.iter().filter(|k| k.curve.is_some()) {
            let curve = curve_of(kind.id, &[], 48_000).ok_or("no curve")?;
            let picture = draw(&curve, &context).ok_or("no picture")?;
            assert_eq!(
                (picture.width(), picture.height()),
                (200, 640),
                "{}",
                kind.id
            );
        }
        Ok(())
    }

    #[test]
    fn test_physical_rounds_and_keeps_one_pixel() {
        assert_eq!(physical(512.0, 1.5), 768);
        assert_eq!(physical(0.0, 2.0), 1);
        assert_eq!(physical(f32::NAN, 1.0), 1);
    }

    #[test]
    fn test_display_area_builds_with_and_without_a_curve() -> Result<(), Box<dyn std::error::Error>>
    {
        let eq = "parametric-eq";
        let values = values_with(eq, &[]).ok_or("unknown kind")?;
        let _blank = display_area("unknown", false, &[0.0], Tone::Dark, 48_000, 96_000.0, 2.0);
        let _wrong_length = display_area(eq, false, &[80.0], Tone::Dark, 48_000, 96_000.0, 2.0);
        let _drawn = display_area(eq, false, &values, Tone::Dark, 48_000, 96_000.0, 2.0);
        Ok(())
    }

    #[test]
    fn test_refresh_keeps_the_picture_until_an_input_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = DisplayState::default();
        let size = Size::new(300.0, DISPLAY_HEIGHT);
        let first = view(80.0, Tone::Dark, 48_000).ok_or("no curve")?;
        assert!(first.refresh(&state, size), "first draw");
        assert!(!first.refresh(&state, size), "same inputs");
        let same = view(80.0, Tone::Dark, 48_000).ok_or("no curve")?;
        assert!(!same.refresh(&state, size));
        let cutoff = view(90.0, Tone::Dark, 48_000).ok_or("no curve")?;
        assert!(cutoff.refresh(&state, size), "value");
        let tone = view(90.0, Tone::Light, 48_000).ok_or("no curve")?;
        assert!(tone.refresh(&state, size), "tone");
        assert!(
            tone.refresh(&state, Size::new(301.0, DISPLAY_HEIGHT)),
            "size"
        );
        let rate = view(90.0, Tone::Light, 44_100).ok_or("no curve")?;
        assert!(
            rate.refresh(&state, Size::new(301.0, DISPLAY_HEIGHT)),
            "sample rate"
        );
        Ok(())
    }

    #[test]
    fn test_refresh_ignores_values_the_curve_does_not_use() -> Result<(), Box<dyn std::error::Error>>
    {
        let state = DisplayState::default();
        let size = Size::new(300.0, DISPLAY_HEIGHT);
        let view = |attack| -> Option<DisplayView> {
            Some(DisplayView {
                picture: Picture::Effect(curve_of("dynamics", &[("attack", attack)], 48_000)?),
                tone: Tone::Dark,
                scale: 2.0,
            })
        };
        assert!(view(10.0).ok_or("no curve")?.refresh(&state, size));
        assert!(!view(50.0).ok_or("no curve")?.refresh(&state, size));
        Ok(())
    }

    #[test]
    fn test_picture_is_drawn_at_physical_size() -> Result<(), Box<dyn std::error::Error>> {
        let view = view(80.0, Tone::Light, 48_000).ok_or("no curve")?;
        let key = view.key(Size::new(100.0, DISPLAY_HEIGHT));
        let picture = view
            .picture(&DisplayState::default(), key.width, key.height)
            .ok_or("no picture")?;
        assert_eq!((picture.width(), picture.height()), (200, 640));
        Ok(())
    }

    /// The synth's default values with `changes` set by name.
    fn synth_values(changes: &[(&str, f32)]) -> Option<Vec<f32>> {
        let infos = instruments::parameter_infos("synth")?;
        Some(
            infos
                .iter()
                .map(|info| {
                    changes
                        .iter()
                        .find(|(name, _)| *name == info.name)
                        .map_or(info.default, |&(_, value)| value)
                })
                .collect(),
        )
    }

    #[test]
    fn test_picture_height_is_half_for_a_note() -> Result<(), Box<dyn std::error::Error>> {
        let synth = synth_values(&[]).ok_or("no synth")?;
        let note = Picture::of("synth", true, &synth, 48_000, 96_000.0).ok_or("no note")?;
        assert_eq!(note.height(), 160.0);
        let eq = values_with("parametric-eq", &[]).ok_or("no eq")?;
        let effect = Picture::of("parametric-eq", false, &eq, 48_000, 96_000.0).ok_or("no eq")?;
        assert_eq!(effect.height(), 320.0);
        assert!(Picture::of("sampler", true, &[], 48_000, 96_000.0).is_none());
        // An instrument's kind is not looked up among the effects, nor the reverse.
        assert!(Picture::of("synth", false, &synth, 48_000, 96_000.0).is_none());
        let _area = display_area("synth", true, &synth, Tone::Dark, 48_000, 96_000.0, 2.0);
        Ok(())
    }

    #[test]
    fn test_refresh_ignores_synth_values_the_note_does_not_use()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = DisplayState::default();
        let size = Size::new(300.0, SYNTH_DISPLAY_HEIGHT);
        let view = |changes: &[(&str, f32)]| -> Option<DisplayView> {
            Some(DisplayView {
                picture: Picture::of("synth", true, &synth_values(changes)?, 48_000, 96_000.0)?,
                tone: Tone::Dark,
                scale: 1.0,
            })
        };
        assert!(
            view(&[("voices", 8.0)])
                .ok_or("no note")?
                .refresh(&state, size)
        );
        assert!(
            !view(&[("voices", 3.0)])
                .ok_or("no note")?
                .refresh(&state, size)
        );
        assert!(
            view(&[("cutoff", 500.0)])
                .ok_or("no note")?
                .refresh(&state, size)
        );
        Ok(())
    }

    #[test]
    fn test_note_picture_is_drawn_at_physical_size_and_keeps_its_note()
    -> Result<(), Box<dyn std::error::Error>> {
        let picture = Picture::of(
            "synth",
            true,
            &synth_values(&[]).ok_or("no synth")?,
            48_000,
            0.0,
        )
        .ok_or("no note")?;
        let view = DisplayView {
            picture,
            tone: Tone::Light,
            scale: 2.0,
        };
        let state = DisplayState::default();
        let key = view.key(Size::new(300.0, SYNTH_DISPLAY_HEIGHT));
        let picture = view
            .picture(&state, key.width, key.height)
            .ok_or("no picture")?;
        assert_eq!((picture.width(), picture.height()), (600, 320));
        assert!(state.note.borrow().is_some());
        Ok(())
    }
}
