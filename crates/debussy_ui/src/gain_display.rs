//! The chorus and flanger display: the effect's gain over ten seconds of pink noise, by time and
//! frequency, as a spectrogram in two stacked lanes, left above right
//! (`docs/architecture/effect-displays.md` "Modulation display", drawn as in "Display plots").

use audio_effects::{SPECTROGRAM_MAX_COLUMNS, SPECTROGRAM_SPAN, SpectrogramCurve};
use iced::Color;

use crate::effect_display::{DisplayContext, Pixels};
use crate::plot::{Axis, Canvas, Plot, lanes, time_marks, time_step};
use crate::theme::{self, CHARCOAL, GRID, SPECTRUM_KNEE, spectrum_colour};

/// The lowest frequency on a lane's axis, in Hz.
const LOW_HZ: f32 = 20.0;

/// The highest frequency on a lane's axis, in Hz.
const HIGH_HZ: f32 = 20_000.0;

/// The frequencies of each lane's grid lines and labels, in Hz.
const FREQUENCY_MARKS: [(f32, &str); 3] =
    [(100.0, "100Hz"), (1_000.0, "1kHz"), (10_000.0, "10kHz")];

/// The frequency a lane's tag is centred on in the gutter, in Hz: between two labels.
const TAG_HZ: f32 = 3_162.0;

/// The gain at and below which a cell is left as background, in dB.
const FLOOR_DB: f32 = -48.0;

/// The gain drawn in the hot end's colour, in dB.
const HOT_DB: f32 = 12.0;

/// `curve` drawn on its grid for `context`: time across, from 0 to ten seconds; the left
/// channel's gain in the top lane and the right's in the bottom, each from 20 Hz up to 20 kHz on
/// a log scale. A cell is coloured by its gain: background at −48 dB and below, the audio colour
/// at 0 dB, the hot end at +12 dB.
pub(crate) fn render(curve: &SpectrogramCurve, context: &DisplayContext) -> Option<Pixels> {
    let mut canvas = Canvas::new(context)?;
    let span = SPECTROGRAM_SPAN;
    let [top, bottom] = lanes(canvas.plot(Axis::linear(0.0, span), lane_axis()));
    let scale = canvas.scale();
    // A lane's size in physical pixels: at most a few thousand each way.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (pixels, rows) = (
        (top.width * scale).round().max(1.0) as usize,
        (top.height * scale).round().max(1.0) as usize,
    );
    let columns = pixels.min(SPECTROGRAM_MAX_COLUMNS);
    let gains = curve.gains(columns, rows, LOW_HZ, HIGH_HZ);
    let audio = theme::tokens(context.tone).audio;
    for (lane, gains) in [&top, &bottom].into_iter().zip(&gains) {
        cells(&mut canvas, lane, pixels, columns, gains, audio);
    }
    let step = time_step(span);
    let verticals: Vec<(f32, Color)> = time_marks(span, step)
        .into_iter()
        .map(|(t, _)| (t, GRID))
        .collect();
    let horizontals: Vec<(f32, Color)> =
        FREQUENCY_MARKS.iter().map(|&(hz, _)| (hz, GRID)).collect();
    let none: &[(f32, String)] = &[];
    for lane in [&top, &bottom] {
        canvas.grid(lane, &verticals, &horizontals);
    }
    canvas.labels(&top, none, &FREQUENCY_MARKS);
    canvas.labels(&bottom, &time_marks(span, step), &FREQUENCY_MARKS);
    canvas.lane_tags([&top, &bottom], TAG_HZ);
    canvas.finish()
}

/// A lane's vertical axis: from [`LOW_HZ`] to [`HIGH_HZ`], log.
fn lane_axis() -> Axis {
    Axis::log(LOW_HZ, HIGH_HZ)
}

/// Fills `lane`, `pixels` physical pixels wide, with one cell per gain in `gains` (rows of
/// `columns`, the lowest band first), each coloured by [`colour`] and spanning whole physical
/// pixels; cells at the background are left transparent.
fn cells(
    canvas: &mut Canvas,
    lane: &Plot,
    pixels: usize,
    columns: usize,
    gains: &[f32],
    audio: Color,
) {
    if columns == 0 {
        return;
    }
    let rows = gains.len() / columns;
    let scale = canvas.scale();
    // The left edge of column `c`, on a physical pixel; a lane is at most a few thousand wide.
    #[allow(clippy::cast_precision_loss)]
    let edge = |c: usize| lane.left + (c * pixels / columns) as f32 / scale;
    #[allow(clippy::cast_precision_loss)]
    let height = lane.height / rows.max(1) as f32;
    for (row, line) in gains.chunks_exact(columns).enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let y = lane.bottom() - (row + 1) as f32 * height;
        for (column, &db) in line.iter().enumerate() {
            if let Some(color) = colour(db, audio) {
                let x = edge(column);
                canvas.fill(x, y, edge(column + 1) - x, height, color);
            }
        }
    }
}

/// The colour of a cell of gain `db` on the spectrum ramp over the display's charcoal: `None`
/// (background) at [`FLOOR_DB`] and below and for NaN, the audio colour at 0 dB, the hot end at
/// [`HOT_DB`] and above.
fn colour(db: f32, audio: Color) -> Option<Color> {
    if db.is_nan() || db <= FLOOR_DB {
        return None;
    }
    let t = if db <= 0.0 {
        SPECTRUM_KNEE * (db - FLOOR_DB) / -FLOOR_DB
    } else {
        SPECTRUM_KNEE + (1.0 - SPECTRUM_KNEE) * (db / HOT_DB).min(1.0)
    };
    Some(spectrum_colour(t, CHARCOAL, audio))
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_effects::Curve;

    use crate::effect_display::values_with;
    use crate::plot::probe::pixel;
    use crate::theme::Tone;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The `kind` curve at its defaults with `changes` by name at `rate`, its picture 300
    /// pixels wide at scale 1, and its lanes.
    fn draw(kind: &str, changes: &[(&str, f32)], rate: u32) -> Option<(Pixels, [Plot; 2])> {
        let values = values_with(kind, changes)?;
        let Some(Curve::Spectrogram(curve)) = audio_effects::curve(kind, &values, rate, 96_000.0)
        else {
            return None;
        };
        let context = DisplayContext {
            tone: Tone::Dark,
            width: 300,
            height: 320,
        };
        let pixels = render(&curve, &context)?;
        let lanes = lanes(Plot::new(
            300.0,
            Axis::linear(0.0, SPECTROGRAM_SPAN),
            lane_axis(),
        ));
        Some((pixels, lanes))
    }

    /// Whether the pixel at `(x, y)` is painted at all.
    fn painted(pixels: &Pixels, x: f32, y: f32) -> bool {
        pixel(pixels, x, y)[3] > 0
    }

    #[test]
    fn test_render_fills_both_lanes() -> TestResult {
        // A dry signal is 0 dB everywhere: the audio colour in every cell.
        let (pixels, lanes) = draw("flanger", &[("mix", 0.0)], 48_000).ok_or("no picture")?;
        let audio = theme::tokens(Tone::Dark).audio.into_rgba8();
        for lane in &lanes {
            // Away from the grid lines.
            for (t, hz) in [(0.3, 50.0), (4.5, 400.0), (9.7, 15_000.0)] {
                let p = pixel(&pixels, lane.x(t), lane.y(hz));
                assert_eq!(p, audio, "{t} s, {hz} Hz");
            }
        }
        Ok(())
    }

    #[test]
    fn test_render_leaves_rows_above_nyquist_empty() -> TestResult {
        let (pixels, [top, _]) = draw("chorus", &[], 22_050).ok_or("no picture")?;
        let x = top.x(4.5);
        assert!(painted(&pixels, x, top.y(5_000.0)));
        assert!(!painted(&pixels, x, top.y(15_000.0)));
        Ok(())
    }

    #[test]
    fn test_colour_ramp_ends() {
        let audio = theme::tokens(Tone::Dark).audio;
        assert_eq!(colour(FLOOR_DB, audio), None);
        assert_eq!(colour(f32::NAN, audio), None);
        assert_eq!(colour(f32::NEG_INFINITY, audio), None);
        assert_eq!(colour(0.0, audio), Some(audio));
        assert_eq!(colour(HOT_DB, audio), colour(40.0, audio));
    }
}
