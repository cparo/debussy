//! The delay and reverb display: the output level over time after a unit impulse, as the peak
//! of each column in dB, in two stacked lanes, left above right
//! (`docs/architecture/effect-displays.md` "Impulse response display", drawn as in "Display
//! plots").

use audio_effects::ImpulseCurve;
use iced::Color;

use crate::effect_display::{DisplayContext, Pixels};
use crate::plot::{Axis, BOTTOM_DB, Canvas, LABEL_INSET, LABEL_SIZE, Plot, columns, db_label};
use crate::theme::{self, GRID, GRID_ZERO};

/// The gap between the two lanes, in logical pixels.
const LANE_GAP: f32 = 8.0;

/// The spacing of a lane's grid lines, in dB.
const LANE_DB_STEP: f32 = 24.0;

/// The levels labelled on each lane's axis, in dB.
const LANE_MARKS: [f32; 3] = [0.0, -48.0, BOTTOM_DB];

/// The level a lane's tag ("L" or "R") is centred on in the gutter, in dB: between two labels.
const TAG_DB: f32 = -24.0;

/// The most marks the time axis shows.
const MAX_TIME_MARKS: usize = 8;

/// `curve` drawn on its grid for `context`, on a transparent background: time across, from the
/// impulse to the curve's span; the left channel's level in the top lane and the right's in the
/// bottom, each from −96 dB up to 0 dB.
pub(crate) fn render(curve: &ImpulseCurve, context: &DisplayContext) -> Option<Pixels> {
    let mut canvas = Canvas::new(context)?;
    let span = curve.span();
    let [top, bottom] = lanes(canvas.plot(Axis::linear(0.0, span), lane_axis()));
    let step = time_step(span);
    let verticals: Vec<(f32, Color)> = time_marks(span, step)
        .into_iter()
        .map(|(t, _)| (t, GRID))
        .collect();
    let horizontals: Vec<(f32, Color)> = lane_lines()
        .map(|db| (db, if db == 0.0 { GRID_ZERO } else { GRID }))
        .collect();
    let lane_marks: Vec<(f32, String)> = LANE_MARKS.iter().map(|&db| (db, db_label(db))).collect();
    let none: &[(f32, String)] = &[];
    for lane in [&top, &bottom] {
        canvas.grid(lane, &verticals, &horizontals);
    }
    canvas.labels(&top, none, &lane_marks);
    canvas.labels(&bottom, &time_marks(span, step), &lane_marks);
    for (lane, tag) in [(&top, "L"), (&bottom, "R")] {
        canvas.text(tag, LABEL_INSET + 1.0, lane.y(TAG_DB) + LABEL_SIZE / 2.0);
    }
    let bins = columns(&top, canvas.scale()).count();
    let peaks = curve.peak_levels(bins);
    let accent = theme::tokens(context.tone).audio;
    for (lane, peaks) in [&top, &bottom].into_iter().zip(&peaks) {
        canvas.curve(lane, points(lane, span, peaks), accent);
    }
    canvas.finish()
}

/// A lane's vertical axis: from [`BOTTOM_DB`] to 0 dB.
fn lane_axis() -> Axis {
    Axis::linear(BOTTOM_DB, 0.0)
}

/// The levels of a lane's grid lines, from 0 dB down every [`LANE_DB_STEP`].
fn lane_lines() -> impl Iterator<Item = f32> {
    // A handful of lines.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = (-BOTTOM_DB / LANE_DB_STEP).round() as u16;
    (0..=count).map(|i| -f32::from(i) * LANE_DB_STEP)
}

/// `plot` split into two lanes, top then bottom, each half its height less half of
/// [`LANE_GAP`].
fn lanes(plot: Plot) -> [Plot; 2] {
    let height = ((plot.height - LANE_GAP) / 2.0).max(1.0);
    [
        Plot { height, ..plot },
        Plot {
            top: plot.top + height + LANE_GAP,
            height,
            ..plot
        },
    ]
}

/// The point of each of `peaks` (equal slices of `span` seconds) in `lane`: at its slice's
/// middle, at its level in dB, with silence and NaN at the bottom edge.
fn points<'a>(
    lane: &'a Plot,
    span: f32,
    peaks: &'a [f32],
) -> impl Iterator<Item = (f32, f32)> + 'a {
    // At most a few thousand slices.
    #[allow(clippy::cast_precision_loss)]
    let count = peaks.len() as f32;
    peaks.iter().enumerate().map(move |(i, &peak)| {
        #[allow(clippy::cast_precision_loss)]
        let t = (i as f32 + 0.5) / count * span;
        let db = 20.0 * peak.log10();
        // `max` turns NaN and silence (−∞) into the bottom edge.
        (lane.x(t), lane.y(db.max(BOTTOM_DB)))
    })
}

/// The time axis's mark spacing for a span of `span` seconds: the smallest 1, 2 or 5 × 10ᵏ
/// seconds that gives at most [`MAX_TIME_MARKS`] marks, so at least four.
fn time_step(span: f32) -> f32 {
    let span = if span.is_finite() && span > 0.0 {
        span
    } else {
        1.0
    };
    // At most `MAX_TIME_MARKS` − 1 steps across the span.
    #[allow(clippy::cast_precision_loss)]
    let rough = span / (MAX_TIME_MARKS - 1) as f32;
    let decade = 10.0_f32.powf(rough.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * decade)
        .find(|&step| step >= rough)
        .unwrap_or(10.0 * decade)
}

/// The time axis's marks: every `step` seconds from 0 to `span`, each with its
/// [`time_label`].
fn time_marks(span: f32, step: f32) -> Vec<(f32, String)> {
    // A handful of marks.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = (span / step + 1e-3).floor().max(0.0) as u16;
    (0..=count)
        .map(|i| {
            let t = f32::from(i) * step;
            (t, time_label(t))
        })
        .collect()
}

/// The label of a time, such as `"0"`, `"250ms"`, `"1s"` or `"1.5s"`.
fn time_label(seconds: f32) -> String {
    // Display times: at most 20 s, to the millisecond.
    #[allow(clippy::cast_possible_truncation)]
    let ms = (seconds * 1000.0).round() as i32;
    if ms == 0 {
        "0".to_owned()
    } else if ms < 1000 {
        format!("{ms}ms")
    } else if ms % 1000 == 0 {
        format!("{}s", ms / 1000)
    } else if ms % 100 == 0 {
        format!("{}.{}s", ms / 1000, (ms % 1000) / 100)
    } else {
        format!("{}.{:02}s", ms / 1000, (ms % 1000) / 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_effects::Curve;

    use crate::effect_display::values_with;
    use crate::plot::probe::pixel;
    use crate::theme::Tone;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The lanes of a display 600 logical pixels wide over `span` seconds.
    fn lanes_of(span: f32) -> [Plot; 2] {
        lanes(Plot::new(600.0, Axis::linear(0.0, span), lane_axis()))
    }

    /// The `delay` curve at its defaults with `changes` by name, and its picture 600 pixels wide
    /// at scale 1.
    fn draw(changes: &[(&str, f32)]) -> Option<(ImpulseCurve, Pixels)> {
        let values = values_with("delay", changes)?;
        let Some(Curve::Impulse(curve)) = audio_effects::curve("delay", &values, 48_000, 96_000.0)
        else {
            return None;
        };
        let context = DisplayContext {
            tone: Tone::Dark,
            width: 600,
            height: 320,
        };
        let pixels = render(&curve, &context)?;
        Some((curve, pixels))
    }

    /// The topmost row within `lane` in columns `x − 1` to `x + 1` painted mostly opaque in the
    /// dark tone's audio colour, or `None`.
    fn tip(pixels: &Pixels, lane: &Plot, x: f32) -> Option<f32> {
        let audio = theme::tokens(Tone::Dark).audio.into_rgba8();
        let mut rows = (lane.top as u16..lane.bottom() as u16).map(f32::from);
        rows.find(|&y| {
            [-1.0, 0.0, 1.0].iter().any(|d| {
                let p = pixel(pixels, x + d, y);
                p[..3] == audio[..3] && p[3] > 128
            })
        })
    }

    #[test]
    fn test_render_draws_each_echo_at_its_level() -> TestResult {
        let (curve, pixels) =
            draw(&[("time", 100.0), ("feedback", 50.0), ("mix", 50.0)]).ok_or("no picture")?;
        let [top, bottom] = lanes_of(curve.span());
        // Wet echoes at half: −6 dB, then −12 dB.
        for (t, db) in [(0.1, -6.02), (0.2, -12.04)] {
            for lane in [&top, &bottom] {
                let row = tip(&pixels, lane, lane.x(t)).ok_or("no curve")?;
                assert!(
                    (row - lane.y(db)).abs() < 2.5,
                    "{t}: {row} vs {}",
                    lane.y(db)
                );
            }
        }
        // Between echoes, nothing rises above the floor.
        let row = tip(&pixels, &top, top.x(0.15)).ok_or("no floor")?;
        assert!(row > top.y(-90.0), "{row}");
        Ok(())
    }

    #[test]
    fn test_render_shows_the_offset_in_the_right_lane() -> TestResult {
        let (curve, pixels) = draw(&[
            ("time", 100.0),
            ("feedback", 0.0),
            ("mix", 100.0),
            ("offset", 30.0),
        ])
        .ok_or("no picture")?;
        let [top, bottom] = lanes_of(curve.span());
        let high = |lane: &Plot, t: f32| {
            tip(&pixels, lane, lane.x(t)).is_some_and(|row| row < lane.y(-6.0))
        };
        assert!(high(&top, 0.1) && !high(&top, 0.13));
        assert!(high(&bottom, 0.13) && !high(&bottom, 0.1));
        Ok(())
    }

    #[test]
    fn test_lanes_stack_with_a_gap() {
        let plot = Plot::new(600.0, Axis::linear(0.0, 1.0), lane_axis());
        let [top, bottom] = lanes(plot);
        assert_eq!(top.top, plot.top);
        assert!((bottom.top - top.bottom() - LANE_GAP).abs() < 1e-4);
        assert!((bottom.bottom() - plot.bottom()).abs() < 1e-4);
    }

    #[test]
    fn test_lane_lines_every_24_db() {
        assert_eq!(
            lane_lines().collect::<Vec<_>>(),
            [0.0, -24.0, -48.0, -72.0, -96.0]
        );
    }

    #[test]
    fn test_time_step_gives_four_to_eight_marks() {
        for span in [0.05, 0.07, 0.26, 1.0, 1.7, 3.21, 7.5, 20.0] {
            let step = time_step(span);
            let marks = time_marks(span, step).len();
            assert!(
                (4..=MAX_TIME_MARKS).contains(&marks),
                "{span}: {step} {marks}"
            );
        }
        assert!((time_step(1.7) - 0.5).abs() < 1e-6);
        assert!((time_step(0.05) - 0.01).abs() < 1e-6);
        assert!((time_step(f32::NAN) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn test_time_label_formats() {
        assert_eq!(
            [0.0, 0.01, 0.25, 1.0, 1.5, 2.25].map(time_label),
            ["0", "10ms", "250ms", "1s", "1.5s", "2.25s"]
        );
    }
}
