//! Exact rational polyphase sample-rate conversion (`docs/architecture/engine.md` "Sample rate",
//! ADR-0036).
//!
//! For rates `fs_in → fs_out` with greatest common divisor `g`, the signal is conceptually
//! upsampled by `L = fs_out / g`, lowpass-filtered, and downsampled by `M = fs_in / g`. Only the
//! needed outputs are computed: the lowpass is split into `L` precomputed phases, and a phase
//! accumulator advances by `M` modulo `L` per output frame. Coefficients are never interpolated
//! or computed while converting, and the ratio is fixed when the converter is made.
//!
//! The lowpass is a Kaiser-windowed sinc, designed in `f64` and stored as `f32`.

use std::f64::consts::PI;

use crate::error::Error;

/// The most filter phases (`L`) a converter may need. Larger exact ratios are refused.
pub const MAX_PHASES: u64 = 4096;

/// Passband edge as a fraction of the lower of the two rates: 20 kHz at 44.1 kHz.
const PASSBAND: f64 = 0.4535;
/// Extra stopband attenuation designed in, beyond each preset's target, in dB.
const MARGIN_DB: f64 = 6.0;

/// A quality preset (ADR-0036).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Offline import: passband flat within ±0.01 dB, stopband from half the lower rate, at least
    /// 120 dB down.
    Import,
    /// Real-time device conversion: the same passband, stopband from the lower rate minus the
    /// passband edge, at least 96 dB down. Shorter, so lower latency and cost.
    Device,
}

impl Quality {
    fn attenuation_db(self) -> f64 {
        match self {
            Quality::Import => 120.0,
            Quality::Device => 96.0,
        }
    }

    /// The stopband edge for a lower rate of `f_min`.
    fn stopband(self, f_min: f64) -> f64 {
        match self {
            Quality::Import => f_min / 2.0,
            Quality::Device => f_min - PASSBAND * f_min,
        }
    }
}

/// A sample-rate converter for one channel at a fixed ratio.
///
/// [`Resampler::new`] allocates and designs the filter; call it off the real-time thread.
/// [`Resampler::process`] and [`Resampler::reset`] are real-time safe.
#[derive(Debug, Clone)]
pub struct Resampler {
    /// Upsampling factor: the number of phases.
    up: usize,
    /// Downsampling factor: the accumulator's step.
    down: usize,
    /// Taps per phase.
    taps: usize,
    /// `up` phases of `taps` coefficients each, phase by phase.
    coefficients: Box<[f32]>,
    /// The last `taps` inputs, newest first, stored twice so a contiguous slice is always
    /// available: `history[newest..newest + taps]`.
    history: Box<[f32]>,
    newest: usize,
    /// The next output's position past the newest input, in upsampled samples; `up` or more means
    /// another input is needed first.
    phase: usize,
    /// Group delay, in output frames.
    latency: f64,
}

impl Resampler {
    /// A converter from `from` Hz to `to` Hz at `quality`.
    ///
    /// # Errors
    ///
    /// [`Error::ZeroRate`], or [`Error::RatioTooComplex`] if the exact ratio needs more than
    /// [`MAX_PHASES`] phases.
    pub fn new(from: u32, to: u32, quality: Quality) -> Result<Self, Error> {
        if from == 0 || to == 0 {
            return Err(Error::ZeroRate);
        }
        let g = gcd(u64::from(from), u64::from(to));
        let up = u64::from(to) / g;
        let down = u64::from(from) / g;
        if up > MAX_PHASES {
            return Err(Error::RatioTooComplex {
                from,
                to,
                phases: up,
            });
        }
        // Both fit: `up` is at most MAX_PHASES and `down` at most u32::MAX.
        let (up, down) = (up as usize, down as usize);

        let (taps, coefficients) = if from == to {
            (1, vec![1.0_f32].into_boxed_slice())
        } else {
            design(f64::from(from), f64::from(to), up, quality)
        };
        let length = taps * up;
        let latency = if from == to {
            0.0
        } else {
            (length - 1) as f64 / 2.0 / down as f64
        };
        Ok(Self {
            up,
            down,
            taps,
            coefficients,
            history: vec![0.0; 2 * taps].into_boxed_slice(),
            newest: 0,
            phase: up,
            latency,
        })
    }

    /// Converts as much of `input` into `output` as possible, returning `(consumed, produced)`.
    /// Stops when either runs out; pass the unconsumed input again with the next call. Real-time
    /// safe.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) -> (usize, usize) {
        let (mut consumed, mut produced) = (0, 0);
        loop {
            while self.phase >= self.up {
                let Some(&sample) = input.get(consumed) else {
                    return (consumed, produced);
                };
                self.push(sample);
                consumed += 1;
                self.phase -= self.up;
            }
            let Some(out) = output.get_mut(produced) else {
                return (consumed, produced);
            };
            let start = self.phase * self.taps;
            let phase = &self.coefficients[start..start + self.taps];
            let recent = &self.history[self.newest..self.newest + self.taps];
            *out = phase.iter().zip(recent).map(|(c, x)| c * x).sum();
            produced += 1;
            self.phase += self.down;
        }
    }

    /// Clears the converter's memory, as if newly made. Real-time safe.
    pub fn reset(&mut self) {
        self.history.fill(0.0);
        self.newest = 0;
        self.phase = self.up;
    }

    /// The delay through the converter, in output frames, for delay compensation (ADR-0022).
    pub fn latency(&self) -> f64 {
        self.latency
    }

    /// The exact ratio as `(L, M)`: output frames per input frame is `L / M`.
    pub fn ratio(&self) -> (usize, usize) {
        (self.up, self.down)
    }

    /// Taps per output frame: the cost of each output, in multiply-adds.
    pub fn taps(&self) -> usize {
        self.taps
    }

    fn push(&mut self, sample: f32) {
        self.newest = if self.newest == 0 {
            self.taps - 1
        } else {
            self.newest - 1
        };
        self.history[self.newest] = sample;
        self.history[self.newest + self.taps] = sample;
    }
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Designs the lowpass for `from → to` with `up` phases, returning taps per phase and the
/// coefficients arranged phase by phase.
fn design(from: f64, to: f64, up: usize, quality: Quality) -> (usize, Box<[f32]>) {
    let f_min = from.min(to);
    let pass = PASSBAND * f_min;
    let stop = quality.stopband(f_min);
    let rate = from * up as f64;
    let attenuation = quality.attenuation_db() + MARGIN_DB;

    // Kaiser's estimate of the length for this attenuation and transition width.
    let transition = (stop - pass) / rate;
    let estimate = ((attenuation - 7.95) / (14.36 * transition)).ceil() + 1.0;
    // At least 1; the estimate is finite and positive for valid rates.
    let taps = ((estimate / up as f64).ceil() as usize).max(1);
    let length = taps * up;

    let beta = kaiser_beta(attenuation);
    let cutoff = (pass + stop) / 2.0 / rate;
    let centre = (length - 1) as f64 / 2.0;
    let i0_beta = bessel_i0(beta);
    let mut prototype: Vec<f64> = (0..length)
        .map(|i| {
            let t = i as f64 - centre;
            let sinc = if t == 0.0 {
                2.0 * cutoff
            } else {
                (2.0 * PI * cutoff * t).sin() / (PI * t)
            };
            let x = if length > 1 {
                2.0 * i as f64 / (length - 1) as f64 - 1.0
            } else {
                0.0
            };
            sinc * bessel_i0(beta * (1.0 - x * x).max(0.0).sqrt()) / i0_beta
        })
        .collect();
    // Unity gain at DC after upsampling by `up`, which divides the signal's level by `up`.
    let sum: f64 = prototype.iter().sum();
    for h in &mut prototype {
        *h *= up as f64 / sum;
    }

    let mut coefficients = vec![0.0_f32; length];
    for phase in 0..up {
        for k in 0..taps {
            coefficients[phase * taps + k] = prototype[phase + k * up] as f32;
        }
    }
    (taps, coefficients.into_boxed_slice())
}

/// Kaiser's β for a stopband attenuation in dB.
pub(crate) fn kaiser_beta(attenuation: f64) -> f64 {
    if attenuation > 50.0 {
        0.1102 * (attenuation - 8.7)
    } else if attenuation >= 21.0 {
        0.5842 * (attenuation - 21.0).powf(0.4) + 0.07886 * (attenuation - 21.0)
    } else {
        0.0
    }
}

/// The zeroth-order modified Bessel function of the first kind, by its power series.
pub(crate) fn bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..500 {
        term *= half / k as f64;
        let square = term * term;
        sum += square;
        if square < sum * 1e-17 {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_finds_exact_ratio() -> Result<(), Error> {
        assert_eq!(
            Resampler::new(44_100, 48_000, Quality::Device)?.ratio(),
            (160, 147)
        );
        assert_eq!(
            Resampler::new(96_000, 44_100, Quality::Device)?.ratio(),
            (147, 320)
        );
        assert_eq!(
            Resampler::new(48_000, 48_000, Quality::Import)?.ratio(),
            (1, 1)
        );
        Ok(())
    }

    #[test]
    fn test_new_rejects_zero_and_complex_ratios() {
        assert_eq!(
            Resampler::new(0, 48_000, Quality::Device).err(),
            Some(Error::ZeroRate)
        );
        assert!(matches!(
            Resampler::new(44_100, 44_101, Quality::Device),
            Err(Error::RatioTooComplex { phases: 44_101, .. })
        ));
    }

    #[test]
    fn test_device_preset_is_shorter_than_import() -> Result<(), Error> {
        let import = Resampler::new(44_100, 48_000, Quality::Import)?;
        let device = Resampler::new(44_100, 48_000, Quality::Device)?;
        assert!(
            device.taps() * 2 <= import.taps(),
            "{} vs {}",
            device.taps(),
            import.taps()
        );
        assert!(device.latency() < import.latency());
        Ok(())
    }

    #[test]
    fn test_same_rate_passes_through() -> Result<(), Error> {
        let mut r = Resampler::new(48_000, 48_000, Quality::Import)?;
        let input = [0.5, -0.25, 1.0];
        let mut output = [0.0; 3];
        assert_eq!(r.process(&input, &mut output), (3, 3));
        assert_eq!(output, input);
        assert_eq!(r.latency(), 0.0);
        Ok(())
    }

    #[test]
    fn test_bessel_i0_known_values() {
        assert!((bessel_i0(0.0) - 1.0).abs() < 1e-15);
        assert!((bessel_i0(1.0) - 1.266_065_877_752_008_4).abs() < 1e-12);
        assert!((bessel_i0(10.0) - 2_815.716_628_466_254).abs() < 1e-8);
    }
}
