//! The equalisers' display: the magnitude response their parameters ask for, on fixed axes of
//! 20 Hz to 20 kHz and +12 to −96 dB (`docs/architecture/user-interface.md` "Equaliser
//! displays").

use audio_effects::{FrequencyResponse, frequency_response};
use iced::Color;

use crate::effect_display::{
    DISPLAY_HEIGHT, DisplayContext, DisplayParameter, EffectDisplay, Pixels,
};
use crate::raster::Raster;
use crate::stroke::Mask;
use crate::theme::{self, GRID, GRID_ZERO, LABEL};
use crate::typeface::{StrokeFont, Typeface};

/// The lowest and highest frequency shown, in Hz.
const FREQUENCIES: (f32, f32) = (20.0, 20_000.0);

/// The highest and lowest gain shown, in dB.
const TOP_DB: f32 = 12.0;
const BOTTOM_DB: f32 = -96.0;

/// The spacing of the gain grid, in dB.
const DB_STEP: f32 = 12.0;

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

/// The plot's insets from the display's edges, in logical pixels: a gutter on the left for the
/// gain labels and one below for the frequency labels. The grid runs to the top and right edges.
const LEFT: f32 = 46.0;
const RIGHT: f32 = 0.0;
const TOP: f32 = 0.0;
const BOTTOM: f32 = 20.0;

/// The labels' capital height, in logical pixels.
const LABEL_SIZE: f32 = 9.0;

/// The gap between a label and the plot or its line, in logical pixels.
const LABEL_GAP: f32 = 5.0;

/// The curve's width, in logical pixels.
const CURVE_WIDTH: f32 = 2.0;

/// The grid lines' width, in logical pixels.
const GRID_WIDTH: f32 = 1.0;

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
        let (width, height) = (
            usize::try_from(context.width).ok()?,
            usize::try_from(context.height).ok()?,
        );
        let raster = render(&response, context, width, height, &StrokeFont);
        Pixels::new(context.width, context.height, raster.into_rgba8())
    }
}

/// Where the plot lies, in logical pixels, and its axes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Plot {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl Plot {
    /// The plot in a display `width` logical pixels wide and [`DISPLAY_HEIGHT`] tall.
    fn new(width: f32) -> Plot {
        Plot {
            left: LEFT,
            top: TOP,
            width: (width - LEFT - RIGHT).max(1.0),
            height: DISPLAY_HEIGHT - TOP - BOTTOM,
        }
    }

    /// The decades from the lowest frequency shown to the highest.
    fn decades() -> f32 {
        (FREQUENCIES.1 / FREQUENCIES.0).log10()
    }

    /// The `x` of `frequency` (Hz), on a log scale.
    fn x(&self, frequency: f32) -> f32 {
        self.left + (frequency / FREQUENCIES.0).log10() / Self::decades() * self.width
    }

    /// The frequency (Hz) at `x`.
    fn frequency(&self, x: f32) -> f32 {
        FREQUENCIES.0 * 10.0_f32.powf((x - self.left) / self.width * Self::decades())
    }

    /// The `y` of `db`.
    fn y(&self, db: f32) -> f32 {
        self.top + (TOP_DB - db) / (TOP_DB - BOTTOM_DB) * self.height
    }

    /// The right edge.
    fn right(&self) -> f32 {
        self.left + self.width
    }

    /// The bottom edge.
    fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

/// `response` drawn on its grid for `context`, `width` × `height` physical pixels, labelled in
/// `face`. The background is left transparent.
fn render(
    response: &FrequencyResponse,
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
    draw_labels(&mut labels, &plot, face);
    raster.paint(&labels, LABEL);
    let mut curve = Mask::new(width, height, scale);
    draw_curve(&mut curve, &plot, response, scale);
    raster.paint(&curve, theme::tokens(context.tone).audio);
    raster
}

/// The vertical and horizontal grid lines.
fn draw_grid(raster: &mut Raster, plot: &Plot) {
    // A line on the plot's edge is moved inward to show its whole width.
    let half = GRID_WIDTH / 2.0;
    for (frequency, _) in FREQUENCY_LINES {
        let x = (plot.x(frequency) - half).clamp(plot.left, plot.right() - GRID_WIDTH);
        raster.fill_rect(x, plot.top, GRID_WIDTH, plot.height, GRID);
    }
    for db in db_lines() {
        let color: Color = if db == 0.0 { GRID_ZERO } else { GRID };
        let y = (plot.y(db) - half).clamp(plot.top, plot.bottom() - GRID_WIDTH);
        raster.fill_rect(plot.left, y, plot.width, GRID_WIDTH, color);
    }
}

/// The gains of the horizontal grid lines, from the top down.
fn db_lines() -> impl Iterator<Item = f32> {
    // A handful of lines.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = ((TOP_DB - BOTTOM_DB) / DB_STEP).round() as u16;
    (0..=count).map(|i| TOP_DB - f32::from(i) * DB_STEP)
}

/// The label of a gain line, such as `"+12dB"`, `"0dB"` or `"−24dB"`.
pub(crate) fn db_label(db: f32) -> String {
    if db > 0.0 {
        format!("+{db:.0}dB")
    } else if db < 0.0 {
        format!("\u{2212}{:.0}dB", -db)
    } else {
        "0dB".to_owned()
    }
}

/// Each label, with its unit, and where it goes: `(text, x, baseline)` in logical pixels. The
/// frequency labels are centred under their lines and the gain labels right-aligned in the
/// gutter, centred on theirs; a label that would cross the frame's edge is moved inward.
fn labels(plot: &Plot, face: &dyn Typeface) -> Vec<(String, f32, f32)> {
    let mut labels = Vec::new();
    let baseline = plot.bottom() + LABEL_GAP + LABEL_SIZE;
    for (frequency, label) in FREQUENCY_LINES {
        let advance = face.advance(label, LABEL_SIZE);
        let x = (plot.x(frequency) - advance / 2.0)
            .min(plot.right() - advance)
            .max(plot.left);
        labels.push((label.to_owned(), x, baseline));
    }
    // The stroke's half-width, kept inside the frame too.
    let margin = 1.0;
    for db in db_lines() {
        let label = db_label(db);
        let x = plot.left - LABEL_GAP - face.advance(&label, LABEL_SIZE);
        let baseline = (plot.y(db) + LABEL_SIZE / 2.0).max(plot.top + LABEL_SIZE + margin);
        labels.push((label, x, baseline));
    }
    labels
}

/// Draws [`labels`] into `mask`.
fn draw_labels(mask: &mut Mask, plot: &Plot, face: &dyn Typeface) {
    for (text, x, baseline) in labels(plot, face) {
        face.draw(mask, &text, x, baseline, LABEL_SIZE);
    }
}

/// The response as a polyline, sampled twice per physical column across the plot and at the
/// centre of each notch. Gains beyond the axes are pinned to the plot's edge; the line stops at
/// the Nyquist frequency.
fn draw_curve(mask: &mut Mask, plot: &Plot, response: &FrequencyResponse, scale: f32) {
    // A few thousand points at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (plot.width * scale * 2.0).ceil().max(1.0) as u32;
    #[allow(clippy::cast_precision_loss)]
    let mut xs: Vec<f32> = (0..=steps)
        .map(|step| plot.left + plot.width * step as f32 / steps as f32)
        .collect();
    xs.extend(
        response
            .zeros()
            .map(|frequency| plot.x(frequency))
            .filter(|x| (plot.left..=plot.right()).contains(x)),
    );
    xs.sort_by(f32::total_cmp);
    let mut previous: Option<(f32, f32)> = None;
    for x in xs {
        let Some(db) = response.magnitude_db(plot.frequency(x)) else {
            break;
        };
        // A notch's centre is negative infinity; NaN is treated the same.
        let db = if db.is_nan() {
            BOTTOM_DB
        } else {
            db.clamp(BOTTOM_DB, TOP_DB)
        };
        // Kept a half-width inside the top edge, so a clipped boost shows the whole line.
        let y = plot.y(db).max(plot.top + CURVE_WIDTH / 2.0);
        let point = (x, y);
        if let Some((x0, y0)) = previous {
            mask.stroke_segment(x0, y0, point.0, point.1, CURVE_WIDTH);
        }
        previous = Some(point);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Tone;

    const RATE: u32 = 48_000;

    /// The plot of a display 600 logical pixels wide at scale 1.
    fn plot() -> Plot {
        Plot::new(600.0)
    }

    /// A context for `kind` at scale 1, 600 pixels wide.
    fn context(kind: &str, tone: Tone) -> DisplayContext<'_> {
        DisplayContext {
            kind,
            tone,
            sample_rate: RATE,
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

    /// The display of the parametric equaliser at `values`.
    fn draw(values: &[f32], tone: Tone) -> Option<Pixels> {
        let parameters: Vec<DisplayParameter<'_>> = values
            .iter()
            .map(|&value| DisplayParameter {
                name: "",
                value,
                unit: "",
            })
            .collect();
        ResponseDisplay.draw(&parameters, &context("parametric-eq", tone))
    }

    /// The rows (and colour) of every pixel in the plot's columns tinted like the curve: grid
    /// and labels are greys, so a pixel whose red and blue differ is the curve's.
    fn accent_pixels(pixels: &Pixels) -> Vec<(usize, [u8; 4])> {
        let width = pixels.width() as usize;
        pixels
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                let x = (i % width) as f32;
                x > LEFT + 4.0 && x < 600.0 - RIGHT - 4.0 && p[3] > 0 && p[0] != p[2]
            })
            .map(|(i, p)| (i / width, [p[0], p[1], p[2], p[3]]))
            .collect()
    }

    #[test]
    fn test_plot_axes_hit_the_corners() {
        let plot = plot();
        assert!((plot.x(20.0) - LEFT).abs() < 1e-3);
        assert!((plot.x(20_000.0) - (600.0 - RIGHT)).abs() < 1e-3);
        assert!((plot.y(12.0) - TOP).abs() < 1e-3);
        assert!((plot.y(-96.0) - (DISPLAY_HEIGHT - BOTTOM)).abs() < 1e-3);
        assert!((plot.frequency(plot.x(1_000.0)) - 1_000.0).abs() < 0.1);
        assert_eq!(db_lines().count(), 10);
        assert_eq!(
            [db_label(12.0), db_label(0.0), db_label(-96.0)],
            ["+12dB", "0dB", "\u{2212}96dB"]
        );
    }

    #[test]
    fn test_draw_flat_eq_lies_on_the_0_db_row() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&bell(0.0), Tone::Dark).ok_or("no picture")?;
        let zero = plot().y(0.0);
        let accents = accent_pixels(&pixels);
        assert!(accents.len() > 500);
        for (row, _) in accents {
            assert!((row as f32 + 0.5 - zero).abs() < 2.5, "row {row}");
        }
        Ok(())
    }

    #[test]
    fn test_draw_clips_a_boost_to_the_top() -> Result<(), Box<dyn std::error::Error>> {
        let pixels = draw(&bell(24.0), Tone::Dark).ok_or("no picture")?;
        let accents = accent_pixels(&pixels);
        let top = accents
            .iter()
            .map(|(row, _)| *row)
            .min()
            .ok_or("no curve")?;
        assert_eq!(top, 0, "the clipped curve reaches the top edge");
        Ok(())
    }

    #[test]
    fn test_draw_uses_each_tones_accent() -> Result<(), Box<dyn std::error::Error>> {
        for tone in [Tone::Dark, Tone::Light] {
            let pixels = draw(&bell(0.0), tone).ok_or("no picture")?;
            let solid = accent_pixels(&pixels)
                .into_iter()
                .find(|(_, p)| p[3] == 255)
                .ok_or("no solid pixel")?;
            let accent = theme::tokens(tone).audio.into_rgba8();
            assert_eq!(solid.1, accent, "{tone:?}");
        }
        Ok(())
    }

    #[test]
    fn test_draw_returns_the_requested_size() -> Result<(), Box<dyn std::error::Error>> {
        let parameters = [
            DisplayParameter {
                name: "cutoff",
                value: 1_000.0,
                unit: "Hz",
            },
            DisplayParameter {
                name: "order",
                value: 4.0,
                unit: "",
            },
        ];
        let mut ctx = context("low-pass", Tone::Light);
        (ctx.width, ctx.height) = (900, 480);
        let pixels = ResponseDisplay
            .draw(&parameters, &ctx)
            .ok_or("no picture")?;
        assert_eq!((pixels.width(), pixels.height()), (900, 480));
        assert_eq!(pixels.rgba().len(), 900 * 480 * 4);
        ctx.kind = "mixer";
        assert!(ResponseDisplay.draw(&parameters, &ctx).is_none());
        Ok(())
    }

    #[test]
    fn test_draw_notch_reaches_the_bottom() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = bell(0.0);
        // Band 1 follows the high-pass stage's three values.
        values[3..8].copy_from_slice(&[1.0, 3.0, 3_000.0, 0.0, 6.0]);
        let pixels = draw(&values, Tone::Dark).ok_or("no picture")?;
        let lowest = accent_pixels(&pixels)
            .iter()
            .map(|(row, _)| *row)
            .max()
            .ok_or("no curve")?;
        assert!(
            lowest as f32 >= DISPLAY_HEIGHT - BOTTOM - 1.0,
            "lowest {lowest}"
        );
        Ok(())
    }

    #[test]
    fn test_labels_carry_units_and_stay_in_the_frame() {
        let font = StrokeFont;
        let plot = plot();
        let labels = labels(&plot, &font);
        let texts: Vec<&str> = labels.iter().map(|(t, _, _)| t.as_str()).collect();
        for text in ["20Hz", "1kHz", "20kHz", "+12dB", "0dB", "\u{2212}96dB"] {
            assert!(texts.contains(&text), "{text}");
        }
        for (text, x, baseline) in &labels {
            let right = x + font.advance(text, LABEL_SIZE);
            assert!(*x >= 0.0 && right <= 600.0, "{text}: {x}..{right}");
            assert!(
                baseline - LABEL_SIZE >= 0.0 && *baseline <= DISPLAY_HEIGHT,
                "{text}"
            );
        }
        // The grid reaches the top and right edges.
        assert_eq!((plot.top, plot.right()), (0.0, 600.0));
    }
}
