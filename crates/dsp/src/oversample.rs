//! Oversampling around a nonlinearity (`docs/architecture/effects.md` "Amplifier", ADR-0102).
//!
//! An [`Oversampler`] runs one channel's samples through a function at 2 or 4 times the base
//! rate: each base frame is upsampled by the [`Resampler`] (`Quality::Device`), each of the
//! resulting sub-samples goes through the function, and the downsampler filters and keeps one
//! sample per base frame. The two filters' combined delay is made a whole number of base frames
//! by padding the downsampler to an even length and topping up with a short delay at the
//! oversampled rate, so the latency it reports is exact.

use crate::dynamics::DelayLine;
use crate::error::Error;
use crate::resample::{Quality, Resampler};

/// The largest oversampling factor.
pub const MAX_FACTOR: usize = 4;

/// How many times the base rate the function runs at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Factor {
    /// The base rate: no filtering, no latency.
    One,
    /// Twice the base rate.
    Two,
    /// Four times the base rate.
    Four,
}

impl Factor {
    /// The factor as a number of sub-samples per base frame.
    pub fn value(self) -> usize {
        match self {
            Factor::One => 1,
            Factor::Two => 2,
            Factor::Four => 4,
        }
    }
}

/// The filters of an oversampler above factor 1.
#[derive(Debug, Clone)]
struct Filters {
    up: Resampler,
    down: Resampler,
    /// The top-up delay at the oversampled rate.
    pad: DelayLine,
}

/// One channel's oversampler at a fixed factor and rate.
///
/// [`Oversampler::new`] allocates and designs the filters; call it off the real-time thread.
/// [`Oversampler::process`] and [`Oversampler::reset`] are real-time safe.
#[derive(Debug, Clone)]
pub struct Oversampler {
    factor: usize,
    filters: Option<Filters>,
    /// The delay through the filters, in base frames.
    latency: usize,
}

impl Oversampler {
    /// An oversampler by `factor` for a base rate of `sample_rate` Hz.
    ///
    /// # Errors
    ///
    /// [`Error::ZeroRate`] for a zero rate, or [`Error::RateTooHigh`] if the oversampled rate
    /// does not fit in a `u32`.
    pub fn new(factor: Factor, sample_rate: u32) -> Result<Self, Error> {
        if sample_rate == 0 {
            return Err(Error::ZeroRate);
        }
        let n = factor.value();
        if n == 1 {
            return Ok(Oversampler {
                factor: 1,
                filters: None,
                latency: 0,
            });
        }
        // `n` is at most MAX_FACTOR.
        #[allow(clippy::cast_possible_truncation)]
        let high = sample_rate
            .checked_mul(n as u32)
            .ok_or(Error::RateTooHigh(sample_rate))?;
        let up = Resampler::new(sample_rate, high, Quality::Device)?;
        // An even downsampler length makes the summed half-lengths whole, as the upsampler's
        // length is a multiple of `n`, which is even.
        let down = Resampler::with_taps_multiple(high, sample_rate, Quality::Device, 2)?;
        let length_up = up.taps() * n;
        let length_down = down.taps();
        let delay = (length_up + length_down - 2) / 2;
        let pad = (n - delay % n) % n;
        Ok(Oversampler {
            factor: n,
            filters: Some(Filters {
                up,
                down,
                pad: DelayLine::new(pad),
            }),
            latency: (delay + pad) / n,
        })
    }

    /// Runs one base frame `input` through `function` at the oversampled rate and returns one base
    /// frame of output, as of [`Oversampler::latency`] frames ago. Real-time safe.
    pub fn process(&mut self, input: f32, mut function: impl FnMut(f32) -> f32) -> f32 {
        let Some(filters) = self.filters.as_mut() else {
            return function(input);
        };
        let n = self.factor;
        let mut high = [0.0_f32; MAX_FACTOR];
        filters.up.process(&[input], &mut high[..n]);
        for sample in &mut high[..n] {
            *sample = filters.pad.process(function(*sample));
        }
        let mut output = [0.0_f32];
        filters.down.process(&high[..n], &mut output);
        output[0]
    }

    /// Clears the filters' memory, as if newly made. Real-time safe.
    pub fn reset(&mut self) {
        if let Some(filters) = self.filters.as_mut() {
            filters.up.reset();
            filters.down.reset();
            let frames = filters.pad.frames();
            for _ in 0..frames {
                filters.pad.process(0.0);
            }
        }
    }

    /// The delay through the oversampler, in whole base frames; 0 at factor 1.
    pub fn latency(&self) -> usize {
        self.latency
    }

    /// The sub-samples per base frame.
    pub fn factor(&self) -> usize {
        self.factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [u32; 3] = [44_100, 48_000, 96_000];

    #[test]
    fn test_new_factor_one_is_identity() -> Result<(), Error> {
        let mut o = Oversampler::new(Factor::One, 48_000)?;
        assert_eq!(o.latency(), 0);
        for x in [0.5, -0.25, 1.0] {
            assert_eq!(o.process(x, |u| u), x);
        }
        Ok(())
    }

    #[test]
    fn test_new_rejects_zero_and_huge_rates() {
        assert_eq!(
            Oversampler::new(Factor::Two, 0).err(),
            Some(Error::ZeroRate)
        );
        assert_eq!(
            Oversampler::new(Factor::Four, u32::MAX).err(),
            Some(Error::RateTooHigh(u32::MAX))
        );
    }

    #[test]
    fn test_process_impulse_arrives_at_latency() -> Result<(), Error> {
        for factor in [Factor::Two, Factor::Four] {
            for rate in RATES {
                let mut o = Oversampler::new(factor, rate)?;
                let latency = o.latency();
                assert!(latency > 0 && latency < 128, "{factor:?} {rate}: {latency}");
                let output: Vec<f32> = (0..latency * 2 + 8)
                    .map(|i| o.process(if i == 0 { 1.0 } else { 0.0 }, |u| u))
                    .collect();
                let peak = output
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                    .map(|(i, _)| i);
                assert_eq!(peak, Some(latency), "{factor:?} {rate}");
                // Symmetric about the peak: a whole-frame delay, not a fractional one.
                for k in 1..latency.min(6) {
                    let (before, after) = (output[latency - k], output[latency + k]);
                    assert!((before - after).abs() < 1e-4, "{factor:?} {rate} ±{k}");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_process_passes_a_sine_flat() -> Result<(), Error> {
        for factor in [Factor::Two, Factor::Four] {
            for rate in RATES {
                let mut o = Oversampler::new(factor, rate)?;
                let latency = o.latency();
                let w = 2.0 * std::f64::consts::PI * 1000.0 / f64::from(rate);
                let frames = 4 * rate as usize / 10;
                let mut peak_in = 0.0_f64;
                let mut peak_out = 0.0_f64;
                let mut error = 0.0_f64;
                for i in 0..frames {
                    #[allow(clippy::cast_precision_loss)]
                    let x = (w * i as f64).sin();
                    let y = f64::from(o.process(x as f32, |u| u));
                    if i > frames / 2 {
                        #[allow(clippy::cast_precision_loss)]
                        let expected = (w * (i - latency) as f64).sin();
                        peak_in = peak_in.max(expected.abs());
                        peak_out = peak_out.max(y.abs());
                        error = error.max((y - expected).abs());
                    }
                }
                let db = 20.0 * (peak_out / peak_in).log10();
                assert!(db.abs() < 0.01, "{factor:?} {rate}: {db} dB");
                // In phase: the latency is exact.
                assert!(error < 2e-3, "{factor:?} {rate}: {error}");
            }
        }
        Ok(())
    }

    #[test]
    fn test_process_runs_function_at_the_oversampled_rate() -> Result<(), Error> {
        let mut o = Oversampler::new(Factor::Four, 48_000)?;
        let mut calls = 0;
        for _ in 0..10 {
            o.process(0.0, |u| {
                calls += 1;
                u
            });
        }
        assert_eq!(calls, 40);
        Ok(())
    }

    #[test]
    fn test_reset_clears_memory() -> Result<(), Error> {
        let mut o = Oversampler::new(Factor::Two, 48_000)?;
        for _ in 0..100 {
            o.process(1.0, |u| u);
        }
        o.reset();
        for _ in 0..100 {
            assert_eq!(o.process(0.0, |u| u), 0.0);
        }
        Ok(())
    }
}
