//! The amplifier display: the shaper's static transfer curve, output sample over input sample, on
//! linear axes of ±3 dBFS each way (`docs/architecture/effect-displays.md` "Amplifier display",
//! drawn as in "Display plots").

use audio_effects::ShaperCurve;

use crate::effect_display::{DisplayContext, Pixels};
use crate::plot::{Axis, Canvas, columns};
use crate::theme::{self, FULL_SCALE, GRID};

/// The axes' extent each way from 0: +3 dBFS as a sample value, 10^(3/20).
const EDGE: f32 = 1.412_537_5;

/// The labelled marks of both axes: full scale and the edges.
const MARKS: [(f32, &str); 4] = [(-EDGE, "+3dB"), (-1.0, "0dB"), (1.0, "0dB"), (EDGE, "+3dB")];

/// The grid of both axes: grey through 0 and red at ±1 (0 dBFS).
const LINES: [(f32, iced::Color); 3] = [(0.0, GRID), (-1.0, FULL_SCALE), (1.0, FULL_SCALE)];

/// Each axis: sample values from −[`EDGE`] to [`EDGE`].
fn sample_axis() -> Axis {
    Axis::linear(-EDGE, EDGE)
}

/// `curve` drawn on its grid for `context`, on a transparent background: input sample across and
/// output sample up.
pub(crate) fn render(curve: &ShaperCurve, context: &DisplayContext) -> Option<Pixels> {
    let mut canvas = Canvas::new(context)?;
    let plot = canvas.plot(sample_axis(), sample_axis());
    canvas.grid(&plot, &LINES, &LINES);
    canvas.labels(&plot, &MARKS, &MARKS);
    let points = columns(&plot, canvas.scale()).map(|x| {
        let v = curve.output(plot.x_value(x));
        (x, plot.y(if v.is_nan() { 0.0 } else { v }))
    });
    canvas.curve(&plot, points, theme::tokens(context.tone).audio);
    canvas.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_effects::Curve;

    use crate::effect_display::values_with;
    use crate::plot::Plot;
    use crate::plot::probe::{red_near, rows_of};
    use crate::theme::Tone;

    /// The plot of a display 600 logical pixels wide.
    fn plot() -> Plot {
        Plot::new(600.0, sample_axis(), sample_axis())
    }

    /// The display at the `amplifier` defaults, with `changes` by name, 600 pixels wide at
    /// scale 1.
    fn draw(changes: &[(&str, f32)]) -> Option<Pixels> {
        let context = DisplayContext {
            tone: Tone::Dark,
            width: 600,
            height: 320,
        };
        render(&shaper(changes)?, &context)
    }

    /// The `amplifier` curve at its defaults with `changes` by name.
    fn shaper(changes: &[(&str, f32)]) -> Option<ShaperCurve> {
        let values = values_with("amplifier", changes)?;
        match audio_effects::curve("amplifier", &values, 48_000)? {
            Curve::Shaper(curve) => Some(curve),
            _ => None,
        }
    }

    /// Whether the curve passes within 2 pixels of output `v` at input `input`.
    fn passes(pixels: &Pixels, input: f32, v: f32) -> bool {
        let accent = theme::tokens(Tone::Dark).audio.into_rgba8();
        rows_of(pixels, plot().x(input), accent)
            .iter()
            .any(|&r| (r + 0.5 - plot().y(v)).abs() < 2.0)
    }

    #[test]
    fn test_edge_is_plus_3_dbfs() {
        assert!((20.0 * EDGE.log10() - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_draw_follows_the_curve() -> Result<(), Box<dyn std::error::Error>> {
        for (shape, drive) in [(0.0, 0.0), (0.0, 12.0), (2.0, 0.0), (2.0, 12.0)] {
            let changes = [("shape", shape), ("drive", drive)];
            let pixels = draw(&changes).ok_or("no picture")?;
            let curve = shaper(&changes).ok_or("no curve")?;
            for input in [-1.2, -0.5, 0.25, 0.8] {
                let expected = curve.output(input);
                assert!(
                    passes(&pixels, input, expected),
                    "shape {shape} drive {drive} at {input}: {expected}"
                );
            }
        }
        // Harsh at +12 dB: clipped flat at 1 from about 0.25 in.
        let pixels = draw(&[("shape", 2.0), ("drive", 12.0)]).ok_or("no picture")?;
        assert!(passes(&pixels, 0.9, 1.0));
        Ok(())
    }

    #[test]
    fn test_draw_bias_still_passes_through_the_origin() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[("bias", 0.5), ("drive", 12.0)]).ok_or("no picture")?;
        assert!(passes(&pixels, 0.01, 0.0));
        Ok(())
    }

    #[test]
    fn test_draw_marks_full_scale_in_red() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[("makeup", -36.0)]).ok_or("no picture")?;
        let plot = plot();
        for v in [-1.0, 1.0] {
            assert!(red_near(&pixels, plot.x(v), plot.y(0.5), true), "input {v}");
            assert!(
                red_near(&pixels, plot.x(0.5), plot.y(v), false),
                "output {v}"
            );
        }
        assert!(!red_near(&pixels, plot.x(0.5), plot.y(0.5), true));
        // The centre lines are grey, not red.
        assert!(!red_near(&pixels, plot.x(0.0), plot.y(0.5), true));
        Ok(())
    }
}
