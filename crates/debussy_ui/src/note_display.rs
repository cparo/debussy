//! The synthesizer's display: one note its parameters play, an A4 at full velocity held and then
//! released, as its waveform on the left and its spectrogram in absolute level on the right, over
//! one time axis (`docs/architecture/effect-displays.md` "Synth note display", drawn as in
//! "Display plots").

use audio_effects::SPECTROGRAM_MAX_COLUMNS;
use iced::Color;
use instruments::{Note, NoteCurve};

use crate::effect_display::{DisplayContext, Pixels};
use crate::gain_display::{FREQUENCY_MARKS, HIGH_HZ, LOW_HZ, cells};
use crate::plot::{Axis, Canvas, Plot, time_marks, time_step};
use crate::theme::{self, CHARCOAL, GRID, GRID_ZERO, SPECTRUM_KNEE, spectrum_colour};

/// The gap between the two halves, in logical pixels.
const HALF_GAP: f32 = 12.0;

/// The waveform's labelled levels.
const WAVE_MARKS: [(f32, &str); 3] = [(1.0, "+1"), (0.0, "0"), (-1.0, "\u{2212}1")];

/// The level at and below which a spectrogram cell is left as background, in dBFS.
const FLOOR_DBFS: f32 = -72.0;

/// The level drawn in the audio colour, in dBFS: the ramp's knee.
const KNEE_DBFS: f32 = -12.0;

/// The level drawn in the hot end's colour, in dBFS.
const HOT_DBFS: f32 = 0.0;

/// `note`, the rendering of `curve`, drawn for `context`: the waveform on the left, from −1 to
/// +1, and the spectrogram on the right, from 20 Hz up to 20 kHz on a log scale, both from the
/// note-on to the curve's span. A column of the waveform fills from the lowest sample in its
/// slice to the highest, values beyond ±1 at the edge; a spectrogram cell is coloured by its
/// level: background at −72 dBFS and below, the audio colour at −12 dBFS, the hot end at 0 dBFS.
pub(crate) fn render(curve: &NoteCurve, note: &Note, context: &DisplayContext) -> Option<Pixels> {
    let mut canvas = Canvas::new(context)?;
    let span = curve.span();
    let [wave, spectrum] = halves(&canvas, span);
    let scale = canvas.scale();
    let audio = theme::tokens(context.tone).audio;
    // A half's size in physical pixels: at most a few thousand each way.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (pixels, rows) = (
        (spectrum.width * scale).round().max(1.0) as usize,
        (spectrum.height * scale).round().max(1.0) as usize,
    );
    let columns = pixels.min(SPECTROGRAM_MAX_COLUMNS);
    let levels = note.levels(columns, rows, LOW_HZ, HIGH_HZ);
    cells(&mut canvas, &spectrum, pixels, columns, &levels, |db| {
        colour(db, audio)
    });
    let step = time_step(span);
    let marks = time_marks(span, step);
    let verticals: Vec<(f32, Color)> = marks.iter().map(|&(t, _)| (t, GRID)).collect();
    let levels: Vec<(f32, Color)> = WAVE_MARKS
        .iter()
        .map(|&(v, _)| (v, if v == 0.0 { GRID_ZERO } else { GRID }))
        .collect();
    let frequencies: Vec<(f32, Color)> =
        FREQUENCY_MARKS.iter().map(|&(hz, _)| (hz, GRID)).collect();
    canvas.grid(&wave, &verticals, &levels);
    canvas.grid(&spectrum, &verticals, &frequencies);
    waveform(&mut canvas, &wave, note.samples(), audio);
    canvas.labels(&wave, &marks, &WAVE_MARKS);
    canvas.labels(&spectrum, &marks, &FREQUENCY_MARKS);
    canvas.finish()
}

/// The canvas's two halves over `span` seconds, each with a left gutter of its own: the
/// waveform's plot, then the spectrogram's.
fn halves(canvas: &Canvas, span: f32) -> [Plot; 2] {
    let half = ((canvas.logical_width() - HALF_GAP) / 2.0).max(1.0);
    let time = Axis::linear(0.0, span);
    [
        canvas.plot_in(0.0, half, time, Axis::linear(-1.0, 1.0)),
        canvas.plot_in(half + HALF_GAP, half, time, Axis::log(LOW_HZ, HIGH_HZ)),
    ]
}

/// Fills one physical column of `plot` per equal slice of `samples`, from the slice's lowest
/// sample to its highest, at least one physical pixel tall, in `color`; values beyond ±1 sit on
/// the edge.
fn waveform(canvas: &mut Canvas, plot: &Plot, samples: &[f32], color: Color) {
    let count = samples.len();
    if count == 0 {
        return;
    }
    let scale = canvas.scale();
    let pixel = 1.0 / scale;
    // A plot's width in physical pixels: at most a few thousand.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = (plot.width * scale).round().max(1.0) as usize;
    for column in 0..pixels {
        let start = column * count / pixels;
        let end = ((column + 1) * count / pixels).clamp(start + 1, count);
        let Some(slice) = samples.get(start..end) else {
            break;
        };
        // `min` and `max` skip NaN; an all-NaN slice stays at ±∞ and draws nothing.
        let (low, high) = slice
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &s| {
                (lo.min(s), hi.max(s))
            });
        if low > high {
            continue;
        }
        let top = plot.y(high.clamp(-1.0, 1.0)).min(plot.bottom() - pixel);
        let bottom = plot.y(low.clamp(-1.0, 1.0)).max(top + pixel);
        // `column` is at most a few thousand.
        #[allow(clippy::cast_precision_loss)]
        let x = plot.left + column as f32 * pixel;
        canvas.fill(x, top, pixel, bottom - top, color);
    }
}

/// The colour of a cell at `db` dBFS on the spectrum ramp over the display's charcoal: `None`
/// (background) at [`FLOOR_DBFS`] and below and for NaN, the audio colour at [`KNEE_DBFS`], the
/// hot end at [`HOT_DBFS`] and above.
fn colour(db: f32, audio: Color) -> Option<Color> {
    if db.is_nan() || db <= FLOOR_DBFS {
        return None;
    }
    let t = if db <= KNEE_DBFS {
        SPECTRUM_KNEE * (db - FLOOR_DBFS) / (KNEE_DBFS - FLOOR_DBFS)
    } else {
        SPECTRUM_KNEE + (1.0 - SPECTRUM_KNEE) * ((db - KNEE_DBFS) / (HOT_DBFS - KNEE_DBFS)).min(1.0)
    };
    Some(spectrum_colour(t, CHARCOAL, audio))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::probe::pixel;
    use crate::theme::Tone;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The synth's note at its defaults with `changes` by name at `rate`, drawn 600 × 160
    /// logical pixels at scale 1, with the canvas's halves.
    fn draw(changes: &[(&str, f32)], rate: u32) -> Option<(NoteCurve, Pixels, [Plot; 2])> {
        let infos = instruments::parameter_infos("synth")?;
        let values: Vec<f32> = infos
            .iter()
            .map(|info| {
                changes
                    .iter()
                    .find(|(name, _)| *name == info.name)
                    .map_or(info.default, |&(_, v)| v)
            })
            .collect();
        let curve = instruments::note_curve("synth", &values, rate)?;
        let context = DisplayContext {
            tone: Tone::Dark,
            width: 600,
            height: 160,
            logical_height: 160,
        };
        let pixels = render(&curve, &curve.render(), &context)?;
        let plots = halves(&Canvas::new(&context)?, curve.span());
        Some((curve, pixels, plots))
    }

    #[test]
    fn test_render_returns_pixels_of_the_requested_size() -> TestResult {
        let curve = instruments::note_curve(
            "synth",
            &instruments::parameter_infos("synth")
                .ok_or("no synth")?
                .iter()
                .map(|i| i.default)
                .collect::<Vec<_>>(),
            48_000,
        )
        .ok_or("no curve")?;
        let note = curve.render();
        for (width, height, logical) in [(600, 160, 160), (1_200, 320, 160), (301, 240, 160)] {
            let context = DisplayContext {
                tone: Tone::Light,
                width,
                height,
                logical_height: logical,
            };
            let pixels = render(&curve, &note, &context).ok_or("no picture")?;
            assert_eq!((pixels.width(), pixels.height()), (width, height));
        }
        Ok(())
    }

    #[test]
    fn test_render_splits_the_width_in_two_halves() -> TestResult {
        let (_, _, [wave, spectrum]) = draw(&[], 48_000).ok_or("no picture")?;
        assert!(wave.right() + HALF_GAP <= spectrum.left);
        assert!((spectrum.right() - 600.0).abs() < 1e-3);
        assert!((wave.width - spectrum.width).abs() < 1e-3);
        assert!((wave.bottom() - spectrum.bottom()).abs() < 1e-3);
        Ok(())
    }

    #[test]
    fn test_render_draws_the_waveform_in_the_audio_colour() -> TestResult {
        // A loud, steady saw: mid-note, the column fills from near −1 to near +1.
        let changes = [("amp sustain", 1.0), ("gain", 0.0), ("cutoff", 20_000.0)];
        let (curve, pixels, [wave, _]) = draw(&changes, 48_000).ok_or("no picture")?;
        let audio = theme::tokens(Tone::Dark).audio.into_rgba8();
        let x = wave.x(curve.gate_off() / 2.0);
        for v in [-0.5, 0.25, 0.5] {
            assert_eq!(pixel(&pixels, x, wave.y(v)), audio, "{v}");
        }
        Ok(())
    }

    #[test]
    fn test_render_spectrogram_shows_the_note_below_nyquist_only() -> TestResult {
        let (curve, pixels, [_, spectrum]) = draw(&[], 22_050).ok_or("no picture")?;
        let x = spectrum.x(curve.gate_off() / 2.0);
        assert!(
            pixel(&pixels, x, spectrum.y(440.0))[3] > 0,
            "no fundamental"
        );
        assert_eq!(
            pixel(&pixels, x, spectrum.y(15_000.0))[3],
            0,
            "above Nyquist"
        );
        Ok(())
    }

    #[test]
    fn test_colour_ramp_ends() {
        let audio = theme::tokens(Tone::Dark).audio;
        assert_eq!(colour(FLOOR_DBFS, audio), None);
        assert_eq!(colour(f32::NAN, audio), None);
        assert_eq!(colour(f32::NEG_INFINITY, audio), None);
        assert_eq!(colour(KNEE_DBFS, audio), Some(audio));
        assert_eq!(colour(HOT_DBFS, audio), colour(20.0, audio));
    }
}
