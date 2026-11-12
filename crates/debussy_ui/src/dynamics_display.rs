//! The dynamics effect's display: its envelope window beside its static transfer curve
//! (`docs/architecture/effect-displays.md` "Dynamics display").

use audio_effects::{DynamicsControl, static_output, window_weight};

use crate::effect_display::{
    DISPLAY_HEIGHT, DisplayContext, DisplayParameter, EffectDisplay, Pixels,
};
use crate::raster::Raster;
use crate::response_display::db_label;
use crate::stroke::Mask;
use crate::theme::{self, GRID, GRID_ZERO, LABEL, ZERO_DB_LINE};
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
    asymmetric: bool,
    /// Steepness, exponent, output gain (dB) and ceiling (dB) for positive, then negative,
    /// samples.
    curves: [(f32, f32, f32, f32); 2],
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
            get(C::PositiveSteepness, 1.0),
            get(C::PositiveExponent, 1.0),
            get(C::PositiveOutputGain, 0.0),
            get(C::PositiveCeiling, 0.0),
        );
        let negative = if asymmetric {
            (
                get(C::NegativeSteepness, 1.0),
                get(C::NegativeExponent, 1.0),
                get(C::NegativeOutputGain, 0.0),
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
        let (steepness, exponent, output_gain, ceiling) = self.curves[side];
        let level = static_output(
            10.0_f32.powf(db / 20.0),
            self.preamp,
            steepness,
            exponent,
            output_gain,
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

/// The envelope window against time on one linear axis, from the oldest sample it weighs (or
/// the current sample, if later) at the left to the lookahead at the right, so each side's width
/// is in proportion to its length. Weight 1 is at the top. The current sample is a brighter
/// vertical line.
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
    let lookahead = settings.lookahead.max(0.0);
    // Times in ms relative to the current sample, positive ahead; the peak is at `p`.
    let peak = lookahead - attack;
    let (mut start, mut end) = ((peak - release).min(0.0), lookahead);
    if end - start < 1e-3 {
        // Nothing to span: a window of zero length at the current sample.
        (start, end) = (-0.5, 0.5);
    }
    let x_of = |t: f32| frame.left + (t - start) / (end - start) * frame.width;
    let y_of = |weight: f32| frame.bottom() - weight.clamp(0.0, 1.0) * frame.height;
    for weight in [0.0, 0.5, 1.0] {
        horizontal(raster, frame, y_of(weight), GRID);
        label_beside(labels, frame, face, &format!("{weight}"), y_of(weight));
    }
    vertical(raster, frame, x_of(0.0), GRID_ZERO);
    label_under(labels, frame, face, &ms_label(peak - release), frame.left);
    label_under(labels, frame, face, &ms_label(lookahead), frame.right());
    let now = x_of(0.0);
    if now - frame.left > 40.0 && frame.right() - now > 40.0 {
        label_under(labels, frame, face, "0ms", now);
    }
    if attack == 0.0 && release == 0.0 {
        let x = x_of(peak);
        curves.stroke_segment(x, y_of(0.0), x, y_of(1.0), CURVE_WIDTH);
        return;
    }
    // A few thousand points at most, plus the peak itself so its corner is drawn.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (frame.width * scale * 2.0).ceil().max(1.0) as u32;
    #[allow(clippy::cast_precision_loss)]
    let mut times: Vec<f32> = (0..=steps)
        .map(|step| start + (end - start) * step as f32 / steps as f32)
        .collect();
    times.push(peak);
    times.sort_by(f32::total_cmp);
    let mut previous: Option<(f32, f32)> = None;
    for t in times {
        let (x, y) = (
            x_of(t),
            y_of(settings.weight(t - peak)).max(frame.top + CURVE_WIDTH / 2.0),
        );
        if let Some((x0, y0)) = previous {
            curves.stroke_segment(x0, y0, x, y, CURVE_WIDTH);
        }
        previous = Some((x, y));
    }
}

/// The static transfer curve, input dB across and output dB up, both −96 to +12 dB, with red
/// lines at 0 dB in and out. When asymmetric, the negative samples' curve is drawn inverted below
/// a centre line at −96 dB.
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
        vertical(raster, frame, x_of(db), GRID);
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
            horizontal(raster, frame, y_of(db, side), GRID);
            // The shared centre line is labelled once.
            if !(side == 1 && db == BOTTOM_DB) {
                label_beside(labels, frame, face, &db_label(db), y_of(db, side));
            }
            db -= step;
        }
    }
    // Over the grid: 0 dB in, and 0 dB out on each curve's half.
    vertical(raster, frame, x_of(0.0), ZERO_DB_LINE);
    for &side in halves {
        horizontal(raster, frame, y_of(0.0, side), ZERO_DB_LINE);
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

    /// The `(column, row)` of every pixel matching `keep`.
    fn pixels_where(pixels: &Pixels, keep: impl Fn(&[u8; 4]) -> bool) -> Vec<(usize, usize)> {
        let width = pixels.width() as usize;
        pixels
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| keep(p))
            .map(|(i, _)| (i % width, i / width))
            .collect()
    }

    /// Whether `p` is the 0 dB lines' red, alone or blended with the grey grid: green and blue
    /// equal, red above them.
    fn is_zero_line(p: &[u8; 4]) -> bool {
        p[3] > 0 && p[1].abs_diff(p[2]) <= 2 && p[0] > p[1].saturating_add(16)
    }

    /// The `(column, row)` of every pixel tinted like the curves: grid and labels are greys, and
    /// the 0 dB lines red.
    fn accent(pixels: &Pixels) -> Vec<(usize, usize)> {
        pixels_where(pixels, |p| p[3] > 0 && p[0] != p[2] && !is_zero_line(p))
    }

    #[test]
    fn test_draw_marks_zero_db_in_red() -> Result<(), Box<dyn std::error::Error>> {
        let frame = Frame::half((800.0 + GAP) / 2.0, 800.0);
        let range = TOP_DB - BOTTOM_DB;
        let x = frame.left + -BOTTOM_DB / range * frame.width;
        let y = frame.bottom() - -BOTTOM_DB / range * frame.height;
        let near = |a: usize, b: f32| (a as f32 - b).abs() <= 1.0;
        let red = pixels_where(&draw(&[]).ok_or("nothing drawn")?, is_zero_line);
        assert!(red.iter().any(|&(px, py)| near(px, x) && py > 200));
        assert!(
            red.iter()
                .any(|&(px, py)| near(py, y) && px > 450 && px < 550)
        );
        // None in the envelope plot.
        assert!(red.iter().all(|&(px, _)| px as f32 >= frame.left - 1.0));
        // Asymmetric: a line on each half.
        let red = pixels_where(
            &draw(&[(DynamicsControl::Asymmetric, 1.0)]).ok_or("nothing drawn")?,
            is_zero_line,
        );
        let centre = frame.top + frame.height / 2.0;
        let reach = -BOTTOM_DB / range * frame.height / 2.0;
        for row in [centre - reach, centre + reach] {
            assert!(
                red.iter()
                    .any(|&(px, py)| near(py, row) && px > 450 && px < 550)
            );
        }
        Ok(())
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
    fn test_draw_envelope_widths_follow_attack_and_release()
    -> Result<(), Box<dyn std::error::Error>> {
        // 10 ms attack, 90 ms release, no lookahead: the peak sits 90% across the envelope plot,
        // at the top, so the attack side is a tenth of its width.
        let pixels = draw(&[
            (DynamicsControl::Attack, 10.0),
            (DynamicsControl::Release, 90.0),
        ])
        .ok_or("nothing drawn")?;
        let frame = Frame::half(0.0, 800.0);
        let top: Vec<usize> = accent(&pixels)
            .into_iter()
            .filter(|&(x, y)| y < 4 && (x as f32) < frame.right())
            .map(|(x, _)| x)
            .collect();
        let peak = top.iter().sum::<usize>() as f32 / top.len().max(1) as f32;
        let expected = frame.left + 0.9 * frame.width;
        assert!((peak - expected).abs() < 3.0, "{peak} vs {expected}");
        Ok(())
    }

    #[test]
    fn test_output_db_includes_output_gain_and_ceiling() {
        let settings = Settings::from(&[]);
        // The curve fixes the floor and full scale.
        assert!(settings.output_db(0.0, 0).abs() < 1e-3);
        assert!(settings.output_db(-60.0, 0) > -60.0);
        let loud = Settings {
            curves: [(1.0, 1.0, 24.0, 0.0), (1.0, 1.0, -6.0, 0.0)],
            ..settings
        };
        assert!((loud.output_db(0.0, 0) - 0.0).abs() < 1e-3);
        assert!((loud.output_db(0.0, 1) + 6.0).abs() < 1e-3);
    }

    #[test]
    fn test_settings_negative_side_follows_asymmetric() {
        let parameter = |control: DynamicsControl, value| DisplayParameter {
            name: control.name(),
            value,
            unit: "",
        };
        let gain = [
            parameter(DynamicsControl::PositiveOutputGain, 3.0),
            parameter(DynamicsControl::NegativeOutputGain, -9.0),
        ];
        assert_eq!(Settings::from(&gain).curves[1].2, 3.0);
        let mut asymmetric = gain.to_vec();
        asymmetric.push(parameter(DynamicsControl::Asymmetric, 1.0));
        assert_eq!(Settings::from(&asymmetric).curves[1].2, -9.0);
    }

    #[test]
    fn test_ms_label_signs() {
        assert_eq!(ms_label(-110.0), "\u{2212}110ms");
        assert_eq!(ms_label(0.0), "0ms");
        assert_eq!(ms_label(10.0), "+10ms");
    }
}
