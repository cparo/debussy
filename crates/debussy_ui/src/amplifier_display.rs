//! The amplifier display: the shaper's static transfer curve, output sample over input sample, on
//! linear axes of ±3 dBFS each way (`docs/architecture/effect-displays.md` "Amplifier display").

use audio_effects::{ShaperCurve, amplifier_curve};

use crate::effect_display::{
    DISPLAY_HEIGHT, DisplayContext, DisplayParameter, EffectDisplay, Pixels,
};
use crate::raster::Raster;
use crate::response_display::{BOTTOM, CURVE_WIDTH, GRID_WIDTH, LABEL_GAP, LABEL_SIZE, LEFT};
use crate::stroke::Mask;
use crate::theme::{self, FULL_SCALE, GRID, LABEL};
use crate::typeface::{StrokeFont, Typeface};

/// The axes' extent each way from 0: +3 dBFS as a sample value, 10^(3/20).
const EDGE: f32 = 1.412_537_5;

/// The label at full scale.
const FULL_SCALE_LABEL: &str = "0dB";

/// The label at the axes' edges.
const EDGE_LABEL: &str = "+3dB";

/// The display of the `amplifier` kind: its shaper curve.
pub(crate) struct AmplifierDisplay;

impl EffectDisplay for AmplifierDisplay {
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
        let curve = amplifier_curve(value);
        let (width, height) = (
            usize::try_from(context.width).ok()?,
            usize::try_from(context.height).ok()?,
        );
        let raster = render(&curve, context, width, height, &StrokeFont);
        Pixels::new(context.width, context.height, raster.into_rgba8())
    }
}

/// Where the plot lies, in logical pixels: input sample across, output sample up.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Plot {
    left: f32,
    width: f32,
    height: f32,
}

impl Plot {
    /// The plot in a display `width` logical pixels wide and [`DISPLAY_HEIGHT`] tall, running to
    /// the top and right edges.
    fn new(width: f32) -> Plot {
        Plot {
            left: LEFT,
            width: (width - LEFT).max(1.0),
            height: DISPLAY_HEIGHT - BOTTOM,
        }
    }

    /// The `x` of input sample `v`.
    fn x(&self, v: f32) -> f32 {
        self.left + (v + EDGE) / (2.0 * EDGE) * self.width
    }

    /// The input sample at `x`.
    fn input(&self, x: f32) -> f32 {
        (x - self.left) / self.width * 2.0 * EDGE - EDGE
    }

    /// The `y` of output sample `v`.
    fn y(&self, v: f32) -> f32 {
        (EDGE - v) / (2.0 * EDGE) * self.height
    }

    /// The right edge.
    fn right(&self) -> f32 {
        self.left + self.width
    }
}

/// `curve` drawn on its grid for `context`, `width` × `height` physical pixels, labelled in
/// `face`. The background is left transparent.
fn render(
    curve: &ShaperCurve,
    context: &DisplayContext<'_>,
    width: usize,
    height: usize,
    face: &dyn Typeface,
) -> Raster {
    // `height` is `DISPLAY_HEIGHT` logical pixels; at most a few thousand.
    #[allow(clippy::cast_precision_loss)]
    let scale = height as f32 / DISPLAY_HEIGHT;
    let mut raster = Raster::with_physical_size(width, height, scale);
    // `width` is a display's width in physical pixels; at most a few thousand.
    #[allow(clippy::cast_precision_loss)]
    let plot = Plot::new(width as f32 / scale);
    draw_grid(&mut raster, &plot);
    let mut labels = Mask::new(width, height, scale);
    for (text, x, baseline) in labels_at(&plot, face) {
        face.draw(&mut labels, text, x, baseline, LABEL_SIZE);
    }
    raster.paint(&labels, LABEL);
    let mut line = Mask::new(width, height, scale);
    draw_curve(&mut line, &plot, curve, scale);
    raster.paint(&line, theme::tokens(context.tone).audio);
    raster
}

/// Grey lines through 0 each way and red lines at ±1 (0 dBFS) on both axes.
fn draw_grid(raster: &mut Raster, plot: &Plot) {
    let half = GRID_WIDTH / 2.0;
    let column = |v: f32| plot.x(v) - half;
    let row = |v: f32| plot.y(v) - half;
    raster.fill_rect(column(0.0), 0.0, GRID_WIDTH, plot.height, GRID);
    raster.fill_rect(plot.left, row(0.0), plot.width, GRID_WIDTH, GRID);
    for v in [-1.0, 1.0] {
        raster.fill_rect(column(v), 0.0, GRID_WIDTH, plot.height, FULL_SCALE);
        raster.fill_rect(plot.left, row(v), plot.width, GRID_WIDTH, FULL_SCALE);
    }
}

/// Each label and where it goes: `(text, x, baseline)` in logical pixels, at ±1 and the edges of
/// both axes. Output labels are right-aligned in the left gutter, centred on their lines; input
/// labels are centred under theirs. A label that would cross the frame's edge is moved inward.
fn labels_at(plot: &Plot, face: &dyn Typeface) -> Vec<(&'static str, f32, f32)> {
    let marks = [
        (-EDGE, EDGE_LABEL),
        (-1.0, FULL_SCALE_LABEL),
        (1.0, FULL_SCALE_LABEL),
        (EDGE, EDGE_LABEL),
    ];
    let mut labels = Vec::new();
    let baseline = plot.height + LABEL_GAP + LABEL_SIZE;
    for (v, label) in marks {
        let advance = face.advance(label, LABEL_SIZE);
        let x = (plot.x(v) - advance / 2.0)
            .min(plot.right() - advance)
            .max(plot.left);
        labels.push((label, x, baseline));
    }
    // The stroke's half-width, kept inside the frame too.
    let margin = 1.0;
    for (v, label) in marks {
        let x = plot.left - LABEL_GAP - face.advance(label, LABEL_SIZE);
        let baseline = (plot.y(v) + LABEL_SIZE / 2.0)
            .max(LABEL_SIZE + margin)
            .min(plot.height + LABEL_SIZE / 2.0);
        labels.push((label, x, baseline));
    }
    labels
}

/// The curve as a polyline, sampled twice per physical column. Outputs beyond the axes are pinned
/// to the plot's edge, the line kept wholly inside it.
fn draw_curve(mask: &mut Mask, plot: &Plot, curve: &ShaperCurve, scale: f32) {
    // A few thousand points at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (plot.width * scale * 2.0).ceil().max(1.0) as u32;
    let half = CURVE_WIDTH / 2.0;
    let mut previous: Option<(f32, f32)> = None;
    for step in 0..=steps {
        // `step` and `steps` are at most a few thousand.
        #[allow(clippy::cast_precision_loss)]
        let x = plot.left + plot.width * step as f32 / steps as f32;
        let v = curve.output(plot.input(x));
        let v = if v.is_nan() {
            0.0
        } else {
            v.clamp(-EDGE, EDGE)
        };
        let y = plot.y(v).clamp(half, plot.height - half);
        let x = x.clamp(plot.left + half, plot.right() - half);
        if let Some((x0, y0)) = previous {
            mask.stroke_segment(x0, y0, x, y, CURVE_WIDTH);
        }
        previous = Some((x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Tone;

    /// A display 600 logical pixels wide at scale 1.
    const WIDTH: u32 = 600;

    /// The display at the `amplifier` defaults, with `changes` by name.
    fn draw(changes: &[(&str, f32)]) -> Option<Pixels> {
        let infos = audio_effects::parameter_infos("amplifier")?;
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
            kind: "amplifier",
            tone: Tone::Dark,
            sample_rate: 48_000,
            width: WIDTH,
            height: 320,
        };
        AmplifierDisplay.draw(&parameters, &context)
    }

    /// The pixel at logical `(x, y)`, at scale 1.
    fn pixel(pixels: &Pixels, x: f32, y: f32) -> [u8; 4] {
        // Test coordinates are small and non-negative.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = (y as usize * pixels.width() as usize + x as usize) * 4;
        let p = &pixels.rgba()[i..i + 4];
        [p[0], p[1], p[2], p[3]]
    }

    /// The rows of the curve's pixels in column `x`: the tone's accent.
    fn curve_rows(pixels: &Pixels, x: f32) -> Vec<f32> {
        let accent = theme::tokens(Tone::Dark).audio.into_rgba8();
        (0..320)
            .map(|y| y as f32)
            .filter(|&y| pixel(pixels, x, y) == accent)
            .collect()
    }

    /// Whether the curve passes within 2 pixels of output `v` at input `input`.
    fn passes(pixels: &Pixels, plot: &Plot, input: f32, v: f32) -> bool {
        curve_rows(pixels, plot.x(input))
            .iter()
            .any(|&r| (r + 0.5 - plot.y(v)).abs() < 2.0)
    }

    #[test]
    fn test_plot_axes_hit_the_corners() {
        let plot = Plot::new(600.0);
        assert!((plot.x(-EDGE) - LEFT).abs() < 1e-3);
        assert!((plot.x(EDGE) - 600.0).abs() < 1e-3);
        assert!(plot.y(EDGE).abs() < 1e-3);
        assert!((plot.y(-EDGE) - (DISPLAY_HEIGHT - BOTTOM)).abs() < 1e-3);
        assert!((plot.input(plot.x(0.3)) - 0.3).abs() < 1e-5);
        assert!((20.0 * EDGE.log10() - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_draw_follows_the_curve() -> Result<(), Box<dyn std::error::Error>> {
        let plot = Plot::new(600.0);
        for (shape, drive) in [(0.0, 0.0), (0.0, 12.0), (2.0, 0.0), (2.0, 12.0)] {
            let changes = [("shape", shape), ("drive", drive)];
            let pixels = draw(&changes).ok_or("no picture")?;
            let curve = amplifier_curve(|name| match name {
                "shape" => shape,
                "drive" => drive,
                _ => 0.0,
            });
            for input in [-1.2, -0.5, 0.25, 0.8] {
                let expected = curve.output(input);
                assert!(
                    passes(&pixels, &plot, input, expected),
                    "shape {shape} drive {drive} at {input}: {expected}"
                );
            }
        }
        // Harsh at +12 dB: clipped flat at 1 from about 0.25 in.
        let pixels = draw(&[("shape", 2.0), ("drive", 12.0)]).ok_or("no picture")?;
        assert!(passes(&pixels, &plot, 0.9, 1.0));
        Ok(())
    }

    #[test]
    fn test_draw_bias_still_passes_through_the_origin() -> Result<(), Box<dyn std::error::Error>> {
        let plot = Plot::new(600.0);
        let pixels = draw(&[("bias", 0.5), ("drive", 12.0)]).ok_or("no picture")?;
        assert!(passes(&pixels, &plot, 0.01, 0.0));
        Ok(())
    }

    #[test]
    fn test_draw_marks_full_scale_in_red() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[("makeup", -36.0)]).ok_or("no picture")?;
        let plot = Plot::new(600.0);
        let red = |p: [u8; 4]| p[0] > 120 && p[1] < 80 && p[3] > 0;
        let near = |x: f32, y: f32, across: bool| {
            [-0.5_f32, 0.0, 0.5].iter().any(|d| {
                red(if across {
                    pixel(&pixels, x + d, y)
                } else {
                    pixel(&pixels, x, y + d)
                })
            })
        };
        for v in [-1.0, 1.0] {
            assert!(near(plot.x(v), plot.y(0.5), true), "input {v}");
            assert!(near(plot.x(0.5), plot.y(v), false), "output {v}");
        }
        assert!(!near(plot.x(0.5), plot.y(0.5), true));
        // The centre lines are grey, not red.
        assert!(!near(plot.x(0.0), plot.y(0.5), true));
        Ok(())
    }

    #[test]
    fn test_labels_stay_in_the_frame() {
        let font = StrokeFont;
        for width in [600.0, 260.0] {
            let plot = Plot::new(width);
            let labels = labels_at(&plot, &font);
            assert_eq!(labels.len(), 8);
            for (text, x, baseline) in &labels {
                let right = x + font.advance(text, LABEL_SIZE);
                assert!(
                    *x >= 0.0 && right <= width + 1e-3,
                    "{width} {text}: {x}..{right}"
                );
                assert!(baseline - LABEL_SIZE >= 0.0 && *baseline <= DISPLAY_HEIGHT);
            }
        }
    }
}
