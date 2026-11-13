//! The plot toolkit the effect displays draw with: the frame's geometry, axes, grid, labels and
//! the clipped polyline (`docs/architecture/effect-displays.md` "Display plots", ADR-0104). Each
//! display's `render` calls these in order.

use iced::Color;

use crate::effect_display::{DISPLAY_HEIGHT, DisplayContext, Pixels};
use crate::raster::Raster;
use crate::stroke::Mask;
use crate::typeface::Typeface;

/// The left gutter, for the vertical axis's labels, in logical pixels.
pub(crate) const LEFT: f32 = 46.0;

/// The gutter below the plot, for the horizontal axis's labels, in logical pixels.
pub(crate) const BOTTOM: f32 = 20.0;

/// The labels' capital height, in logical pixels.
pub(crate) const LABEL_SIZE: f32 = 9.0;

/// The gap between a label and the plot or its line, in logical pixels.
pub(crate) const LABEL_GAP: f32 = 5.0;

/// The curve's width, in logical pixels.
pub(crate) const CURVE_WIDTH: f32 = 2.0;

/// The grid lines' width, in logical pixels.
pub(crate) const GRID_WIDTH: f32 = 1.0;

/// The space a label needs between its neighbours' to be shown at every mark, as a multiple of
/// the widest label's extent along the axis; with less, every other mark is labelled.
pub(crate) const LABEL_ROOM: f32 = 1.4;

/// The highest level on a decibel axis, in dB.
pub(crate) const TOP_DB: f32 = 12.0;

/// The lowest level on a decibel axis, in dB.
pub(crate) const BOTTOM_DB: f32 = -96.0;

/// The spacing of a decibel axis's grid, in dB.
pub(crate) const DB_STEP: f32 = 12.0;

/// How an axis spaces its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scale {
    /// Equal steps of value take equal space.
    Linear,
    /// Equal ratios of value take equal space; the axis's ends must be positive.
    Log,
}

/// The values an axis runs over, from `min` at its start to `max` at its end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Axis {
    /// The value at the axis's start (left or bottom).
    pub(crate) min: f32,
    /// The value at the axis's end (right or top).
    pub(crate) max: f32,
    /// How the values are spaced.
    pub(crate) scale: Scale,
}

impl Axis {
    /// A linear axis from `min` to `max`.
    pub(crate) fn linear(min: f32, max: f32) -> Axis {
        Axis {
            min,
            max,
            scale: Scale::Linear,
        }
    }

    /// A log axis from `min` to `max`, both positive.
    pub(crate) fn log(min: f32, max: f32) -> Axis {
        Axis {
            min,
            max,
            scale: Scale::Log,
        }
    }

    /// The linear axis from [`BOTTOM_DB`] to [`TOP_DB`].
    pub(crate) fn decibels() -> Axis {
        Axis::linear(BOTTOM_DB, TOP_DB)
    }

    /// How far along the axis `v` lies: 0 at `min`, 1 at `max`, beyond for values outside.
    pub(crate) fn fraction(&self, v: f32) -> f32 {
        match self.scale {
            Scale::Linear => (v - self.min) / (self.max - self.min),
            Scale::Log => (v / self.min).log10() / (self.max / self.min).log10(),
        }
    }

    /// The value `t` of the way along the axis; the inverse of [`Axis::fraction`].
    pub(crate) fn value_at(&self, t: f32) -> f32 {
        match self.scale {
            Scale::Linear => self.min + t * (self.max - self.min),
            Scale::Log => self.min * (self.max / self.min).powf(t),
        }
    }
}

/// The levels of a decibel axis's grid lines, from [`TOP_DB`] down every [`DB_STEP`].
pub(crate) fn db_lines() -> impl Iterator<Item = f32> {
    // A handful of lines.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = ((TOP_DB - BOTTOM_DB) / DB_STEP).round() as u16;
    (0..=count).map(|i| TOP_DB - f32::from(i) * DB_STEP)
}

/// The label of a level, such as `"+12dB"`, `"0dB"` or `"−24dB"`.
pub(crate) fn db_label(db: f32) -> String {
    if db > 0.0 {
        format!("+{db:.0}dB")
    } else if db < 0.0 {
        format!("\u{2212}{:.0}dB", -db)
    } else {
        "0dB".to_owned()
    }
}

/// Each of [`db_lines`] with its [`db_label`], as marks for [`labels`].
pub(crate) fn db_marks() -> Vec<(f32, String)> {
    db_lines().map(|db| (db, db_label(db))).collect()
}

/// Where the plot lies in its display, in logical pixels, and the axes it maps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Plot {
    /// The left edge.
    pub(crate) left: f32,
    /// The top edge.
    pub(crate) top: f32,
    /// The width.
    pub(crate) width: f32,
    /// The height.
    pub(crate) height: f32,
    /// The horizontal axis, left to right.
    pub(crate) x_axis: Axis,
    /// The vertical axis, bottom to top.
    pub(crate) y_axis: Axis,
}

impl Plot {
    /// The plot in a display `width` logical pixels wide and [`DISPLAY_HEIGHT`] tall: inside the
    /// [`LEFT`] and [`BOTTOM`] gutters, running to the top and right edges.
    pub(crate) fn new(width: f32, x_axis: Axis, y_axis: Axis) -> Plot {
        Plot {
            left: LEFT,
            top: 0.0,
            width: (width - LEFT).max(1.0),
            height: DISPLAY_HEIGHT - BOTTOM,
            x_axis,
            y_axis,
        }
    }

    /// The `x` of `v` on the horizontal axis.
    pub(crate) fn x(&self, v: f32) -> f32 {
        self.left + self.x_axis.fraction(v) * self.width
    }

    /// The `y` of `v` on the vertical axis.
    pub(crate) fn y(&self, v: f32) -> f32 {
        self.top + (1.0 - self.y_axis.fraction(v)) * self.height
    }

    /// The horizontal axis's value at `x`.
    pub(crate) fn x_value(&self, x: f32) -> f32 {
        self.x_axis.value_at((x - self.left) / self.width)
    }

    /// The right edge.
    pub(crate) fn right(&self) -> f32 {
        self.left + self.width
    }

    /// The bottom edge.
    pub(crate) fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

/// A display's picture while it is drawn: a transparent raster of the context's physical size,
/// at [`DISPLAY_HEIGHT`] logical pixels tall.
pub(crate) struct Canvas {
    /// The picture so far.
    raster: Raster,
    /// The width in physical pixels.
    width: usize,
    /// The height in physical pixels.
    height: usize,
    /// Physical pixels per logical pixel.
    scale: f32,
}

impl Canvas {
    /// A canvas of `context`'s size, if it fits in memory's index range.
    pub(crate) fn new(context: &DisplayContext<'_>) -> Option<Canvas> {
        let width = usize::try_from(context.width).ok()?;
        let height = usize::try_from(context.height).ok()?;
        // `height` is `DISPLAY_HEIGHT` logical pixels; at most a few thousand.
        #[allow(clippy::cast_precision_loss)]
        let scale = height as f32 / DISPLAY_HEIGHT;
        Some(Canvas {
            raster: Raster::with_physical_size(width, height, scale),
            width,
            height,
            scale,
        })
    }

    /// Physical pixels per logical pixel.
    pub(crate) fn scale(&self) -> f32 {
        self.scale
    }

    /// The plot of `x_axis` across and `y_axis` up, at this canvas's width.
    pub(crate) fn plot(&self, x_axis: Axis, y_axis: Axis) -> Plot {
        // `width` is a display's width in physical pixels; at most a few thousand.
        #[allow(clippy::cast_precision_loss)]
        Plot::new(self.width as f32 / self.scale, x_axis, y_axis)
    }

    /// The raster, to draw on directly (with [`grid`], for one).
    pub(crate) fn raster(&mut self) -> &mut Raster {
        &mut self.raster
    }

    /// An empty mask for a new layer, of this canvas's size.
    pub(crate) fn mask(&self) -> Mask {
        Mask::new(self.width, self.height, self.scale)
    }

    /// Paints `mask` in `color` over the picture so far.
    pub(crate) fn paint(&mut self, mask: &Mask, color: Color) {
        self.raster.paint(mask, color);
    }

    /// The finished picture.
    pub(crate) fn finish(self) -> Option<Pixels> {
        let width = u32::try_from(self.width).ok()?;
        let height = u32::try_from(self.height).ok()?;
        Pixels::new(width, height, self.raster.into_rgba8())
    }
}

/// Grid lines across the whole plot: a vertical line at each of `columns` (a horizontal-axis
/// value and its colour) and a horizontal one at each of `rows`. A line on the plot's edge is
/// moved inward to show its whole width.
pub(crate) fn grid(
    raster: &mut Raster,
    plot: &Plot,
    columns: &[(f32, Color)],
    rows: &[(f32, Color)],
) {
    let half = GRID_WIDTH / 2.0;
    for &(v, color) in columns {
        let x = (plot.x(v) - half).clamp(plot.left, plot.right() - GRID_WIDTH);
        raster.fill_rect(x, plot.top, GRID_WIDTH, plot.height, color);
    }
    for &(v, color) in rows {
        let y = (plot.y(v) - half).clamp(plot.top, plot.bottom() - GRID_WIDTH);
        raster.fill_rect(plot.left, y, plot.width, GRID_WIDTH, color);
    }
}

/// `marks` ordered from the low end of the axis, every other one dropped if any two neighbours
/// lie closer than `extent * LABEL_ROOM` apart at `position`.
fn thinned<S: AsRef<str>>(
    marks: &[(f32, S)],
    position: impl Fn(f32) -> f32,
    extent: f32,
) -> Vec<&(f32, S)> {
    let mut sorted: Vec<&(f32, S)> = marks.iter().collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let crowded = sorted
        .windows(2)
        .any(|pair| (position(pair[1].0) - position(pair[0].0)).abs() < extent * LABEL_ROOM);
    let every = if crowded { 2 } else { 1 };
    sorted.into_iter().step_by(every).collect()
}

/// Each label and where it goes: `(text, x, baseline)` in logical pixels. The `below` marks (a
/// horizontal-axis value and its text) are centred under the plot on their values; the `left`
/// marks are right-aligned in the left gutter, centred on theirs. Crowded marks on either axis
/// are thinned to every other one from the low end, and every label is kept inside the frame.
pub(crate) fn labels<B: AsRef<str>, L: AsRef<str>>(
    plot: &Plot,
    face: &dyn Typeface,
    below: &[(f32, B)],
    left: &[(f32, L)],
) -> Vec<(String, f32, f32)> {
    let mut labels = Vec::new();
    let widest = below
        .iter()
        .map(|(_, text)| face.advance(text.as_ref(), LABEL_SIZE))
        .fold(0.0, f32::max);
    let baseline = plot.bottom() + LABEL_GAP + LABEL_SIZE;
    for (v, text) in thinned(below, |v| plot.x(v), widest) {
        let text = text.as_ref();
        let advance = face.advance(text, LABEL_SIZE);
        let x = (plot.x(*v) - advance / 2.0)
            .min(plot.right() - advance)
            .max(plot.left);
        labels.push((text.to_owned(), x, baseline));
    }
    // The stroke's half-width, kept inside the frame too.
    let margin = 1.0;
    for (v, text) in thinned(left, |v| plot.y(v), LABEL_SIZE) {
        let text = text.as_ref();
        let x = plot.left - LABEL_GAP - face.advance(text, LABEL_SIZE);
        let baseline = (plot.y(*v) + LABEL_SIZE / 2.0)
            .min(plot.bottom() + LABEL_SIZE / 2.0)
            .max(plot.top + LABEL_SIZE + margin);
        labels.push((text.to_owned(), x, baseline));
    }
    labels
}

/// Draws `labels` (from [`labels`]) into `mask` in `face`.
pub(crate) fn draw_labels(mask: &mut Mask, face: &dyn Typeface, labels: &[(String, f32, f32)]) {
    for (text, x, baseline) in labels {
        face.draw(mask, text, *x, *baseline, LABEL_SIZE);
    }
}

/// The `x` of each sample across the plot, twice per physical column at `scale`, from its left
/// edge to its right.
pub(crate) fn columns(plot: &Plot, scale: f32) -> impl Iterator<Item = f32> {
    // A few thousand points at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (plot.width * scale * 2.0).ceil().max(1.0) as u32;
    let (left, width) = (plot.left, plot.width);
    // `step` and `steps` are at most a few thousand.
    #[allow(clippy::cast_precision_loss)]
    (0..=steps).map(move |step| left + width * step as f32 / steps as f32)
}

/// Strokes the polyline through `points` (`(x, y)` in logical pixels) [`CURVE_WIDTH`] wide.
/// Each point is first pulled half the curve's width inside the plot's edges, so the line stays
/// wholly inside the plot. A segment with a NaN end draws nothing.
pub(crate) fn polyline(mask: &mut Mask, plot: &Plot, points: impl IntoIterator<Item = (f32, f32)>) {
    let half = CURVE_WIDTH / 2.0;
    let mut previous: Option<(f32, f32)> = None;
    for (x, y) in points {
        let x = x.clamp(plot.left + half, plot.right() - half);
        let y = y.clamp(plot.top + half, plot.bottom() - half);
        if let Some((x0, y0)) = previous {
            mask.stroke_segment(x0, y0, x, y, CURVE_WIDTH);
        }
        previous = Some((x, y));
    }
}

/// Pixel probes shared by the displays' tests, at scale 1.
#[cfg(test)]
pub(crate) mod probe {
    use crate::effect_display::{DISPLAY_HEIGHT, Pixels};

    /// The pixel at logical `(x, y)`.
    pub(crate) fn pixel(pixels: &Pixels, x: f32, y: f32) -> [u8; 4] {
        // Test coordinates are small and non-negative.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = (y as usize * pixels.width() as usize + x as usize) * 4;
        let p = &pixels.rgba()[i..i + 4];
        [p[0], p[1], p[2], p[3]]
    }

    /// The rows (as pixel centres' tops) in column `x` painted exactly `color`.
    pub(crate) fn rows_of(pixels: &Pixels, x: f32, color: [u8; 4]) -> Vec<f32> {
        // The display's height in rows.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let rows = DISPLAY_HEIGHT as u16;
        (0..rows)
            .map(f32::from)
            .filter(|&y| pixel(pixels, x, y) == color)
            .collect()
    }

    /// Whether a pixel within half a pixel of `(x, y)`, across (`across`) or down, is
    /// red-dominant: a 1-pixel line off the pixel grid is shared between two columns or rows.
    pub(crate) fn red_near(pixels: &Pixels, x: f32, y: f32, across: bool) -> bool {
        [-0.5_f32, 0.0, 0.5].iter().any(|d| {
            let p = if across {
                pixel(pixels, x + d, y)
            } else {
                pixel(pixels, x, y + d)
            };
            p[0] > 120 && p[1] < 80 && p[3] > 0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{GRID, Tone};
    use crate::typeface::StrokeFont;

    /// The plot of a display `width` logical pixels wide, a log axis across and dB up.
    fn log_plot(width: f32) -> Plot {
        Plot::new(width, Axis::log(20.0, 20_000.0), Axis::decibels())
    }

    /// Asserts every label lies inside a display `width` wide.
    fn assert_in_frame(labels: &[(String, f32, f32)], width: f32) {
        for (text, x, baseline) in labels {
            let right = x + StrokeFont.advance(text, LABEL_SIZE);
            assert!(
                *x >= 0.0 && right <= width + 1e-3,
                "{width} {text}: {x}..{right}"
            );
            assert!(
                baseline - LABEL_SIZE >= 0.0 && *baseline <= DISPLAY_HEIGHT,
                "{width} {text}: {baseline}"
            );
        }
    }

    #[test]
    fn test_plot_linear_axes_hit_the_corners() {
        let plot = Plot::new(600.0, Axis::linear(-2.0, 2.0), Axis::decibels());
        assert!((plot.x(-2.0) - LEFT).abs() < 1e-3);
        assert!((plot.x(2.0) - 600.0).abs() < 1e-3);
        assert!(plot.y(TOP_DB).abs() < 1e-3);
        assert!((plot.y(BOTTOM_DB) - (DISPLAY_HEIGHT - BOTTOM)).abs() < 1e-3);
        assert!((plot.x_value(plot.x(0.3)) - 0.3).abs() < 1e-5);
        assert_eq!((plot.top, plot.right()), (0.0, 600.0));
    }

    #[test]
    fn test_plot_log_axis_hits_the_corners_and_round_trips() {
        let plot = log_plot(600.0);
        assert!((plot.x(20.0) - LEFT).abs() < 1e-3);
        assert!((plot.x(20_000.0) - 600.0).abs() < 1e-3);
        // A decade takes a third of the width.
        assert!((plot.x(200.0) - plot.x(20.0) - plot.width / 3.0).abs() < 1e-3);
        assert!((plot.x_value(plot.x(1_000.0)) - 1_000.0).abs() < 0.1);
        let axis = Axis::log(20.0, 20_000.0);
        assert!((axis.value_at(axis.fraction(440.0)) - 440.0).abs() < 1e-2);
    }

    #[test]
    fn test_db_lines_and_labels() {
        assert_eq!(db_lines().count(), 10);
        assert_eq!(db_lines().next(), Some(TOP_DB));
        assert_eq!(db_lines().last(), Some(BOTTOM_DB));
        assert_eq!(
            [db_label(12.0), db_label(0.0), db_label(-96.0)],
            ["+12dB", "0dB", "\u{2212}96dB"]
        );
    }

    #[test]
    fn test_labels_stay_in_the_frame() {
        let below = [(20.0, "20Hz"), (1_000.0, "1kHz"), (20_000.0, "20kHz")];
        for width in [600.0, 260.0] {
            let plot = log_plot(width);
            let labels = labels(&plot, &StrokeFont, &below, &db_marks());
            assert_in_frame(&labels, width);
        }
        // Marks at the vertical axis's ends, beyond them, and at the bottom edge.
        let plot = Plot::new(600.0, Axis::linear(0.0, 1.0), Axis::linear(0.0, 1.0));
        let edge = [(-1.0, "+3dB"), (0.0, "0dB"), (1.0, "0dB"), (2.0, "+3dB")];
        assert_in_frame(&labels(&plot, &StrokeFont, &edge, &edge), 600.0);
    }

    #[test]
    fn test_labels_thin_from_the_low_end_when_crowded() {
        let marks = db_marks();
        let count = |width: f32| {
            let plot = Plot::new(width, Axis::decibels(), Axis::decibels());
            let labels = labels(&plot, &StrokeFont, &marks, &[] as &[(f32, &str)]);
            let texts: Vec<String> = labels.into_iter().map(|(t, _, _)| t).collect();
            texts
        };
        let wide = count(600.0);
        assert_eq!(wide.len(), 10);
        let narrow = count(260.0);
        assert_eq!(narrow.len(), 5);
        assert_eq!(narrow.first().map(String::as_str), Some("\u{2212}96dB"));
        // The vertical axis is never crowded at the display's height.
        let plot = Plot::new(600.0, Axis::decibels(), Axis::decibels());
        let left = labels(&plot, &StrokeFont, &[] as &[(f32, &str)], &marks);
        assert_eq!(left.len(), 10);
    }

    #[test]
    fn test_polyline_stays_inside_every_edge() {
        let plot = Plot::new(200.0, Axis::linear(0.0, 1.0), Axis::linear(0.0, 1.0));
        let mut mask = Mask::new(200, 320, 1.0);
        // Out past each edge and back across.
        let points = [
            (-50.0, -50.0),
            (300.0, 100.0),
            (100.0, 900.0),
            (0.0, f32::INFINITY),
        ];
        polyline(&mut mask, &plot, points);
        let covered: Vec<(usize, usize)> = mask
            .coverage()
            .iter()
            .enumerate()
            .filter(|(_, c)| **c > 0.0)
            .map(|(i, _)| (i % 200, i / 200))
            .collect();
        assert!(!covered.is_empty());
        for (x, y) in covered {
            let (x, y) = (x as f32, y as f32);
            assert!(x >= plot.left && x + 1.0 <= plot.right(), "x {x}");
            assert!(y >= plot.top && y + 1.0 <= plot.bottom(), "y {y}");
        }
    }

    #[test]
    fn test_columns_sample_twice_per_physical_pixel() {
        let plot = log_plot(600.0);
        let xs: Vec<f32> = columns(&plot, 2.0).collect();
        assert_eq!(xs.len(), (plot.width * 4.0) as usize + 1);
        assert_eq!(xs.first(), Some(&plot.left));
        assert!(xs.last().is_some_and(|x| (x - plot.right()).abs() < 1e-3));
    }

    #[test]
    fn test_canvas_finishes_at_the_requested_size() -> Result<(), Box<dyn std::error::Error>> {
        let context = DisplayContext {
            kind: "",
            tone: Tone::Light,
            sample_rate: 48_000,
            width: 900,
            height: 480,
        };
        let mut canvas = Canvas::new(&context).ok_or("no canvas")?;
        assert!((canvas.scale() - 1.5).abs() < 1e-6);
        let plot = canvas.plot(Axis::decibels(), Axis::decibels());
        assert!((plot.right() - 600.0).abs() < 1e-3);
        grid(canvas.raster(), &plot, &[(0.0, GRID)], &[(0.0, GRID)]);
        let pixels = canvas.finish().ok_or("no picture")?;
        assert_eq!((pixels.width(), pixels.height()), (900, 480));
        assert_eq!(pixels.rgba().len(), 900 * 480 * 4);
        Ok(())
    }
}
