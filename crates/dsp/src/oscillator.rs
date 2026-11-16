//! Audio-rate oscillators: naive saw, pulse and triangle waves corrected at each discontinuity by a
//! minimum-phase band-limited step (MinBLEP), and [`note_to_hz`]
//! (`docs/architecture/instruments.md` "Synthesizer", ADR-0148).
//!
//! A [`MinBlep`] table holds the step's residual: the band-limited step minus the ideal one,
//! sampled [`OVERSAMPLING`] times per output sample over [`BLEP_SAMPLES`] samples. It is built
//! once, off the real-time thread, by turning a windowed sinc into its minimum-phase form through
//! the real cepstrum and integrating it. An [`Oscillator`] adds the residual, scaled by the step's
//! height and read at the step's fractional position, into a short ring of pending corrections.
//! The residual is causal, so no output is delayed.

use std::f32::consts::PI;

use crate::error::Error;
use crate::fft::{Complex, Fft};

/// How many output samples a step's residual lasts.
pub const BLEP_SAMPLES: usize = 32;
/// How many table points per output sample.
pub const OVERSAMPLING: usize = 64;
/// The sinc's cutoff as a share of the output Nyquist frequency: just below it, so the window's
/// transition band, and the ringing it causes, stays mostly below Nyquist.
const CUTOFF: f32 = 0.9;
/// The cepstrum's transform size: several times the sinc's length, so the folded cepstrum
/// barely aliases.
const CEPSTRUM_SIZE: usize = 8_192;
/// The smallest magnitude whose logarithm is taken, so a stopband null stays finite.
const MAGNITUDE_FLOOR: f32 = 1e-9;
/// The highest phase increment, in cycles per sample: just under Nyquist.
const MAX_INCREMENT: f32 = 0.45;
/// The narrowest and widest pulse, as a fraction of the cycle.
const PULSE_RANGE: (f32, f32) = (0.02, 0.98);
/// The triangle integrator's leak per sample, which keeps its offset from drifting.
const TRIANGLE_LEAK: f32 = 0.0005;

/// The frequency of MIDI note `note` in 12-tone equal temperament, with A4 (note 69) at 440 Hz.
/// Fractional notes are cents between semitones.
pub fn note_to_hz(note: f32) -> f32 {
    440.0 * ((note - 69.0) / 12.0).exp2()
}

/// The residual of a minimum-phase band-limited unit step: the band-limited step minus the
/// ideal step, from the step onwards, in `BLEP_SAMPLES · OVERSAMPLING + 1` points ending at 0.
/// Built once; read by every [`Oscillator`].
#[derive(Debug, Clone, PartialEq)]
pub struct MinBlep {
    residual: Box<[f32]>,
    /// The residual's area, in samples: minus the step's delay against the ideal step.
    area: f32,
}

impl MinBlep {
    /// Builds the table. Allocates and runs four transforms of [`CEPSTRUM_SIZE`]; keep it off the
    /// real-time thread.
    ///
    /// # Errors
    ///
    /// An [`Error`] from the FFT, which does not happen for the fixed size here.
    pub fn new() -> Result<Self, Error> {
        let points = BLEP_SAMPLES * OVERSAMPLING + 1;
        let fft = Fft::new(CEPSTRUM_SIZE)?;
        // A Blackman-windowed sinc, cut off at the output Nyquist frequency.
        let mut data = vec![Complex::default(); CEPSTRUM_SIZE];
        let centre = (points - 1) as f32 / 2.0;
        for (n, x) in data.iter_mut().take(points).enumerate() {
            let t = (n as f32 - centre) / OVERSAMPLING as f32;
            let arg = PI * CUTOFF * t;
            let sinc = if arg == 0.0 { 1.0 } else { arg.sin() / arg };
            let w = n as f32 / (points - 1) as f32;
            let window = 0.42 - 0.5 * (2.0 * PI * w).cos() + 0.08 * (4.0 * PI * w).cos();
            x.re = sinc * window;
        }
        // The real cepstrum: the inverse transform of the log magnitude.
        fft.forward(&mut data)?;
        for x in data.iter_mut() {
            *x = Complex::new(x.abs().max(MAGNITUDE_FLOOR).ln(), 0.0);
        }
        fft.inverse(&mut data)?;
        // Folding the cepstrum onto positive quefrencies makes the minimum-phase spectrum's log.
        let half = CEPSTRUM_SIZE / 2;
        for (n, x) in data.iter_mut().enumerate() {
            let fold = match n {
                0 => 1.0,
                n if n < half => 2.0,
                n if n == half => 1.0,
                _ => 0.0,
            };
            *x = Complex::new(x.re * fold, 0.0);
        }
        fft.forward(&mut data)?;
        for x in data.iter_mut() {
            let magnitude = x.re.exp();
            *x = Complex::new(magnitude * x.im.cos(), magnitude * x.im.sin());
        }
        fft.inverse(&mut data)?;
        // Integrated, the minimum-phase impulse is the step; normalised to end at exactly 1.
        let mut sum = 0.0_f64;
        let mut step: Vec<f64> = data
            .iter()
            .take(points)
            .map(|x| {
                sum += f64::from(x.re);
                sum
            })
            .collect();
        let end = step.last().copied().unwrap_or(1.0);
        let end = if end.abs() > f64::EPSILON { end } else { 1.0 };
        for s in &mut step {
            *s /= end;
        }
        // The residual against the ideal step, whose last point is 0 by the normalisation.
        let residual: Box<[f32]> = step.iter().map(|&s| (s - 1.0) as f32).collect();
        let area = residual.iter().map(|&r| f64::from(r)).sum::<f64>() / OVERSAMPLING as f64;
        Ok(MinBlep {
            residual,
            area: area as f32,
        })
    }

    /// The residual at `t` samples after the step, by linear interpolation; 0 from
    /// [`BLEP_SAMPLES`] on.
    fn at(&self, t: f32) -> f32 {
        let position = t.max(0.0) * OVERSAMPLING as f32;
        // Non-negative and finite; `as` saturates a huge value, which the bound check catches.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = position as usize;
        match (self.residual.get(index), self.residual.get(index + 1)) {
            (Some(&a), Some(&b)) => a + (b - a) * (position - index as f32),
            (Some(&a), None) => a,
            _ => 0.0,
        }
    }
}

/// An oscillator's waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wave {
    /// A ramp from −1 up to 1, then a step down.
    #[default]
    Saw,
    /// 1 for the pulse width's share of each cycle, −1 for the rest.
    Pulse,
    /// A rise from −1 to 1 and a fall back, integrated from a square.
    Triangle,
}

/// A phase-accumulator oscillator whose saw and pulse steps are corrected by a [`MinBlep`], and
/// whose triangle is a leaky integral of its corrected square. Never allocates.
#[derive(Debug, Clone, PartialEq)]
pub struct Oscillator {
    wave: Wave,
    /// The phase, `0 ≤ phase < 1`, in cycles.
    phase: f32,
    /// The phase step per sample.
    increment: f32,
    /// The pulse width, within [`PULSE_RANGE`].
    width: f32,
    /// Whether the pulse (or the triangle's square) is at its high level.
    high: bool,
    /// The triangle integrator's state.
    triangle: f32,
    /// Pending corrections for this and the next `BLEP_SAMPLES − 1` samples, as a ring.
    pending: [f32; BLEP_SAMPLES],
    /// Where in `pending` this sample's correction is.
    position: usize,
}

impl Default for Oscillator {
    fn default() -> Self {
        Self::new()
    }
}

impl Oscillator {
    /// A saw at 0 Hz and phase 0, with a pulse width of one half.
    pub fn new() -> Self {
        Oscillator {
            wave: Wave::Saw,
            phase: 0.0,
            increment: 0.0,
            width: 0.5,
            high: true,
            triangle: -1.0,
            pending: [0.0; BLEP_SAMPLES],
            position: 0,
        }
    }

    /// Sets the waveform; the phase continues. Pending corrections are kept, so a change mid-note
    /// steps once rather than ringing.
    pub fn set_wave(&mut self, wave: Wave) {
        self.wave = wave;
    }

    /// The waveform.
    pub fn wave(&self) -> Wave {
        self.wave
    }

    /// Sets the frequency to `hz` at `sample_rate`. A negative or NaN frequency is 0; one too
    /// close to Nyquist is held just below it.
    pub fn set_frequency(&mut self, hz: f32, sample_rate: f32) {
        let increment = hz / sample_rate;
        self.increment = if increment.is_nan() {
            0.0
        } else {
            increment.clamp(0.0, MAX_INCREMENT)
        };
    }

    /// Sets the pulse width as a fraction of the cycle, clamped to 0.02–0.98; NaN is one half.
    /// Only [`Wave::Pulse`] uses it.
    pub fn set_pulse_width(&mut self, width: f32) {
        self.width = if width.is_nan() {
            0.5
        } else {
            width.clamp(PULSE_RANGE.0, PULSE_RANGE.1)
        };
    }

    /// Returns to phase 0 with no pending corrections.
    pub fn reset(&mut self) {
        *self = Oscillator {
            wave: self.wave,
            increment: self.increment,
            width: self.width,
            ..Oscillator::new()
        };
    }

    /// The next sample, from −1 to 1 apart from the band-limited steps' overshoot.
    pub fn next(&mut self, table: &MinBlep) -> f32 {
        self.phase += self.increment;
        let wrapped = self.phase >= 1.0;
        if wrapped {
            self.phase -= 1.0;
        }
        let naive = match self.wave {
            Wave::Saw => {
                if wrapped {
                    self.step(table, self.phase, -2.0);
                }
                // The minimum-phase step lags the ideal one; shifting the ramp by the same lag
                // keeps the wave centred on 0. A pulse's rise and fall cancel theirs.
                2.0 * (self.phase + table.area * self.increment) - 1.0
            }
            Wave::Pulse => self.square(table, wrapped, self.width),
            Wave::Triangle => {
                let square = self.square(table, wrapped, 0.5) + self.pending[self.position];
                self.pending[self.position] = 0.0;
                self.triangle =
                    (1.0 - TRIANGLE_LEAK) * self.triangle + 4.0 * self.increment * square;
                self.position = (self.position + 1) % BLEP_SAMPLES;
                return self.triangle;
            }
        };
        let out = naive + self.pending[self.position];
        self.pending[self.position] = 0.0;
        self.position = (self.position + 1) % BLEP_SAMPLES;
        out
    }

    /// The naive square of pulse width `width` this sample, adding the corrections for its rise
    /// at the wrap and its fall at the width.
    fn square(&mut self, table: &MinBlep, wrapped: bool, width: f32) -> f32 {
        if wrapped && !self.high {
            self.high = true;
            self.step(table, self.phase, 2.0);
        }
        if self.high && self.phase >= width {
            self.high = false;
            self.step(table, self.phase - width, -2.0);
        }
        if self.high { 1.0 } else { -1.0 }
    }

    /// Adds the correction for a step of `height` that happened `since` cycles of phase before
    /// this sample.
    fn step(&mut self, table: &MinBlep, since: f32, height: f32) {
        let fraction = if self.increment > 0.0 {
            (since / self.increment).clamp(0.0, 1.0)
        } else {
            0.0
        };
        for k in 0..BLEP_SAMPLES {
            let slot = (self.position + k) % BLEP_SAMPLES;
            #[allow(clippy::cast_precision_loss)] // k < BLEP_SAMPLES
            let t = fraction + k as f32;
            self.pending[slot] += height * table.at(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::RealFft;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const RATE: f32 = 48_000.0;

    #[test]
    fn test_note_to_hz_is_equal_tempered_from_a440() {
        assert!((note_to_hz(69.0) - 440.0).abs() < 1e-3);
        assert!((note_to_hz(81.0) - 880.0).abs() < 1e-2);
        assert!((note_to_hz(60.0) - 261.6256).abs() < 1e-2);
        // A cent is 1/1200 of an octave.
        assert!((note_to_hz(69.01) / 440.0 - 2f32.powf(1.0 / 1200.0)).abs() < 1e-5);
    }

    #[test]
    fn test_min_blep_starts_near_minus_one_and_ends_at_zero() -> TestResult {
        let table = MinBlep::new()?;
        assert_eq!(table.residual.len(), BLEP_SAMPLES * OVERSAMPLING + 1);
        // Minimum phase: almost none of the step has happened at the step itself.
        assert!(table.at(0.0) < -0.9, "{}", table.at(0.0));
        assert!(table.at(BLEP_SAMPLES as f32).abs() < 1e-6);
        // Most of the step is done within a few samples.
        assert!(table.at(6.0).abs() < 0.1, "{}", table.at(6.0));
        Ok(())
    }

    /// `frames` samples of `wave` at `hz`, after `settle` discarded samples.
    fn render(wave: Wave, hz: f32, frames: usize, table: Option<&MinBlep>) -> Vec<f32> {
        let mut osc = Oscillator::new();
        osc.set_wave(wave);
        osc.set_frequency(hz, RATE);
        let mut phase = 0.0_f32;
        (0..frames + 4_096)
            .map(|_| match table {
                Some(table) => osc.next(table),
                None => {
                    phase = (phase + hz / RATE).fract();
                    2.0 * phase - 1.0
                }
            })
            .skip(4_096)
            .collect()
    }

    /// The share of `signal`'s Hann-windowed power not within two bins of a multiple of `hz`.
    fn inharmonic_share(signal: &[f32], hz: f32) -> Result<f32, Box<dyn std::error::Error>> {
        let size = signal.len();
        let mut fft = RealFft::new(size)?;
        let window = crate::fft::hann(size);
        let input: Vec<f32> = signal
            .iter()
            .zip(window.iter())
            .map(|(s, w)| s * w)
            .collect();
        let mut bins = vec![Complex::default(); fft.bins()];
        fft.forward(&input, &mut bins)?;
        let bin_hz = RATE / size as f32;
        let (mut harmonic, mut other) = (0.0_f64, 0.0_f64);
        for (k, bin) in bins.iter().enumerate().skip(1) {
            let f = k as f32 * bin_hz;
            let power = f64::from(bin.abs().powi(2));
            let nearest = (f / hz).round() * hz;
            if (f - nearest).abs() <= 2.5 * bin_hz && nearest > 0.0 {
                harmonic += power;
            } else {
                other += power;
            }
        }
        Ok((other / (harmonic + other)) as f32)
    }

    #[test]
    fn test_saw_at_5khz_aliases_far_less_than_naive() -> TestResult {
        let table = MinBlep::new()?;
        // Not a divisor of the rate, so aliases fall between the harmonics.
        let hz = 5_123.0;
        let naive = inharmonic_share(&render(Wave::Saw, hz, 16_384, None), hz)?;
        let blep = inharmonic_share(&render(Wave::Saw, hz, 16_384, Some(&table)), hz)?;
        assert!(naive > 1e-3, "naive {naive}");
        assert!(blep < naive / 100.0, "MinBLEP {blep} against naive {naive}");
        Ok(())
    }

    #[test]
    fn test_pulse_and_triangle_are_band_limited_and_in_range() -> TestResult {
        let table = MinBlep::new()?;
        let hz = 3_217.0;
        for wave in [Wave::Pulse, Wave::Triangle] {
            let signal = render(wave, hz, 16_384, Some(&table));
            assert!(inharmonic_share(&signal, hz)? < 1e-4, "{wave:?}");
            // A band-limited edge overshoots (Gibbs), more so in minimum phase.
            let peak = signal.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!((0.8..1.45).contains(&peak), "{wave:?} peak {peak}");
        }
        Ok(())
    }

    #[test]
    fn test_saw_has_the_right_period_and_level() -> TestResult {
        let table = MinBlep::new()?;
        // 480 Hz: exactly 100 samples a cycle.
        let signal = render(Wave::Saw, 480.0, 1_000, Some(&table));
        for n in 0..900 {
            assert!((signal[n] - signal[n + 100]).abs() < 1e-3, "sample {n}");
        }
        let mean = signal.iter().sum::<f32>() / signal.len() as f32;
        assert!(mean.abs() < 0.05, "mean {mean}");
        Ok(())
    }

    #[test]
    fn test_pulse_width_sets_duty_cycle() -> TestResult {
        let table = MinBlep::new()?;
        let mut osc = Oscillator::new();
        osc.set_wave(Wave::Pulse);
        osc.set_frequency(100.0, RATE);
        osc.set_pulse_width(0.25);
        let signal: Vec<f32> = (0..4_800).map(|_| osc.next(&table)).collect();
        let high = signal.iter().filter(|&&s| s > 0.0).count() as f32 / signal.len() as f32;
        assert!((high - 0.25).abs() < 0.01, "{high}");
        Ok(())
    }

    #[test]
    fn test_set_frequency_clamps_and_reset_returns_to_rest() -> TestResult {
        let table = MinBlep::new()?;
        let mut osc = Oscillator::new();
        osc.set_frequency(f32::NAN, RATE);
        assert_eq!(osc.next(&table), -1.0);
        osc.set_frequency(1e9, RATE);
        for _ in 0..100 {
            assert!(osc.next(&table).is_finite());
        }
        osc.reset();
        assert_eq!(osc.pending, [0.0; BLEP_SAMPLES]);
        assert_eq!(osc.phase, 0.0);
        Ok(())
    }
}
