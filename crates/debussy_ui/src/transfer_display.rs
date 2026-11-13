//! The dynamics display: output level over detected input level, as the steady-state curve the
//! parameters ask for, on fixed axes of −96 to +12 dBFS each way
//! (`docs/architecture/effect-displays.md` "Dynamics display", drawn as in "Display plots").

use audio_effects::{TransferCurve, dynamics_curve};
use iced::Color;

use crate::effect_display::{DisplayContext, DisplayParameter, EffectDisplay, Pixels};
use crate::plot::{
    Axis, BOTTOM_DB, Canvas, columns, db_lines, db_marks, draw_labels, grid, labels, polyline,
};
use crate::theme::{self, FULL_SCALE, GRID, LABEL};
use crate::typeface::StrokeFont;

/// The display of the `dynamics` kind: its transfer curve.
pub(crate) struct TransferDisplay;

impl EffectDisplay for TransferDisplay {
    fn draw(
        &self,
        parameters: &[DisplayParameter<'_>],
        context: &DisplayContext<'_>,
    ) -> Option<Pixels> {
        let value = |name: &str| {
            parameters
                .iter()
                .find(|p| p.name == name)
                .map_or(f32::NAN, |p| p.value)
        };
        render(&dynamics_curve(value), context)
    }
}

/// `curve` drawn on its grid for `context`, on a transparent background: input level across and
/// output level up, grey lines every 12 dB and the 0 dBFS lines of both axes in red.
fn render(curve: &TransferCurve, context: &DisplayContext<'_>) -> Option<Pixels> {
    let mut canvas = Canvas::new(context)?;
    let plot = canvas.plot(Axis::decibels(), Axis::decibels());
    let lines: Vec<(f32, Color)> = db_lines()
        .map(|db| (db, if db == 0.0 { FULL_SCALE } else { GRID }))
        .collect();
    grid(canvas.raster(), &plot, &lines, &lines);
    let mut text = canvas.mask();
    let marks = db_marks();
    draw_labels(
        &mut text,
        &StrokeFont,
        &labels(&plot, &StrokeFont, &marks, &marks),
    );
    canvas.paint(&text, LABEL);
    let mut line = canvas.mask();
    let points = columns(&plot, canvas.scale()).map(|x| {
        let db = curve.output_db(plot.x_value(x));
        (x, plot.y(if db.is_nan() { BOTTOM_DB } else { db }))
    });
    polyline(&mut line, &plot, points);
    canvas.paint(&line, theme::tokens(context.tone).audio);
    canvas.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::probe::{red_near, rows_of};
    use crate::plot::{CURVE_WIDTH, Plot};
    use crate::theme::Tone;

    /// The plot of a display 600 logical pixels wide.
    fn plot() -> Plot {
        Plot::new(600.0, Axis::decibels(), Axis::decibels())
    }

    /// The display at the `dynamics` defaults, with `changes` by name, 600 pixels wide at scale 1.
    fn draw(changes: &[(&str, f32)]) -> Option<Pixels> {
        let infos = audio_effects::parameter_infos("dynamics")?;
        let parameters: Vec<DisplayParameter<'_>> = infos
            .iter()
            .map(|info| DisplayParameter {
                name: &info.name,
                value: changes
                    .iter()
                    .find(|(n, _)| *n == info.name)
                    .map_or(info.default, |&(_, v)| v),
                unit: &info.unit,
            })
            .collect();
        let context = DisplayContext {
            kind: "dynamics",
            tone: Tone::Dark,
            sample_rate: 48_000,
            width: 600,
            height: 320,
        };
        TransferDisplay.draw(&parameters, &context)
    }

    /// Whether the curve passes within 2 pixels of output `db` in column `x`.
    fn passes(pixels: &Pixels, x: f32, db: f32) -> bool {
        let accent = theme::tokens(Tone::Dark).audio.into_rgba8();
        rows_of(pixels, x, accent)
            .iter()
            .any(|&r| (r + 0.5 - plot().y(db)).abs() < 2.0)
    }

    #[test]
    fn test_draw_follows_the_curve() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[]).ok_or("no picture")?;
        let plot = plot();
        // Below the knee the default compressor passes the level unchanged.
        assert!(passes(&pixels, plot.x(-48.0), -48.0));
        // At 0 dBFS in: −18 + 18/4 = −13.5 dBFS out.
        assert!(passes(&pixels, plot.x(0.0) + 3.0, -13.5 + 0.75));
        Ok(())
    }

    #[test]
    fn test_draw_includes_makeup_and_ceiling() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[
            ("ratio above", 1.0),
            ("makeup", 6.0),
            ("ceiling enabled", 1.0),
            ("ceiling", -6.0),
        ])
        .ok_or("no picture")?;
        // Quiet input: +6 dB makeup.
        assert!(passes(&pixels, plot().x(-60.0), -54.0));
        // Loud input: held at the ceiling.
        assert!(passes(&pixels, plot().x(6.0), -6.0));
        Ok(())
    }

    #[test]
    fn test_draw_marks_full_scale_in_red() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[("threshold", -80.0), ("ratio above", 1.0)]).ok_or("no picture")?;
        let plot = plot();
        assert!(
            red_near(&pixels, plot.x(0.0), plot.y(-60.0), true),
            "input 0 dBFS"
        );
        assert!(
            red_near(&pixels, plot.x(-60.0), plot.y(0.0), false),
            "output 0 dBFS"
        );
        assert!(!red_near(&pixels, plot.x(-12.0), plot.y(-60.0), true));
        Ok(())
    }

    #[test]
    fn test_draw_gate_pins_the_floor_to_the_bottom() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[("ratio below", 100.0), ("threshold", -40.0)]).ok_or("no picture")?;
        let plot = plot();
        let accent = theme::tokens(Tone::Dark).audio.into_rgba8();
        let rows = rows_of(&pixels, plot.x(-80.0), accent);
        let lowest = rows.iter().copied().fold(0.0, f32::max);
        assert!(lowest >= plot.height - CURVE_WIDTH - 1.0, "{rows:?}");
        Ok(())
    }
}
