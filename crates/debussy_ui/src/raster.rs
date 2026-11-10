//! A lane drawn into memory: rectangles painted at the display's scale, blurred with a Gaussian,
//! and handed to the canvas as an image (`docs/architecture/user-interface.md` "Layout",
//! ADR-0068).

use iced::Color;

use crate::stroke::Mask;

/// Physical pixels in premultiplied RGBA, painted in logical coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    /// Width in physical pixels.
    width: usize,
    /// Height in physical pixels.
    height: usize,
    /// Physical pixels per logical pixel.
    scale: f32,
    /// Row-major, premultiplied RGBA, each channel 0 to 1.
    pixels: Vec<[f32; 4]>,
}

impl Raster {
    /// A transparent raster covering `width` by `height` logical pixels at `scale` physical
    /// pixels each, rounded to whole pixels and at least one each way.
    pub fn new(width: f32, height: f32, scale: f32) -> Raster {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let to_px = |v: f32| ((v * scale).round().max(1.0)) as usize;
        let (width, height) = (to_px(width), to_px(height));
        Raster {
            width,
            height,
            scale,
            pixels: vec![[0.0; 4]; width * height],
        }
    }

    /// A transparent raster of exactly `width` by `height` physical pixels, painted at `scale`
    /// physical pixels per logical pixel (1 if `scale` is not a positive number).
    pub fn with_physical_size(width: usize, height: usize, scale: f32) -> Raster {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        Raster {
            width,
            height,
            scale,
            pixels: vec![[0.0; 4]; width * height],
        }
    }

    /// Width in physical pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Height in physical pixels.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Physical pixels per logical pixel.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Paints `color` over the rectangle at (`x`, `y`) of `w` by `h` logical pixels. A pixel the
    /// rectangle only partly covers is painted at that fraction of the colour's alpha.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        let (x0, x1) = (x * self.scale, (x + w) * self.scale);
        let (y0, y1) = (y * self.scale, (y + h) * self.scale);
        let Some(columns) = span(x0, x1, self.width) else {
            return;
        };
        let Some(rows) = span(y0, y1, self.height) else {
            return;
        };
        for row in rows {
            let cover_y = overlap(row, y0, y1);
            let line = &mut self.pixels[row * self.width..(row + 1) * self.width];
            for column in columns.clone() {
                let alpha = color.a * cover_y * overlap(column, x0, x1);
                if alpha <= 0.0 {
                    continue;
                }
                let px = &mut line[column];
                let keep = 1.0 - alpha;
                px[0] = color.r * alpha + px[0] * keep;
                px[1] = color.g * alpha + px[1] * keep;
                px[2] = color.b * alpha + px[2] * keep;
                px[3] = alpha + px[3] * keep;
            }
        }
    }

    /// Paints `color` over each pixel at the fraction of its alpha that `mask` covers. Nothing
    /// changes unless the mask is this raster's size.
    pub fn paint(&mut self, mask: &Mask, color: Color) {
        if (mask.width(), mask.height()) != (self.width, self.height) {
            return;
        }
        for (px, &cover) in self.pixels.iter_mut().zip(mask.coverage()) {
            let alpha = color.a * cover;
            if alpha <= 0.0 {
                continue;
            }
            let keep = 1.0 - alpha;
            px[0] = color.r * alpha + px[0] * keep;
            px[1] = color.g * alpha + px[1] * keep;
            px[2] = color.b * alpha + px[2] * keep;
            px[3] = alpha + px[3] * keep;
        }
    }

    /// Blurs with a Gaussian of standard deviation `sigma` physical pixels, out to `ceil(3σ)`
    /// pixels each side, repeating the edge pixels beyond the raster. Nothing changes if
    /// `sigma` is not positive.
    pub fn blur(&mut self, sigma: f32) {
        let kernel = kernel(sigma);
        if kernel.len() <= 1 {
            return;
        }
        let radius = (kernel.len() / 2) as isize;
        let (w, h) = (self.width as isize, self.height as isize);
        let mut scratch = vec![[0.0f32; 4]; self.pixels.len()];
        // Across each row, into the scratch buffer.
        for row in 0..h {
            let line = &self.pixels[(row * w) as usize..((row + 1) * w) as usize];
            let out = &mut scratch[(row * w) as usize..((row + 1) * w) as usize];
            for (x, px) in out.iter_mut().enumerate() {
                *px = convolve(&kernel, |k| {
                    line[(x as isize + k - radius).clamp(0, w - 1) as usize]
                });
            }
        }
        // Down each column, back.
        for x in 0..w {
            for y in 0..h {
                self.pixels[(y * w + x) as usize] = convolve(&kernel, |k| {
                    scratch[((y + k - radius).clamp(0, h - 1) * w + x) as usize]
                });
            }
        }
    }

    /// The pixels as straight (not premultiplied) 8-bit RGBA, row by row.
    pub fn into_rgba8(self) -> Vec<u8> {
        let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for [r, g, b, a] in self.pixels {
            let (r, g, b) = if a > 0.0 {
                (r / a, g / a, b / a)
            } else {
                (0.0, 0.0, 0.0)
            };
            bytes.extend([to_byte(r), to_byte(g), to_byte(b), to_byte(a)]);
        }
        bytes
    }
}

/// The whole pixels from `a` to `b` touches, within `0..len`; `None` if none.
fn span(a: f32, b: f32, len: usize) -> Option<std::ops::Range<usize>> {
    // Also refuses a NaN edge, which compares false either way.
    if a.partial_cmp(&b) != Some(std::cmp::Ordering::Less) || b <= 0.0 || a >= len as f32 {
        return None;
    }
    Some(a.max(0.0).floor() as usize..(b.ceil() as usize).min(len))
}

/// How much of pixel `i` (from `i` to `i + 1`) lies between `a` and `b`.
fn overlap(i: usize, a: f32, b: f32) -> f32 {
    let i = i as f32;
    (b.min(i + 1.0) - a.max(i)).clamp(0.0, 1.0)
}

/// The normalised Gaussian weights from `-ceil(3σ)` to `ceil(3σ)`; a single 1 if `sigma` is not
/// positive.
fn kernel(sigma: f32) -> Vec<f32> {
    if !sigma.is_finite() || sigma <= 0.0 {
        return vec![1.0];
    }
    let radius = (3.0 * sigma).ceil() as isize;
    let mut weights: Vec<f32> = (-radius..=radius)
        .map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = weights.iter().sum();
    weights.iter_mut().for_each(|w| *w /= sum);
    weights
}

/// The weighted sum of `sample(k)` for each tap `k` of `kernel`.
fn convolve(kernel: &[f32], sample: impl Fn(isize) -> [f32; 4]) -> [f32; 4] {
    let mut sum = [0.0; 4];
    for (k, weight) in kernel.iter().enumerate() {
        let px = sample(k as isize);
        for (s, p) in sum.iter_mut().zip(px) {
            *s += weight * p;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-5;

    fn at(raster: &Raster, x: usize, y: usize) -> [f32; 4] {
        raster.pixels[y * raster.width + x]
    }

    #[test]
    fn test_blur_uniform_image_unchanged() {
        let mut raster = Raster::new(9.0, 5.0, 1.0);
        raster.fill_rect(0.0, 0.0, 9.0, 5.0, Color::from_rgb(0.2, 0.4, 0.6));
        let before = raster.clone();
        raster.blur(1.0);
        for (a, b) in raster.pixels.iter().zip(&before.pixels) {
            for (x, y) in a.iter().zip(b) {
                assert!((x - y).abs() < EPSILON, "{a:?} != {b:?}");
            }
        }
    }

    #[test]
    fn test_blur_spreads_single_pixel_symmetrically() {
        let mut raster = Raster::new(15.0, 15.0, 1.0);
        raster.fill_rect(7.0, 7.0, 1.0, 1.0, Color::WHITE);
        raster.blur(1.0);
        let weights = kernel(1.0);
        assert_eq!(weights.len(), 7);
        let total: f32 = raster.pixels.iter().map(|p| p[3]).sum();
        assert!((total - 1.0).abs() < EPSILON);
        for dy in 0..7 {
            for dx in 0..7 {
                let expected = weights[dx] * weights[dy];
                let got = at(&raster, 4 + dx, 4 + dy)[3];
                assert!((got - expected).abs() < EPSILON, "({dx}, {dy})");
            }
        }
        assert!((at(&raster, 6, 7)[3] - at(&raster, 8, 7)[3]).abs() < EPSILON);
        assert!((at(&raster, 7, 6)[3] - at(&raster, 7, 8)[3]).abs() < EPSILON);
    }

    #[test]
    fn test_blur_non_positive_sigma_is_a_no_op() {
        let mut raster = Raster::new(3.0, 3.0, 1.0);
        raster.fill_rect(1.0, 1.0, 1.0, 1.0, Color::WHITE);
        let before = raster.clone();
        raster.blur(0.0);
        assert_eq!(raster, before);
    }

    #[test]
    fn test_fill_rect_partial_coverage() {
        let mut raster = Raster::new(4.0, 1.0, 1.0);
        raster.fill_rect(1.0, 0.0, 0.5, 1.0, Color::WHITE);
        assert_eq!(at(&raster, 0, 0)[3], 0.0);
        assert!((at(&raster, 1, 0)[3] - 0.5).abs() < EPSILON);
        assert_eq!(at(&raster, 2, 0)[3], 0.0);
        let bytes = raster.into_rgba8();
        assert_eq!(&bytes[4..8], &[255, 255, 255, 128]);
    }

    #[test]
    fn test_fill_rect_scales_logical_to_physical() {
        let mut raster = Raster::new(4.0, 2.0, 2.0);
        assert_eq!((raster.width(), raster.height()), (8, 4));
        raster.fill_rect(1.0, 0.0, 1.0, 1.0, Color::WHITE);
        for y in 0..4 {
            for x in 0..8 {
                let lit = (2..4).contains(&x) && y < 2;
                assert_eq!(
                    at(&raster, x, y)[3],
                    if lit { 1.0 } else { 0.0 },
                    "({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn test_fill_rect_outside_is_ignored() {
        let mut raster = Raster::new(2.0, 2.0, 1.0);
        raster.fill_rect(-5.0, 0.0, 2.0, 2.0, Color::WHITE);
        raster.fill_rect(3.0, 0.0, 2.0, 2.0, Color::WHITE);
        raster.fill_rect(0.0, 0.0, 0.0, 2.0, Color::WHITE);
        assert!(raster.pixels.iter().all(|p| p[3] == 0.0));
    }

    #[test]
    fn test_fill_rect_blends_over() {
        let mut raster = Raster::new(1.0, 1.0, 1.0);
        raster.fill_rect(0.0, 0.0, 1.0, 1.0, Color::BLACK);
        raster.fill_rect(0.0, 0.0, 1.0, 1.0, Color::from_rgba(1.0, 1.0, 1.0, 0.5));
        assert_eq!(raster.into_rgba8(), vec![128, 128, 128, 255]);
    }

    #[test]
    fn test_paint_composites_the_mask() {
        let mut raster = Raster::with_physical_size(2, 1, 1.0);
        let mut mask = Mask::new(2, 1, 1.0);
        mask.stroke_segment(0.0, 0.5, 0.5, 0.5, 1.0);
        raster.paint(&mask, Color::WHITE);
        assert_eq!(raster.clone().into_rgba8()[..4], [255, 255, 255, 255]);
        assert_eq!(at(&raster, 1, 0)[3], 0.0);
        raster.paint(&Mask::new(3, 1, 1.0), Color::BLACK);
        assert_eq!(at(&raster, 0, 0), [1.0; 4]);
    }
}
