//! Text drawn into a [`Mask`]: the [`Typeface`] seam, and [`StrokeFont`], an in-house face of
//! straight strokes covering the labels the equaliser displays need
//! (`docs/architecture/effect-displays.md` "Display plots").

use crate::stroke::Mask;

/// A face that draws a line of text into a mask, in logical pixels.
pub(crate) trait Typeface {
    /// How far `text` at `size` (its capital height, in logical pixels) advances the pen.
    fn advance(&self, text: &str, size: f32) -> f32;

    /// Draws `text` at `size` into `mask`, its first glyph starting at `x` and sitting on
    /// `baseline`.
    fn draw(&self, mask: &mut Mask, text: &str, x: f32, baseline: f32, size: f32);
}

/// A glyph of [`StrokeFont`]: straight strokes on a grid six units tall (the capital height),
/// with `y` up from the baseline.
struct Glyph {
    /// The glyph's width in grid units; the pen advances one unit more.
    width: f32,
    /// Each stroke, as `[x0, y0, x1, y1]`.
    strokes: &'static [[f32; 4]],
}

/// The grid units in the capital height.
const UNITS: f32 = 6.0;

/// The width of a glyph the face does not have, and of a space.
const BLANK: f32 = 3.0;

/// The stroke width, as a fraction of the capital height.
const WEIGHT: f32 = 0.12;

/// An in-house face of straight strokes: digits, `k`, `H`, `z`, `d`, `B`, `+`, `-`, `−` and
/// space. A character it lacks advances as a space and draws nothing.
pub(crate) struct StrokeFont;

impl StrokeFont {
    /// The glyph for `c`, if the face has one.
    fn glyph(c: char) -> Option<Glyph> {
        let glyph = |width, strokes| Some(Glyph { width, strokes });
        match c {
            '0' => glyph(
                4.0,
                &[
                    [1.0, 0.0, 3.0, 0.0],
                    [3.0, 0.0, 4.0, 1.0],
                    [4.0, 1.0, 4.0, 5.0],
                    [4.0, 5.0, 3.0, 6.0],
                    [3.0, 6.0, 1.0, 6.0],
                    [1.0, 6.0, 0.0, 5.0],
                    [0.0, 5.0, 0.0, 1.0],
                    [0.0, 1.0, 1.0, 0.0],
                ],
            ),
            '1' => glyph(
                3.0,
                &[
                    [0.5, 5.0, 1.5, 6.0],
                    [1.5, 6.0, 1.5, 0.0],
                    [0.5, 0.0, 2.5, 0.0],
                ],
            ),
            '2' => glyph(
                4.0,
                &[
                    [0.0, 5.0, 1.0, 6.0],
                    [1.0, 6.0, 3.0, 6.0],
                    [3.0, 6.0, 4.0, 5.0],
                    [4.0, 5.0, 4.0, 4.0],
                    [4.0, 4.0, 0.0, 0.0],
                    [0.0, 0.0, 4.0, 0.0],
                ],
            ),
            '3' => glyph(
                4.0,
                &[
                    [0.0, 6.0, 4.0, 6.0],
                    [4.0, 6.0, 2.0, 4.0],
                    [2.0, 4.0, 3.0, 4.0],
                    [3.0, 4.0, 4.0, 3.0],
                    [4.0, 3.0, 4.0, 1.0],
                    [4.0, 1.0, 3.0, 0.0],
                    [3.0, 0.0, 1.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0],
                ],
            ),
            '4' => glyph(
                4.0,
                &[
                    [3.0, 0.0, 3.0, 6.0],
                    [3.0, 6.0, 0.0, 2.0],
                    [0.0, 2.0, 4.0, 2.0],
                ],
            ),
            '5' => glyph(
                4.0,
                &[
                    [4.0, 6.0, 0.0, 6.0],
                    [0.0, 6.0, 0.0, 3.5],
                    [0.0, 3.5, 3.0, 3.5],
                    [3.0, 3.5, 4.0, 2.5],
                    [4.0, 2.5, 4.0, 1.0],
                    [4.0, 1.0, 3.0, 0.0],
                    [3.0, 0.0, 0.0, 0.0],
                ],
            ),
            '6' => glyph(
                4.0,
                &[
                    [3.0, 6.0, 1.0, 6.0],
                    [1.0, 6.0, 0.0, 5.0],
                    [0.0, 5.0, 0.0, 1.0],
                    [0.0, 1.0, 1.0, 0.0],
                    [1.0, 0.0, 3.0, 0.0],
                    [3.0, 0.0, 4.0, 1.0],
                    [4.0, 1.0, 4.0, 2.5],
                    [4.0, 2.5, 3.0, 3.5],
                    [3.0, 3.5, 0.0, 3.5],
                ],
            ),
            '7' => glyph(4.0, &[[0.0, 6.0, 4.0, 6.0], [4.0, 6.0, 1.0, 0.0]]),
            '8' => glyph(
                4.0,
                &[
                    [1.0, 3.0, 0.0, 4.0],
                    [0.0, 4.0, 0.0, 5.0],
                    [0.0, 5.0, 1.0, 6.0],
                    [1.0, 6.0, 3.0, 6.0],
                    [3.0, 6.0, 4.0, 5.0],
                    [4.0, 5.0, 4.0, 4.0],
                    [4.0, 4.0, 3.0, 3.0],
                    [3.0, 3.0, 1.0, 3.0],
                    [1.0, 3.0, 0.0, 2.0],
                    [0.0, 2.0, 0.0, 1.0],
                    [0.0, 1.0, 1.0, 0.0],
                    [1.0, 0.0, 3.0, 0.0],
                    [3.0, 0.0, 4.0, 1.0],
                    [4.0, 1.0, 4.0, 2.0],
                    [4.0, 2.0, 3.0, 3.0],
                ],
            ),
            '9' => glyph(
                4.0,
                &[
                    [1.0, 0.0, 3.0, 0.0],
                    [3.0, 0.0, 4.0, 1.0],
                    [4.0, 1.0, 4.0, 5.0],
                    [4.0, 5.0, 3.0, 6.0],
                    [3.0, 6.0, 1.0, 6.0],
                    [1.0, 6.0, 0.0, 5.0],
                    [0.0, 5.0, 0.0, 3.5],
                    [0.0, 3.5, 1.0, 2.5],
                    [1.0, 2.5, 4.0, 2.5],
                ],
            ),
            'k' => glyph(
                3.0,
                &[
                    [0.0, 0.0, 0.0, 6.0],
                    [0.0, 1.5, 3.0, 4.0],
                    [1.0, 2.3, 3.0, 0.0],
                ],
            ),
            'H' => glyph(
                4.0,
                &[
                    [0.0, 0.0, 0.0, 6.0],
                    [4.0, 0.0, 4.0, 6.0],
                    [0.0, 3.0, 4.0, 3.0],
                ],
            ),
            'z' => glyph(
                3.0,
                &[
                    [0.0, 4.0, 3.0, 4.0],
                    [3.0, 4.0, 0.0, 0.0],
                    [0.0, 0.0, 3.0, 0.0],
                ],
            ),
            'd' => glyph(
                4.0,
                &[
                    [4.0, 0.0, 4.0, 6.0],
                    [4.0, 3.0, 3.0, 4.0],
                    [3.0, 4.0, 1.0, 4.0],
                    [1.0, 4.0, 0.0, 3.0],
                    [0.0, 3.0, 0.0, 1.0],
                    [0.0, 1.0, 1.0, 0.0],
                    [1.0, 0.0, 3.0, 0.0],
                    [3.0, 0.0, 4.0, 1.0],
                ],
            ),
            'B' => glyph(
                4.0,
                &[
                    [0.0, 0.0, 0.0, 6.0],
                    [0.0, 6.0, 3.0, 6.0],
                    [3.0, 6.0, 4.0, 5.0],
                    [4.0, 5.0, 4.0, 4.0],
                    [4.0, 4.0, 3.0, 3.0],
                    [0.0, 3.0, 3.0, 3.0],
                    [3.0, 3.0, 4.0, 2.0],
                    [4.0, 2.0, 4.0, 1.0],
                    [4.0, 1.0, 3.0, 0.0],
                    [3.0, 0.0, 0.0, 0.0],
                ],
            ),
            '+' => glyph(4.0, &[[0.0, 3.0, 4.0, 3.0], [2.0, 1.0, 2.0, 5.0]]),
            '-' | '\u{2212}' => glyph(4.0, &[[0.0, 3.0, 4.0, 3.0]]),
            _ => None,
        }
    }

    /// The width of `c` plus the gap after it, in grid units.
    fn units(c: char) -> f32 {
        Self::glyph(c).map_or(BLANK, |g| g.width) + 1.0
    }
}

impl Typeface for StrokeFont {
    fn advance(&self, text: &str, size: f32) -> f32 {
        // The last glyph's gap is not part of the text's width.
        let units: f32 = text.chars().map(Self::units).sum();
        (units - 1.0).max(0.0) * size / UNITS
    }

    fn draw(&self, mask: &mut Mask, text: &str, x: f32, baseline: f32, size: f32) {
        let unit = size / UNITS;
        let mut pen = x;
        for c in text.chars() {
            if let Some(glyph) = Self::glyph(c) {
                for &[x0, y0, x1, y1] in glyph.strokes {
                    mask.stroke_segment(
                        pen + x0 * unit,
                        baseline - y0 * unit,
                        pen + x1 * unit,
                        baseline - y1 * unit,
                        size * WEIGHT,
                    );
                }
            }
            pen += Self::units(c) * unit;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_advance_grows_with_the_text() {
        let font = StrokeFont;
        assert_eq!(font.advance("", 12.0), 0.0);
        let one = font.advance("1", 12.0);
        let two = font.advance("1k", 12.0);
        assert!(one > 0.0 && two > one);
        assert!(font.advance("20 kHz", 12.0) > font.advance("20kHz", 12.0));
        assert!((font.advance("88", 24.0) - 2.0 * font.advance("88", 12.0)).abs() < 1e-4);
        // An unknown glyph advances like a space.
        assert_eq!(font.advance("a", 12.0), font.advance(" ", 12.0));
    }

    #[test]
    fn test_draw_marks_only_inside_the_text_box() {
        let font = StrokeFont;
        let mut mask = Mask::new(40, 30, 1.0);
        let (x, baseline, size) = (5.0, 20.0, 12.0);
        font.draw(&mut mask, "1k", x, baseline, size);
        let right = x + font.advance("1k", size);
        let margin = size * WEIGHT / 2.0 + 1.0;
        let mut inside = 0;
        for (i, &cover) in mask.coverage().iter().enumerate() {
            let (px, py) = ((i % 40) as f32 + 0.5, (i / 40) as f32 + 0.5);
            let in_box = px > x - margin
                && px < right + margin
                && py > baseline - size - margin
                && py < baseline + margin;
            if cover > 0.0 {
                assert!(in_box, "({px}, {py})");
                inside += 1;
            }
        }
        assert!(inside > 10);
    }
}
