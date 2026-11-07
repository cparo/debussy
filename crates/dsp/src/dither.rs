//! TPDF dither and quantisation to an integer bit depth, applied before integer export
//! (`docs/architecture/import-export.md` "Export bit depth", ADR-0025).

use crate::noise::Noise;

/// An integer bit depth to quantise to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// 16-bit.
    Int16,
    /// 24-bit.
    Int24,
}

impl Depth {
    /// Steps per unit of full scale: 2^15 or 2^23.
    pub const fn full_scale(self) -> f32 {
        match self {
            Depth::Int16 => 32_768.0,
            Depth::Int24 => 8_388_608.0,
        }
    }
}

/// Adds triangular (TPDF) dither of ±1 LSB and rounds to a bit depth, in place.
///
/// Outputs are exact multiples of one LSB within `-1.0..=1.0 - LSB`, so an integer encoder that
/// rounds `sample × full scale` (as `wav_files` does) stores them unchanged.
#[derive(Debug, Clone)]
pub struct Ditherer {
    noise: Noise,
    depth: Depth,
}

impl Ditherer {
    /// A ditherer for `depth` whose noise comes from `seed`. Each channel should have its own
    /// ditherer, with a different seed, so their noise is uncorrelated.
    pub fn new(depth: Depth, seed: u64) -> Self {
        Self {
            noise: Noise::new(seed),
            depth,
        }
    }

    /// The bit depth.
    pub fn depth(&self) -> Depth {
        self.depth
    }

    /// Dithers and quantises `samples` in place. Values beyond full scale are clipped; NaN
    /// becomes silence.
    pub fn process(&mut self, samples: &mut [f32]) {
        let scale = self.depth.full_scale();
        for sample in samples {
            let dithered = *sample * scale + self.noise.triangular();
            let step = if dithered.is_nan() {
                0.0
            } else {
                dithered.round().clamp(-scale, scale - 1.0)
            };
            // `step` is an integer of at most 24 bits and `scale` a power of two, so this is
            // exact.
            *sample = step / scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lsb(depth: Depth) -> f32 {
        1.0 / depth.full_scale()
    }

    #[test]
    fn test_process_silence_stays_within_one_lsb() {
        for depth in [Depth::Int16, Depth::Int24] {
            let mut d = Ditherer::new(depth, 3);
            let mut block = [0.0_f32; 4096];
            d.process(&mut block);
            for &s in &block {
                let steps = s / lsb(depth);
                assert!([-1.0, 0.0, 1.0].contains(&steps), "{depth:?}: {steps}");
            }
            assert!(
                block.iter().any(|&s| s != 0.0),
                "dither is audible as noise"
            );
        }
    }

    #[test]
    fn test_process_outputs_whole_steps_near_input() {
        let depth = Depth::Int16;
        let mut d = Ditherer::new(depth, 11);
        let input: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.37).sin() * 0.8).collect();
        let mut out = input.clone();
        d.process(&mut out);
        for (&x, &y) in input.iter().zip(&out) {
            let steps = y * depth.full_scale();
            assert_eq!(steps, steps.round(), "whole LSB steps");
            assert!((y - x).abs() <= 1.5 * lsb(depth), "{x} → {y}");
        }
    }

    #[test]
    fn test_process_preserves_sub_lsb_dc_on_average() {
        // TPDF dither makes the quantiser's average output equal its input, even below 1 LSB.
        let depth = Depth::Int16;
        let mut d = Ditherer::new(depth, 5);
        let level = 0.3 * lsb(depth);
        let mut block = vec![level; 200_000];
        d.process(&mut block);
        let mean = block.iter().map(|&s| f64::from(s)).sum::<f64>() / block.len() as f64;
        assert!(
            (mean - f64::from(level)).abs() < 0.01 * f64::from(lsb(depth)),
            "{mean}"
        );
    }

    #[test]
    fn test_process_clips_and_handles_nan() {
        let depth = Depth::Int24;
        let mut d = Ditherer::new(depth, 1);
        let mut block = [2.0, -2.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
        d.process(&mut block);
        assert_eq!(block, [1.0 - lsb(depth), -1.0, 0.0, 1.0 - lsb(depth), -1.0]);
    }

    #[test]
    fn test_process_same_seed_same_output() {
        let mut a = [0.1_f32; 64];
        let mut b = a;
        Ditherer::new(Depth::Int16, 9).process(&mut a);
        Ditherer::new(Depth::Int16, 9).process(&mut b);
        assert_eq!(a, b);
    }
}
