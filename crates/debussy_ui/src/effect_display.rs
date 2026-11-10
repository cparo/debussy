//! The display at the top of an effect's dialog: a picture of what the effect does, drawn by
//! the [`EffectDisplay`] registered for its kind, or blank charcoal grey for a kind with none
//! (`docs/architecture/user-interface.md` "Effect displays").

use iced::widget::{container, image};
use iced::{ContentFit, Element, Fill, Length};
use signal_graph::ParameterInfo;

use crate::app::Message;
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
}

/// Draws the display of one kind of effect. It runs on the interface thread whenever the
/// dialog is drawn, so it should be quick.
pub trait EffectDisplay: Sync {
    /// The picture of an effect whose parameters stand at `parameters` (in the effect's own
    /// order), for chrome of `tone`, `width` × `height` physical pixels; it is stretched to the
    /// display area if its size differs. `None` leaves the area blank.
    fn draw(
        &self,
        parameters: &[DisplayParameter<'_>],
        tone: Tone,
        width: u32,
        height: u32,
    ) -> Option<Pixels>;
}

/// The displays, by effect kind (the `kind` of `NodeKind::Effect`). Register a new display
/// here.
const DISPLAYS: &[(&str, &dyn EffectDisplay)] = &[];

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

/// The display area for an effect of `kind` whose parameters `infos` stand at `values`, `width`
/// logical pixels wide at `scale` physical pixels per logical pixel: the registered display's
/// picture, or charcoal grey.
pub(crate) fn display_area<'a>(
    display: Option<&dyn EffectDisplay>,
    infos: &[ParameterInfo],
    values: &[f32],
    tone: Tone,
    width: f32,
    scale: f32,
) -> Element<'a, Message> {
    let parameters: Vec<DisplayParameter<'_>> = infos
        .iter()
        .zip(values)
        .map(|(info, &value)| DisplayParameter {
            name: &info.name,
            value,
            unit: &info.unit,
        })
        .collect();
    let picture = display.and_then(|d| {
        d.draw(
            &parameters,
            tone,
            physical(width, scale),
            physical(DISPLAY_HEIGHT, scale),
        )
    });
    let body: Element<'a, Message> = match picture {
        Some(p) => image(image::Handle::from_rgba(p.width, p.height, p.rgba))
            .width(Fill)
            .height(Fill)
            .content_fit(ContentFit::Fill)
            .into(),
        None => iced::widget::Space::new().into(),
    };
    container(body)
        .width(Fill)
        .height(Length::Fixed(DISPLAY_HEIGHT))
        .style(theme::blank_display)
        .into()
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
            tone: Tone,
            width: u32,
            height: u32,
        ) -> Option<Pixels> {
            let grey = match tone {
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
    }

    #[test]
    fn test_display_draws_at_physical_size() -> Result<(), Box<dyn std::error::Error>> {
        let display = find(TEST_DISPLAYS, "flat").ok_or("not registered")?;
        let parameters = [DisplayParameter {
            name: "gain",
            value: -3.0,
            unit: "dB",
        }];
        let picture = display
            .draw(
                &parameters,
                Tone::Light,
                physical(100.0, 2.0),
                physical(DISPLAY_HEIGHT, 2.0),
            )
            .ok_or("no picture")?;
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
        let _blank = display_area(None, &infos, &[0.0], Tone::Dark, 300.0, 2.0);
        let display = find(TEST_DISPLAYS, "flat");
        let _drawn = display_area(display, &infos, &[0.0], Tone::Dark, 300.0, 2.0);
    }
}
