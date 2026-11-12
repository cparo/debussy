//! The dynamics display: output level over detected input level, as the steady-state curve the
//! parameters ask for, on fixed axes of −96 to +12 dBFS each way
//! (`docs/architecture/effect-displays.md` "Dynamics display").

use audio_effects::{TransferCurve, dynamics_curve};
use iced::Color;

use crate::effect_display::{
    DISPLAY_HEIGHT, DisplayContext, DisplayParameter, EffectDisplay, Pixels,
};
use crate::raster::Raster;
use crate::response_display::{
    BOTTOM, BOTTOM_DB, CURVE_WIDTH, DB_STEP, GRID_WIDTH, LABEL_GAP, LABEL_SIZE, LEFT, TOP_DB,
    db_label, db_lines,
};
use crate::stroke::Mask;
use crate::theme::{self, FULL_SCALE, GRID, LABEL};
use crate::typeface::{StrokeFont, Typeface};

/// The space an input label needs between its neighbours' to be shown at every grid line, as a
/// multiple of the widest label's width; with less, every other line is labelled.
const LABEL_ROOM: f32 = 1.4;

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
        let curve = dynamics_curve(value);
        let (width, height) = (
            usize::try_from(context.width).ok()?,
            usize::try_from(context.height).ok()?,
        );
        let raster = render(&curve, context, width, height, &StrokeFont);
        Pixels::new(context.width, context.height, raster.into_rgba8())
    }
}

/// Where the plot lies, in logical pixels: input level across, output level up.
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

    /// The `x` of input level `db`.
    fn x(&self, db: f32) -> f32 {
        self.left + (db - BOTTOM_DB) / (TOP_DB - BOTTOM_DB) * self.width
    }

    /// The input level at `x`.
    fn input_db(&self, x: f32) -> f32 {
        BOTTOM_DB + (x - self.left) / self.width * (TOP_DB - BOTTOM_DB)
    }

    /// The `y` of output level `db`.
    fn y(&self, db: f32) -> f32 {
        (TOP_DB - db) / (TOP_DB - BOTTOM_DB) * self.height
    }

    /// The right edge.
    fn right(&self) -> f32 {
        self.left + self.width
    }
}

/// `curve` drawn on its grid for `context`, `width` × `height` physical pixels, labelled in
/// `face`. The background is left transparent.
fn render(
    curve: &TransferCurve,
    context: &DisplayContext<'_>,
    width: usize,
    height: usize,
    face: &dyn Typeface,
) -> Raster {
    // `height` is `DISPLAY_HEIGHT` logical pixels; at most a few thousand.
    #[allow(clippy::cast_precision_loss)]
    let scale = height as f32 / DISPLAY_HEIGHT;
    let mut raster = Raster::with_physical_size(width, height, scale);
    #[allow(clippy::cast_precision_loss)]
    let plot = Plot::new(width as f32 / scale);
    draw_grid(&mut raster, &plot);
    let mut labels = Mask::new(width, height, scale);
    for (text, x, baseline) in labels_at(&plot, face) {
        face.draw(&mut labels, &text, x, baseline, LABEL_SIZE);
    }
    raster.paint(&labels, LABEL);
    let mut line = Mask::new(width, height, scale);
    draw_curve(&mut line, &plot, curve, scale);
    raster.paint(&line, theme::tokens(context.tone).audio);
    raster
}

/// The grid every [`DB_STEP`] each way, with the 0 dBFS lines of both axes in red.
fn draw_grid(raster: &mut Raster, plot: &Plot) {
    // A line on the plot's edge is moved inward to show its whole width.
    let half = GRID_WIDTH / 2.0;
    let column = |db: f32| (plot.x(db) - half).clamp(plot.left, plot.right() - GRID_WIDTH);
    let row = |db: f32| (plot.y(db) - half).clamp(0.0, plot.height - GRID_WIDTH);
    for db in db_lines().filter(|&db| db != 0.0) {
        raster.fill_rect(column(db), 0.0, GRID_WIDTH, plot.height, GRID);
        raster.fill_rect(plot.left, row(db), plot.width, GRID_WIDTH, GRID);
    }
    let red: Color = FULL_SCALE;
    raster.fill_rect(column(0.0), 0.0, GRID_WIDTH, plot.height, red);
    raster.fill_rect(plot.left, row(0.0), plot.width, GRID_WIDTH, red);
}

/// Each label and where it goes: `(text, x, baseline)` in logical pixels. Output levels are
/// right-aligned in the left gutter, centred on their lines; input levels are centred under
/// theirs, every line if there is room and every other one if not. A label that would cross the
/// frame's edge is moved inward.
fn labels_at(plot: &Plot, face: &dyn Typeface) -> Vec<(String, f32, f32)> {
    let mut labels = Vec::new();
    let widest = db_lines()
        .map(|db| face.advance(&db_label(db), LABEL_SIZE))
        .fold(0.0, f32::max);
    let spacing = plot.width * DB_STEP / (TOP_DB - BOTTOM_DB);
    let every = if spacing >= widest * LABEL_ROOM { 1 } else { 2 };
    let baseline = plot.height + LABEL_GAP + LABEL_SIZE;
    // From the bottom of the range, so −96, −72, … keep their labels when thinned.
    let mut inputs: Vec<f32> = db_lines().collect();
    inputs.reverse();
    for db in inputs.into_iter().step_by(every) {
        let label = db_label(db);
        let advance = face.advance(&label, LABEL_SIZE);
        let x = (plot.x(db) - advance / 2.0)
            .min(plot.right() - advance)
            .max(plot.left);
        labels.push((label, x, baseline));
    }
    // The stroke's half-width, kept inside the frame too.
    let margin = 1.0;
    for db in db_lines() {
        let label = db_label(db);
        let x = plot.left - LABEL_GAP - face.advance(&label, LABEL_SIZE);
        let baseline = (plot.y(db) + LABEL_SIZE / 2.0).max(LABEL_SIZE + margin);
        labels.push((label, x, baseline));
    }
    labels
}

/// The curve as a polyline, sampled twice per physical column. Output levels beyond the axes are
/// pinned to the plot's edge, the line kept wholly inside it.
fn draw_curve(mask: &mut Mask, plot: &Plot, curve: &TransferCurve, scale: f32) {
    // A few thousand points at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (plot.width * scale * 2.0).ceil().max(1.0) as u32;
    let half = CURVE_WIDTH / 2.0;
    let mut previous: Option<(f32, f32)> = None;
    for step in 0..=steps {
        #[allow(clippy::cast_precision_loss)]
        let x = plot.left + plot.width * step as f32 / steps as f32;
        let db = curve.output_db(plot.input_db(x));
        let db = if db.is_nan() {
            BOTTOM_DB
        } else {
            db.clamp(BOTTOM_DB, TOP_DB)
        };
        let y = plot.y(db).clamp(half, plot.height - half);
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

    /// The display at the `dynamics` defaults, with `changes` by name.
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
            width: WIDTH,
            height: 320,
        };
        TransferDisplay.draw(&parameters, &context)
    }

    /// The pixel at logical `(x, y)`, at scale 1.
    fn pixel(pixels: &Pixels, x: f32, y: f32) -> [u8; 4] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = (y as usize * pixels.width() as usize + x as usize) * 4;
        let p = &pixels.rgba()[i..i + 4];
        [p[0], p[1], p[2], p[3]]
    }

    /// The rows of the curve's pixels in column `x`: the tone's accent, not a grey or the red.
    fn curve_rows(pixels: &Pixels, x: f32) -> Vec<f32> {
        let accent = theme::tokens(Tone::Dark).audio.into_rgba8();
        (0..320)
            .map(|y| y as f32)
            .filter(|&y| pixel(pixels, x, y) == accent)
            .collect()
    }

    #[test]
    fn test_plot_axes_hit_the_corners() {
        let plot = Plot::new(600.0);
        assert!((plot.x(BOTTOM_DB) - LEFT).abs() < 1e-3);
        assert!((plot.x(TOP_DB) - 600.0).abs() < 1e-3);
        assert!(plot.y(TOP_DB).abs() < 1e-3);
        assert!((plot.y(BOTTOM_DB) - (DISPLAY_HEIGHT - BOTTOM)).abs() < 1e-3);
        assert!((plot.input_db(plot.x(-30.0)) - (-30.0)).abs() < 1e-3);
    }

    #[test]
    fn test_draw_follows_the_curve() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[]).ok_or("no picture")?;
        let plot = Plot::new(600.0);
        // Below the knee the default compressor passes the level unchanged.
        let x = plot.x(-48.0);
        let rows = curve_rows(&pixels, x);
        assert!(
            rows.iter().any(|&r| (r + 0.5 - plot.y(-48.0)).abs() < 2.0),
            "{rows:?}"
        );
        // At 0 dBFS in: −18 + 18/4 = −13.5 dBFS out.
        let rows = curve_rows(&pixels, plot.x(0.0) + 3.0);
        let expected = plot.y(-13.5 + 0.75);
        assert!(
            rows.iter().any(|&r| (r + 0.5 - expected).abs() < 2.0),
            "{rows:?}"
        );
        Ok(())
    }

    #[test]
    fn test_draw_includes_makeup_and_ceiling() -> Result<(), Box<dyn std::error::Error>> {
        let plot = Plot::new(600.0);
        let pixels = draw(&[
            ("ratio above", 1.0),
            ("makeup", 6.0),
            ("ceiling enabled", 1.0),
            ("ceiling", -6.0),
        ])
        .ok_or("no picture")?;
        // Quiet input: +6 dB makeup.
        let rows = curve_rows(&pixels, plot.x(-60.0));
        assert!(
            rows.iter().any(|&r| (r + 0.5 - plot.y(-54.0)).abs() < 2.0),
            "{rows:?}"
        );
        // Loud input: held at the ceiling.
        let rows = curve_rows(&pixels, plot.x(6.0));
        assert!(
            rows.iter().any(|&r| (r + 0.5 - plot.y(-6.0)).abs() < 2.0),
            "{rows:?}"
        );
        Ok(())
    }

    #[test]
    fn test_draw_marks_full_scale_in_red() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[("threshold", -80.0), ("ratio above", 1.0)]).ok_or("no picture")?;
        let plot = Plot::new(600.0);
        // A 1-pixel line off the pixel grid is shared between two columns or rows: look for
        // red-dominant coverage beside the point.
        let red = |x: f32, y: f32| {
            [-0.5_f32, 0.0, 0.5].iter().any(|d| {
                let p = pixel(&pixels, x + d, y);
                p[0] > 120 && p[1] < 80 && p[3] > 0
            })
        };
        assert!(red(plot.x(0.0), plot.y(-60.0)), "input 0 dBFS");
        let red_row = |x: f32, y: f32| {
            [-0.5_f32, 0.0, 0.5].iter().any(|d| {
                let p = pixel(&pixels, x, y + d);
                p[0] > 120 && p[1] < 80 && p[3] > 0
            })
        };
        assert!(red_row(plot.x(-60.0), plot.y(0.0)), "output 0 dBFS");
        assert!(!red(plot.x(-12.0), plot.y(-60.0)));
        Ok(())
    }

    #[test]
    fn test_draw_gate_pins_the_floor_to_the_bottom() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&[("ratio below", 100.0), ("threshold", -40.0)]).ok_or("no picture")?;
        let plot = Plot::new(600.0);
        let rows = curve_rows(&pixels, plot.x(-80.0));
        let lowest = rows.iter().copied().fold(0.0, f32::max);
        assert!(lowest >= plot.height - CURVE_WIDTH - 1.0, "{rows:?}");
        Ok(())
    }

    #[test]
    fn test_labels_stay_in_the_frame_and_thin_when_narrow() {
        let font = StrokeFont;
        for width in [600.0, 260.0] {
            let plot = Plot::new(width);
            let labels = labels_at(&plot, &font);
            for (text, x, baseline) in &labels {
                let right = x + font.advance(text, LABEL_SIZE);
                assert!(
                    *x >= 0.0 && right <= width + 1e-3,
                    "{width} {text}: {x}..{right}"
                );
                assert!(baseline - LABEL_SIZE >= 0.0 && *baseline <= DISPLAY_HEIGHT);
            }
            let inputs = labels.len() - db_lines().count();
            assert_eq!(inputs, if width > 400.0 { 10 } else { 5 }, "{width}");
        }
    }
}
