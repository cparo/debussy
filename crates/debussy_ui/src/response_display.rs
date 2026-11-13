//! The equalisers' display: the magnitude response their parameters ask for, on fixed axes of
//! 20 Hz to 20 kHz and +12 to −96 dB (`docs/architecture/effect-displays.md` "Equaliser
//! displays", drawn as in "Display plots").

use audio_effects::FrequencyResponse;
use iced::Color;

use crate::effect_display::{DisplayContext, Pixels};
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

/// `response` drawn on its grid for `context`, on a transparent background: grey lines at the
/// marked frequencies and every 12 dB, the 0 dB line lighter.
pub(crate) fn render(response: &FrequencyResponse, context: &DisplayContext) -> Option<Pixels> {
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
    use audio_effects::Curve;

    use crate::effect_display::{DISPLAY_HEIGHT, values_with};
    use crate::plot::{BOTTOM_GUTTER, LEFT_GUTTER};
    use crate::theme::Tone;

    /// The display of a parametric equaliser at its defaults with `changes` by name, 600 pixels
    /// wide at scale 1.
    fn draw(changes: &[(&str, f32)]) -> Option<Pixels> {
        let values = values_with("parametric-eq", changes)?;
        let Some(Curve::Response(response)) =
            audio_effects::curve("parametric-eq", &values, 48_000, 96_000.0)
        else {
            return None;
        };
        let context = DisplayContext {
            tone: Tone::Dark,
            width: 600,
            height: 320,
        };
        render(&response, &context)
    }

    /// The changes that make band 1 a bell of `gain` dB at 1 kHz, Q 1.
    fn bell(gain: f32) -> [(&'static str, f32); 5] {
        [
            ("band 1 enabled", 1.0),
            ("band 1 type", 0.0),
            ("band 1 frequency", 1_000.0),
            ("band 1 gain", gain),
            ("band 1 q", 1.0),
        ]
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
        let pixels = draw(&bell(0.0)).ok_or("no picture")?;
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
        let pixels = draw(&bell(24.0)).ok_or("no picture")?;
        let top = accent_rows(&pixels).into_iter().min().ok_or("no curve")?;
        assert_eq!(top, 0, "the clipped curve reaches the top edge");
        Ok(())
    }

    #[test]
    fn test_draw_notch_reaches_the_bottom() -> Result<(), Box<dyn std::error::Error>> {
        let notch = [
            ("band 1 enabled", 1.0),
            ("band 1 type", 3.0),
            ("band 1 frequency", 3_000.0),
            ("band 1 q", 6.0),
        ];
        let pixels = draw(&notch).ok_or("no picture")?;
        let lowest = accent_rows(&pixels).into_iter().max().ok_or("no curve")?;
        assert!(
            lowest as f32 >= DISPLAY_HEIGHT - BOTTOM_GUTTER - 1.0,
            "lowest {lowest}"
        );
        Ok(())
    }
}
