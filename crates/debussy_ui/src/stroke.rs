//! Anti-aliased strokes: line segments drawn as coverage into a [`Mask`], which a
//! [`Raster`](crate::raster::Raster) then paints in one colour
//! (`docs/architecture/effect-displays.md` "Display plots").

/// How much of each physical pixel is covered, from 0 to 1, drawn in logical coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mask {
    /// Width in physical pixels.
    width: usize,
    /// Height in physical pixels.
    height: usize,
    /// Physical pixels per logical pixel.
    scale: f32,
    /// Row-major coverage.
    coverage: Vec<f32>,
}

impl Mask {
    /// An empty mask of `width` by `height` physical pixels, drawn at `scale` physical pixels
    /// per logical pixel (1 if `scale` is not a positive number).
    pub(crate) fn new(width: usize, height: usize, scale: f32) -> Mask {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        Mask {
            width,
            height,
            scale,
            coverage: vec![0.0; width * height],
        }
    }

    /// Width in physical pixels.
    pub(crate) fn width(&self) -> usize {
        self.width
    }

    /// Height in physical pixels.
    pub(crate) fn height(&self) -> usize {
        self.height
    }

    /// The coverage of each pixel, row by row.
    pub(crate) fn coverage(&self) -> &[f32] {
        &self.coverage
    }

    /// Covers the segment from (`x0`, `y0`) to (`x1`, `y1`), `width` logical pixels wide with
    /// round ends. A pixel's coverage is how far its centre lies inside the stroke's edge, eased
    /// over one physical pixel; it keeps the larger of that and what it had, so the joints of a
    /// polyline are not covered twice. A segment with a NaN end draws nothing.
    pub(crate) fn stroke_segment(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32) {
        let s = self.scale;
        let (x0, y0, x1, y1) = (x0 * s, y0 * s, x1 * s, y1 * s);
        let half = (width * s / 2.0).max(0.0);
        let reach = half + 1.0;
        let columns = range(x0.min(x1) - reach, x0.max(x1) + reach, self.width);
        let rows = range(y0.min(y1) - reach, y0.max(y1) + reach, self.height);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let length_squared = dx * dx + dy * dy;
        for row in rows {
            let py = row as f32 + 0.5;
            for column in columns.clone() {
                let px = column as f32 + 0.5;
                // The nearest point of the segment to the pixel's centre.
                let t = if length_squared > 0.0 {
                    (((px - x0) * dx + (py - y0) * dy) / length_squared).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let distance = (px - (x0 + t * dx)).hypot(py - (y0 + t * dy));
                let cover = (half + 0.5 - distance).clamp(0.0, 1.0);
                let cell = &mut self.coverage[row * self.width + column];
                *cell = cell.max(cover);
            }
        }
    }
}

/// The whole pixels from `a` to `b` touches, within `0..len`; empty if none, or for a NaN edge.
fn range(a: f32, b: f32, len: usize) -> std::ops::Range<usize> {
    if a.partial_cmp(&b)
        .is_none_or(|o| o == std::cmp::Ordering::Greater)
        || b < 0.0
        || a >= len as f32
    {
        return 0..0;
    }
    a.max(0.0).floor() as usize..(b.ceil() as usize).min(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(mask: &Mask, x: usize, y: usize) -> f32 {
        mask.coverage[y * mask.width + x]
    }

    #[test]
    fn test_stroke_segment_covers_a_horizontal_line() {
        let mut mask = Mask::new(10, 5, 1.0);
        mask.stroke_segment(2.0, 2.5, 8.0, 2.5, 1.0);
        assert_eq!(at(&mask, 5, 2), 1.0);
        assert_eq!(at(&mask, 5, 0), 0.0);
        assert_eq!(at(&mask, 5, 4), 0.0);
        assert_eq!(at(&mask, 0, 2), 0.0);
    }

    #[test]
    fn test_stroke_segment_joints_do_not_add_up() {
        let mut mask = Mask::new(10, 10, 1.0);
        mask.stroke_segment(1.0, 5.0, 5.0, 5.0, 2.0);
        mask.stroke_segment(5.0, 5.0, 9.0, 1.0, 2.0);
        assert!(mask.coverage.iter().all(|&c| (0.0..=1.0).contains(&c)));
        assert!(at(&mask, 4, 4) > 0.5);
    }

    #[test]
    fn test_stroke_segment_scales_and_clips() {
        let mut mask = Mask::new(8, 8, 2.0);
        mask.stroke_segment(-10.0, 1.0, 10.0, 1.0, 1.0);
        assert_eq!(at(&mask, 0, 1), 1.0);
        assert_eq!(at(&mask, 7, 2), 1.0);
        assert_eq!(at(&mask, 7, 5), 0.0);
        mask.stroke_segment(f32::NAN, 0.0, 1.0, 1.0, 1.0);
        assert_eq!((mask.width(), mask.height()), (8, 8));
    }
}
