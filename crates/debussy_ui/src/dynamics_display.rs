//! The dynamics effect's display: its envelope window beside its static transfer curve
//! (`docs/architecture/effect-displays.md` "Dynamics display").

use audio_effects::{DynamicsControl, static_output, window_weight};

use crate::effect_display::{
    DISPLAY_HEIGHT, DisplayContext, DisplayParameter, EffectDisplay, Pixels,
};
use crate::raster::Raster;
use crate::response_display::db_label;
use crate::stroke::Mask;
use crate::theme::{self, GRID, GRID_ZERO, LABEL};
use crate::typeface::{StrokeFont, Typeface};

/// The input and output range of the curve plot, in dB.
const TOP_DB: f32 = 12.0;
const BOTTOM_DB: f32 = -96.0;

/// The curve plot's grid spacing, in dB: across, and down for one curve or for each of two.
const INPUT_STEP: f32 = 24.0;
const OUTPUT_STEP: f32 = 12.0;
const HALF_OUTPUT_STEP: f32 = 36.0;

/// Each plot's insets from its half of the display, in logical pixels: a gutter on the left for
/// labels and one below. The halves are separated by [`GAP`].
const LEFT: f32 = 46.0;
const BOTTOM: f32 = 20.0;
const GAP: f32 = 12.0;

/// The labels' capital height, and their gap from the plot or their line, in logical pixels.
const LABEL_SIZE: f32 = 9.0;
const LABEL_GAP: f32 = 5.0;

/// Line widths, in logical pixels.
const CURVE_WIDTH: f32 = 2.0;
const GRID_WIDTH: f32 = 1.0;

/// The display of the `dynamics` kind.
pub(crate) struct DynamicsDisplay;

/// The parameter values the display uses.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Settings {
    attack: f32,
    attack_exponent: f32,
    release: f32,
    release_exponent: f32,
    lookahead: f32,
    preamp: f32,
    makeup: f32,
    asymmetric: bool,
    /// Steepness (dB), curve exponent and ceiling (dB) for positive, then negative, samples.
    curves: [(f32, f32, f32); 2],
}

impl Settings {
    /// The settings in `parameters`, each by name; a missing one takes its default.
    fn from(parameters: &[DisplayParameter<'_>]) -> Settings {
        let get = |control: DynamicsControl, default: f32| {
            parameters
                .iter()
                .find(|p| p.name == control.name())
                .map_or(default, |p| p.value)
        };
        use DynamicsControl as C;
        let asymmetric = get(C::Asymmetric, 0.0) >= 0.5;
        let positive = (
            get(C::Steepness, 0.0),
            get(C::CurveExponent, 1.0),
            get(C::Ceiling, 0.0),
        );
        let negative = if asymmetric {
            (
                get(C::NegativeSteepness, 0.0),
                get(C::NegativeCurveExponent, 1.0),
                get(C::NegativeCeiling, 0.0),
            )
        } else {
            positive
        };
        Settings {
            attack: get(C::Attack, 10.0),
            attack_exponent: get(C::AttackExponent, 1.0),
            release: get(C::Release, 100.0),
            release_exponent: get(C::ReleaseExponent, 1.0),
            lookahead: get(C::Lookahead, 0.0),
            preamp: get(C::Preamp, 0.0),
            makeup: get(C::Makeup, 0.0),
            asymmetric,
            curves: [positive, negative],
        }
    }

    /// The window's weight at `offset` ms from its peak.
    fn weight(&self, offset: f32) -> f32 {
        window_weight(
            offset,
            self.attack,
            self.attack_exponent,
            self.release,
            self.release_exponent,
        )
    }

    /// The output level in dB for an input level of `db`, for positive (`side` 0) or negative
    /// samples.
    fn output_db(&self, db: f32, side: usize) -> f32 {
        let (steepness, exponent, ceiling) = self.curves[side];
        let level = static_output(
            10.0_f32.powf(db / 20.0),
            self.preamp,
            steepness,
            exponent,
            self.makeup,
            ceiling,
        );
        20.0 * level.max(1e-12).log10()
    }
}

impl EffectDisplay for DynamicsDisplay {
    fn draw(
        &self,
        parameters: &[DisplayParameter<'_>],
        context: &DisplayContext<'_>,
    ) -> Option<Pixels> {
        let (width, height) = (
            usize::try_from(context.width).ok()?,
            usize::try_from(context.height).ok()?,
        );
        let raster = render(
            &Settings::from(parameters),
            context,
            width,
            height,
            &StrokeFont,
        );
        Pixels::new(context.width, context.height, raster.into_rgba8())
    }
}

/// A plot's frame, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frame {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl Frame {
    /// The plot in the half of a display `width` logical pixels wide starting at `x`.
    fn half(x: f32, width: f32) -> Frame {
        let half = ((width - GAP) / 2.0).max(1.0);
        Frame {
            left: x + LEFT,
            top: 0.0,
            width: (half - LEFT).max(1.0),
            height: DISPLAY_HEIGHT - BOTTOM,
        }
    }

    fn right(&self) -> f32 {
        self.left + self.width
    }

    fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

/// The display for `settings`, `width` × `height` physical pixels, labelled in `face`. The
/// background is left transparent.
fn render(
    settings: &Settings,
    context: &DisplayContext<'_>,
    width: usize,
    height: usize,
    face: &dyn Typeface,
) -> Raster {
    // `height` is `DISPLAY_HEIGHT` logical pixels; at most a few thousand.
    #[allow(clippy::cast_precision_loss)]
    let scale = height as f32 / DISPLAY_HEIGHT;
    #[allow(clippy::cast_precision_loss)]
    let logical = width as f32 / scale;
    let mut raster = Raster::with_physical_size(width, height, scale);
    let mut labels = Mask::new(width, height, scale);
    let mut curves = Mask::new(width, height, scale);
    let envelope = Frame::half(0.0, logical);
    let curve = Frame::half((logical + GAP) / 2.0, logical);
    draw_envelope(
        &mut raster,
        &mut labels,
        &mut curves,
        &envelope,
        settings,
        face,
        scale,
    );
    draw_transfer(
        &mut raster,
        &mut labels,
        &mut curves,
        &curve,
        settings,
        face,
        scale,
    );
    raster.paint(&labels, LABEL);
    raster.paint(&curves, theme::tokens(context.tone).audio);
    raster
}

/// A vertical grid line at `x` in `frame`, kept inside it.
fn vertical(raster: &mut Raster, frame: &Frame, x: f32, color: iced::Color) {
    let x = (x - GRID_WIDTH / 2.0).clamp(frame.left, frame.right() - GRID_WIDTH);
    raster.fill_rect(x, frame.top, GRID_WIDTH, frame.height, color);
}

/// A horizontal grid line at `y` in `frame`, kept inside it.
fn horizontal(raster: &mut Raster, frame: &Frame, y: f32, color: iced::Color) {
    let y = (y - GRID_WIDTH / 2.0).clamp(frame.top, frame.bottom() - GRID_WIDTH);
    raster.fill_rect(frame.left, y, frame.width, GRID_WIDTH, color);
}

/// A label centred under `x`, kept inside the frame's width.
fn label_under(mask: &mut Mask, frame: &Frame, face: &dyn Typeface, text: &str, x: f32) {
    let advance = face.advance(text, LABEL_SIZE);
    let x = (x - advance / 2.0)
        .min(frame.right() - advance)
        .max(frame.left);
    face.draw(
        mask,
        text,
        x,
        frame.bottom() + LABEL_GAP + LABEL_SIZE,
        LABEL_SIZE,
    );
}

/// A label right-aligned in the gutter, centred on `y`, kept inside the frame's height.
fn label_beside(mask: &mut Mask, frame: &Frame, face: &dyn Typeface, text: &str, y: f32) {
    let x = frame.left - LABEL_GAP - face.advance(text, LABEL_SIZE);
    let baseline = (y + LABEL_SIZE / 2.0)
        .max(frame.top + LABEL_SIZE + 1.0)
        .min(frame.bottom());
    face.draw(mask, text, x, baseline, LABEL_SIZE);
}

/// A time label in ms, such as `"−110ms"`, `"0ms"` or `"+10ms"`.
fn ms_label(ms: f32) -> String {
    if ms > 0.0 {
        format!("+{ms:.0}ms")
    } else if ms < 0.0 {
        format!("\u{2212}{:.0}ms", -ms)
    } else {
        "0ms".to_owned()
    }
}

/// The envelope window: its release side on the left half and its attack side on the right,
/// each scaled to fill its half (a side of zero length gives the other the whole width), the
/// peak at weight 1 at the top. The current sample is a brighter vertical line.
#[allow(clippy::too_many_arguments)] // the frame, its settings and the three layers it draws on
fn draw_envelope(
    raster: &mut Raster,
    labels: &mut Mask,
    curves: &mut Mask,
    frame: &Frame,
    settings: &Settings,
    face: &dyn Typeface,
    scale: f32,
) {
    let (attack, release) = (settings.attack.max(0.0), settings.release.max(0.0));
    let peak_x = match (attack > 0.0, release > 0.0) {
        (true, true) | (false, false) => frame.left + frame.width / 2.0,
        (true, false) => frame.left,
        (false, true) => frame.right(),
    };
    // The `x` of a time `offset` ms from the peak.
    let x_of = |offset: f32| {
        if offset >= 0.0 {
            if attack > 0.0 {
                peak_x + (offset / attack).min(1.0) * (frame.right() - peak_x)
            } else {
                peak_x
            }
        } else if release > 0.0 {
            peak_x - (-offset / release).min(1.0) * (peak_x - frame.left)
        } else {
            peak_x
        }
    };
    let y_of = |weight: f32| frame.bottom() - weight.clamp(0.0, 1.0) * frame.height;
    for weight in [0.0, 0.5, 1.0] {
        horizontal(raster, frame, y_of(weight), GRID);
        label_beside(labels, frame, face, &format!("{weight}"), y_of(weight));
    }
    // The current sample is `attack − lookahead` ms before the peak's offset from it.
    let current = attack - settings.lookahead;
    vertical(raster, frame, x_of(current), GRID_ZERO);
    // Labels: the oldest sample's age at the left, the lookahead at the right, and the current
    // sample.
    let oldest = -(attack + release - settings.lookahead);
    label_under(labels, frame, face, &ms_label(oldest), frame.left);
    label_under(
        labels,
        frame,
        face,
        &ms_label(settings.lookahead),
        frame.right(),
    );
    let now = x_of(current);
    if now - frame.left > 40.0 && frame.right() - now > 40.0 {
        label_under(labels, frame, face, "0ms", now);
    }
    if attack == 0.0 && release == 0.0 {
        curves.stroke_segment(peak_x, y_of(0.0), peak_x, y_of(1.0), CURVE_WIDTH);
        return;
    }
    // A few thousand points at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (frame.width * scale * 2.0).ceil().max(1.0) as u32;
    let mut previous: Option<(f32, f32)> = None;
    for step in 0..=steps {
        #[allow(clippy::cast_precision_loss)]
        let x = frame.left + frame.width * step as f32 / steps as f32;
        let offset = if x >= peak_x {
            if frame.right() > peak_x {
                (x - peak_x) / (frame.right() - peak_x) * attack
            } else {
                0.0
            }
        } else {
            -(peak_x - x) / (peak_x - frame.left) * release
        };
        let y = y_of(settings.weight(offset)).max(frame.top + CURVE_WIDTH / 2.0);
        if let Some((x0, y0)) = previous {
            curves.stroke_segment(x0, y0, x, y, CURVE_WIDTH);
        }
        previous = Some((x, y));
    }
}

/// The static transfer curve, input dB across and output dB up, both −96 to +12 dB. When
/// asymmetric, the negative samples' curve is drawn inverted below a centre line at −96 dB.
#[allow(clippy::too_many_arguments)] // the frame, its settings and the three layers it draws on
fn draw_transfer(
    raster: &mut Raster,
    labels: &mut Mask,
    curves: &mut Mask,
    frame: &Frame,
    settings: &Settings,
    face: &dyn Typeface,
    scale: f32,
) {
    let range = TOP_DB - BOTTOM_DB;
    let x_of = |db: f32| frame.left + (db - BOTTOM_DB) / range * frame.width;
    // Each curve's `y` for an output of `db`: one over the whole height, or two from the centre.
    let halves: &[usize] = if settings.asymmetric { &[0, 1] } else { &[0] };
    let y_of = |db: f32, side: usize| {
        let db = db.clamp(BOTTOM_DB, TOP_DB);
        if settings.asymmetric {
            let centre = frame.top + frame.height / 2.0;
            let reach = (db - BOTTOM_DB) / range * frame.height / 2.0;
            if side == 0 {
                centre - reach
            } else {
                centre + reach
            }
        } else {
            frame.bottom() - (db - BOTTOM_DB) / range * frame.height
        }
    };
    let mut db = BOTTOM_DB;
    while db <= TOP_DB {
        let color = if db == 0.0 { GRID_ZERO } else { GRID };
        vertical(raster, frame, x_of(db), color);
        label_under(labels, frame, face, &db_label(db), x_of(db));
        db += INPUT_STEP;
    }
    let step = if settings.asymmetric {
        HALF_OUTPUT_STEP
    } else {
        OUTPUT_STEP
    };
    for &side in halves {
        let mut db = TOP_DB;
        while db >= BOTTOM_DB {
            let color = if db == 0.0 { GRID_ZERO } else { GRID };
            horizontal(raster, frame, y_of(db, side), color);
            // The shared centre line is labelled once.
            if !(side == 1 && db == BOTTOM_DB) {
                label_beside(labels, frame, face, &db_label(db), y_of(db, side));
            }
            db -= step;
        }
    }
    // A few thousand points at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (frame.width * scale * 2.0).ceil().max(1.0) as u32;
    let inset = CURVE_WIDTH / 2.0;
    for &side in halves {
        let mut previous: Option<(f32, f32)> = None;
        for i in 0..=steps {
            #[allow(clippy::cast_precision_loss)]
            let input = BOTTOM_DB + range * i as f32 / steps as f32;
            let x = x_of(input);
            let y = y_of(settings.output_db(input, side), side)
                .clamp(frame.top + inset, frame.bottom() - inset);
            if let Some((x0, y0)) = previous {
                curves.stroke_segment(x0, y0, x, y, CURVE_WIDTH);
            }
            previous = Some((x, y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Tone;

    /// The display at `values`, given by control, at scale 1 and 800 × 320 pixels.
    fn draw(values: &[(DynamicsControl, f32)]) -> Option<Pixels> {
        let parameters: Vec<DisplayParameter<'_>> = values
            .iter()
            .map(|&(control, value)| DisplayParameter {
                name: control.name(),
                value,
                unit: "",
            })
            .collect();
        let context = DisplayContext {
            kind: "dynamics",
            tone: Tone::Dark,
            sample_rate: 48_000,
            width: 800,
            height: 320,
        };
        DynamicsDisplay.draw(&parameters, &context)
    }

    /// The `(column, row)` of every pixel tinted like the curves: grid and labels are greys.
    fn accent(pixels: &Pixels) -> Vec<(usize, usize)> {
        let width = pixels.width() as usize;
        pixels
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| p[3] > 0 && p[0] != p[2])
            .map(|(i, _)| (i % width, i / width))
            .collect()
    }

    #[test]
    fn test_draw_returns_the_requested_size() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[]).ok_or("nothing drawn")?;
        assert_eq!((pixels.width(), pixels.height()), (800, 320));
        Ok(())
    }

    #[test]
    fn test_draw_plots_both_halves() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[]).ok_or("nothing drawn")?;
        let points = accent(&pixels);
        assert!(points.iter().any(|&(x, _)| x < 394));
        assert!(points.iter().any(|&(x, _)| x > 406));
        Ok(())
    }

    #[test]
    fn test_draw_asymmetric_curve_reaches_below_centre() -> Result<(), Box<dyn std::error::Error>> {
        let below =
            |values: &[(DynamicsControl, f32)]| -> Result<bool, Box<dyn std::error::Error>> {
                let pixels = draw(values).ok_or("nothing drawn")?;
                Ok(accent(&pixels)
                    .iter()
                    .any(|&(x, y)| x > 700 && y > 200 && y < 290))
            };
        // At loud inputs (the right of the curve plot) the symmetric curve is near the top; the
        // asymmetric one's inverted negative curve is near the bottom.
        assert!(!below(&[])?);
        assert!(below(&[(DynamicsControl::Asymmetric, 1.0)])?);
        Ok(())
    }

    #[test]
    fn test_output_db_includes_preamp_makeup_and_ceiling() {
        let settings = Settings::from(&[]);
        // tanh of a small level is the level.
        assert!((settings.output_db(-60.0, 0) + 60.0).abs() < 0.01);
        let loud = Settings {
            makeup: 24.0,
            ..settings
        };
        assert!((loud.output_db(0.0, 0) - 0.0).abs() < 1e-3);
    }

    #[test]
    fn test_ms_label_signs() {
        assert_eq!(ms_label(-110.0), "\u{2212}110ms");
        assert_eq!(ms_label(0.0), "0ms");
        assert_eq!(ms_label(10.0), "+10ms");
    }
}
