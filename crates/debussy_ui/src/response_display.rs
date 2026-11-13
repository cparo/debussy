//! The equalisers' display: the magnitude response their parameters ask for, on fixed axes of
//! 20 Hz to 20 kHz and +12 to −96 dB (`docs/architecture/effect-displays.md` "Equaliser
//! displays", drawn as in "Display plots").

use audio_effects::{FrequencyResponse, frequency_response};
use iced::Color;

use crate::effect_display::{DisplayContext, DisplayParameter, EffectDisplay, Pixels};
use crate::plot::{Axis, BOTTOM_DB, Canvas, Plot, columns, db_lines, db_marks};
use crate::theme::{self, GRID, GRID_ZERO};

/// The frequencies of the vertical grid lines, in Hz, and their labels.
const FREQUENCY_LINES: [(f32, &str); 10] = [
    (20.0, "20Hz"),
    (50.0, "50Hz"),
    (100.0, "100Hz"),
    (200.0, "200Hz"),
    (500.0, "500Hz"),
    (1_000.0, "1kHz"),
    (2_000.0, "2kHz"),
    (5_000.0, "5kHz"),
    (10_000.0, "10kHz"),
    (20_000.0, "20kHz"),
];

/// The frequency axis: 20 Hz to 20 kHz on a log scale.
fn frequencies() -> Axis {
    Axis::log(20.0, 20_000.0)
}

/// The display of every equaliser kind: its frequency response.
pub(crate) struct ResponseDisplay;

impl EffectDisplay for ResponseDisplay {
    fn draw(
        &self,
        parameters: &[DisplayParameter<'_>],
        context: &DisplayContext<'_>,
    ) -> Option<Pixels> {
        let values: Vec<f32> = parameters.iter().map(|p| p.value).collect();
        // Sample rates are exact in `f32` well past any audio rate.
        #[allow(clippy::cast_precision_loss)]
        let response = frequency_response(context.kind, &values, context.sample_rate as f32)?;
        render(&response, context)
    }
}

/// `response` drawn on its grid for `context`, on a transparent background: grey lines at the
/// marked frequencies and every 12 dB, the 0 dB line lighter.
fn render(response: &FrequencyResponse, context: &DisplayContext<'_>) -> Option<Pixels> {
    let mut canvas = Canvas::new(context)?;
    let plot = canvas.plot(frequencies(), Axis::decibels());
    let verticals: Vec<(f32, Color)> = FREQUENCY_LINES.iter().map(|&(f, _)| (f, GRID)).collect();
    let horizontals: Vec<(f32, Color)> = db_lines()
        .map(|db| (db, if db == 0.0 { GRID_ZERO } else { GRID }))
        .collect();
    canvas.grid(&plot, &verticals, &horizontals);
    canvas.labels(&plot, &FREQUENCY_LINES, &db_marks());
    let points = points(&plot, response, canvas.scale());
    canvas.curve(&plot, points, theme::tokens(context.tone).audio);
    canvas.finish()
}

/// The response's points, sampled at [`columns`] and at the centre of each notch. A notch's
/// centre (negative infinity) and NaN go to the bottom of the axis; the line stops at the
/// Nyquist frequency.
fn points(plot: &Plot, response: &FrequencyResponse, scale: f32) -> Vec<(f32, f32)> {
    let mut xs: Vec<f32> = columns(plot, scale).collect();
    xs.extend(
        response
            .zeros()
            .map(|frequency| plot.x(frequency))
            .filter(|x| (plot.left..=plot.right()).contains(x)),
    );
    xs.sort_by(f32::total_cmp);
    xs.into_iter()
        .map_while(|x| {
            let db = response.magnitude_db(plot.x_value(x))?;
            let db = if db.is_nan() { BOTTOM_DB } else { db };
            Some((x, plot.y(db)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect_display::DISPLAY_HEIGHT;
    use crate::plot::{BOTTOM_GUTTER, LEFT_GUTTER};
    use crate::theme::Tone;

    /// A context for `kind` at scale 1, 600 pixels wide.
    fn context(kind: &str) -> DisplayContext<'_> {
        DisplayContext {
            kind,
            tone: Tone::Dark,
            sample_rate: 48_000,
            width: 600,
            height: 320,
        }
    }

    /// A parametric equaliser's parameters with band 1 a bell of `gain` dB at 1 kHz, Q 1.
    /// The high-pass and low-pass stages are off.
    fn bell(gain: f32) -> Vec<f32> {
        let off = [0.0, 1_000.0, 2.0];
        let mut values = off.to_vec();
        values.extend_from_slice(&[1.0, 0.0, 1_000.0, gain, 1.0]);
        for _ in 1..audio_effects::BANDS {
            values.extend_from_slice(&[0.0, 0.0, 1_000.0, 0.0, 1.0]);
        }
        values.extend_from_slice(&off);
        values
    }

    /// The display of `kind` at `values`.
    fn draw(kind: &str, values: &[f32]) -> Option<Pixels> {
        let parameters: Vec<DisplayParameter<'_>> = values
            .iter()
            .map(|&value| DisplayParameter {
                name: "",
                value,
                unit: "",
            })
            .collect();
        ResponseDisplay.draw(&parameters, &context(kind))
    }

    /// The rows of every pixel in the plot's columns tinted like the curve: grid and labels are
    /// greys, so a pixel whose red and blue differ is the curve's.
    fn accent_rows(pixels: &Pixels) -> Vec<usize> {
        let width = pixels.width() as usize;
        pixels
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                let x = (i % width) as f32;
                x > LEFT_GUTTER + 4.0 && x < 600.0 - 4.0 && p[3] > 0 && p[0] != p[2]
            })
            .map(|(i, _)| i / width)
            .collect()
    }

    #[test]
    fn test_draw_flat_eq_lies_on_the_0_db_row() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw("parametric-eq", &bell(0.0)).ok_or("no picture")?;
        let zero = Plot::new(600.0, frequencies(), Axis::decibels()).y(0.0);
        let rows = accent_rows(&pixels);
        assert!(rows.len() > 500);
        for row in rows {
            assert!((row as f32 + 0.5 - zero).abs() < 2.5, "row {row}");
        }
        Ok(())
    }

    #[test]
    fn test_draw_clips_a_boost_to_the_top() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw("parametric-eq", &bell(24.0)).ok_or("no picture")?;
        let top = accent_rows(&pixels).into_iter().min().ok_or("no curve")?;
        assert_eq!(top, 0, "the clipped curve reaches the top edge");
        Ok(())
    }

    #[test]
    fn test_draw_notch_reaches_the_bottom() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = bell(0.0);
        // Band 1 follows the high-pass stage's three values.
        values[3..8].copy_from_slice(&[1.0, 3.0, 3_000.0, 0.0, 6.0]);
        let pixels = draw("parametric-eq", &values).ok_or("no picture")?;
        let lowest = accent_rows(&pixels).into_iter().max().ok_or("no curve")?;
        assert!(
            lowest as f32 >= DISPLAY_HEIGHT - BOTTOM_GUTTER - 1.0,
            "lowest {lowest}"
        );
        Ok(())
    }

    #[test]
    fn test_draw_needs_an_equaliser_kind() {
        assert!(draw("low-pass", &[1_000.0, 4.0]).is_some());
        assert!(draw("mixer", &[1_000.0, 4.0]).is_none());
    }
}
