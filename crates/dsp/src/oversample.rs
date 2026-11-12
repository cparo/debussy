//! Integer-factor oversampling with a whole-sample delay (`docs/architecture/effects.md`
//! "Dynamics", ADR-0099).
//!
//! Up-sampling inserts `factor - 1` zeros after each sample and low-pass filters; down-sampling
//! low-pass filters and keeps every `factor`-th sample. Both use one linear-phase Kaiser-windowed
//! sinc of `2·HALF·factor + 1` taps cut off at half the base rate, so each delays by `HALF` base
//! frames and a round trip by `2·HALF`. The design is the one in `resample` (ADR-0036), with the
//! length fixed so the delay is whole.

use std::f64::consts::PI;

use crate::resample::{bessel_i0, kaiser_beta};

/// The largest factor an [`Oversampler`] accepts.
pub const MAX_OVERSAMPLING: usize = 16;

/// Each filter's delay, in base-rate frames.
const HALF: usize = 16;

/// The stopband attenuation designed for, in dB.
const ATTENUATION_DB: f64 = 80.0;

/// One channel's up- and down-sampler at a fixed factor.
///
/// [`Oversampler::new`] allocates and designs the filter; call it off the real-time thread.
/// [`Oversampler::up`], [`Oversampler::down`] and [`Oversampler::reset`] are real-time safe.
#[derive(Debug, Clone)]
pub struct Oversampler {
    factor: usize,
    /// The prototype low-pass, unity gain at DC, `2·HALF·factor + 1` taps.
    kernel: Box<[f32]>,
    /// The last `2·HALF + 1` base-rate inputs, newest first, stored twice so a contiguous slice
    /// is always available from `up_newest`.
    up_history: Box<[f32]>,
    up_newest: usize,
    /// The last `kernel.len() + factor - 1` high-rate inputs, newest first, stored twice.
    down_history: Box<[f32]>,
    down_newest: usize,
}

impl Oversampler {
    /// An oversampler by `factor`, clamped to 1 to [`MAX_OVERSAMPLING`]. A factor of 1 passes
    /// samples through with no delay. Allocates.
    pub fn new(factor: usize) -> Self {
        let factor = factor.clamp(1, MAX_OVERSAMPLING);
        let kernel = if factor == 1 {
            vec![1.0_f32].into_boxed_slice()
        } else {
            design(factor)
        };
        let up_length = if factor == 1 { 1 } else { 2 * HALF + 1 };
        let down_length = kernel.len() + factor - 1;
        Oversampler {
            factor,
            kernel,
            up_history: vec![0.0; 2 * up_length].into_boxed_slice(),
            up_newest: 0,
            down_history: vec![0.0; 2 * down_length].into_boxed_slice(),
            down_newest: 0,
        }
    }

    /// The factor.
    pub fn factor(&self) -> usize {
        self.factor
    }

    /// The delay of a round trip through [`Oversampler::up`] and [`Oversampler::down`], in
    /// base-rate frames: 0 at factor 1, else 32.
    pub fn latency(&self) -> usize {
        if self.factor == 1 { 0 } else { 2 * HALF }
    }

    /// Up-samples one base-rate sample into the first [`Oversampler::factor`] values of
    /// `output`, oldest first. Real-time safe.
    pub fn up(&mut self, input: f32, output: &mut [f32]) {
        if self.factor == 1 {
            if let Some(out) = output.first_mut() {
                *out = input;
            }
            return;
        }
        let length = self.up_history.len() / 2;
        push(&mut self.up_history, &mut self.up_newest, length, input);
        let recent = &self.up_history[self.up_newest..self.up_newest + length];
        // Zero-stuffing divides the level by the factor; the filter's taps are scaled back up.
        #[allow(clippy::cast_precision_loss)] // a factor of at most 16
        let gain = self.factor as f32;
        for (phase, out) in output.iter_mut().take(self.factor).enumerate() {
            // Output `phase` after the newest input uses taps phase, phase + F, ... against the
            // inputs newest first.
            let sum: f32 = self
                .kernel
                .iter()
                .skip(phase)
                .step_by(self.factor)
                .zip(recent)
                .map(|(h, x)| h * x)
                .sum();
            *out = gain * sum;
        }
    }

    /// Down-samples the first [`Oversampler::factor`] values of `input`, oldest first, to one
    /// base-rate sample. Real-time safe.
    pub fn down(&mut self, input: &[f32]) -> f32 {
        if self.factor == 1 {
            return input.first().copied().unwrap_or(0.0);
        }
        let length = self.down_history.len() / 2;
        for &sample in input.iter().take(self.factor) {
            push(
                &mut self.down_history,
                &mut self.down_newest,
                length,
                sample,
            );
        }
        // Filter at the group's first sample, so the round trip's delay is whole.
        let start = self.down_newest + self.factor - 1;
        let recent = &self.down_history[start..start + self.kernel.len()];
        self.kernel.iter().zip(recent).map(|(h, x)| h * x).sum()
    }

    /// Clears both filters' memory. Real-time safe.
    pub fn reset(&mut self) {
        self.up_history.fill(0.0);
        self.down_history.fill(0.0);
        self.up_newest = 0;
        self.down_newest = 0;
    }
}

/// Pushes `sample` into a doubled history of `length` values, newest first.
fn push(history: &mut [f32], newest: &mut usize, length: usize, sample: f32) {
    *newest = if *newest == 0 {
        length - 1
    } else {
        *newest - 1
    };
    history[*newest] = sample;
    history[*newest + length] = sample;
}

/// The prototype low-pass for `factor`: cut off at half the base rate, unity gain at DC.
fn design(factor: usize) -> Box<[f32]> {
    let length = 2 * HALF * factor + 1;
    let centre = (HALF * factor) as f64;
    let cutoff = 0.5 / factor as f64;
    let beta = kaiser_beta(ATTENUATION_DB);
    let i0_beta = bessel_i0(beta);
    let prototype: Vec<f64> = (0..length)
        .map(|i| {
            let t = i as f64 - centre;
            let sinc = if t == 0.0 {
                2.0 * cutoff
            } else {
                (2.0 * PI * cutoff * t).sin() / (PI * t)
            };
            let x = t / centre;
            sinc * bessel_i0(beta * (1.0 - x * x).max(0.0).sqrt()) / i0_beta
        })
        .collect();
    let sum: f64 = prototype.iter().sum();
    // Coefficients are well inside `f32`.
    #[allow(clippy::cast_possible_truncation)]
    let kernel: Box<[f32]> = prototype.iter().map(|h| (h / sum) as f32).collect();
    kernel
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips `input` through an oversampler by `factor`.
    fn round_trip(factor: usize, input: &[f32]) -> Vec<f32> {
        let mut oversampler = Oversampler::new(factor);
        let mut high = vec![0.0; factor];
        input
            .iter()
            .map(|&x| {
                oversampler.up(x, &mut high);
                oversampler.down(&high)
            })
            .collect()
    }

    #[test]
    fn test_new_clamps_factor_and_reports_latency() {
        assert_eq!(Oversampler::new(0).factor(), 1);
        assert_eq!(Oversampler::new(99).factor(), MAX_OVERSAMPLING);
        assert_eq!(Oversampler::new(1).latency(), 0);
        assert_eq!(Oversampler::new(4).latency(), 32);
    }

    #[test]
    fn test_round_trip_factor_one_is_identity() {
        let input = [0.5, -1.0, 0.25];
        assert_eq!(round_trip(1, &input), input);
    }

    #[test]
    fn test_round_trip_delays_impulse_by_latency() {
        for factor in [2, 4, 8] {
            let mut input = vec![0.0; 100];
            input[0] = 1.0;
            let output = round_trip(factor, &input);
            let (peak, value) = output
                .iter()
                .enumerate()
                .fold((0, 0.0_f32), |best, (i, &v)| {
                    if v.abs() > best.1.abs() { (i, v) } else { best }
                });
            assert_eq!(peak, 2 * HALF, "factor {factor}");
            assert!((value - 1.0).abs() < 0.05, "factor {factor}: {value}");
        }
    }

    #[test]
    fn test_round_trip_passes_audio_band_tone() {
        // 1 kHz at 48 kHz: same amplitude, delayed by the latency.
        let input: Vec<f32> = (0..2_000)
            .map(|i| (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / 48_000.0).sin())
            .collect();
        for factor in [2, 4, 8] {
            let output = round_trip(factor, &input);
            for i in 200..2_000 {
                assert!(
                    (output[i] - input[i - 2 * HALF]).abs() < 1e-3,
                    "factor {factor} at {i}"
                );
            }
        }
    }

    #[test]
    fn test_up_holds_dc_in_every_phase() {
        let mut oversampler = Oversampler::new(4);
        let mut high = [0.0; 4];
        for _ in 0..100 {
            oversampler.up(1.0, &mut high);
        }
        for value in high {
            assert!((value - 1.0).abs() < 1e-3, "{value}");
        }
    }

    #[test]
    fn test_down_rejects_tone_above_base_nyquist() {
        // 36 kHz at the 4 × 48 kHz rate would alias to 12 kHz; it must be filtered out.
        let factor = 4;
        let rate = 48_000.0 * factor as f32;
        let mut oversampler = Oversampler::new(factor);
        let mut peak = 0.0_f32;
        for i in 0..2_000 {
            let group: Vec<f32> = (0..factor)
                .map(|j| {
                    let n = (i * factor + j) as f32;
                    (2.0 * std::f32::consts::PI * 36_000.0 * n / rate).sin()
                })
                .collect();
            let out = oversampler.down(&group);
            if i > 100 {
                peak = peak.max(out.abs());
            }
        }
        assert!(peak < 1e-3, "{peak}");
    }
}
