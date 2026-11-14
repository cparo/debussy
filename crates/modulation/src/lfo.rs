//! [`Lfo`]: a phase-accumulator low-frequency oscillator (`docs/architecture/modulation.md`
//! "LFO").

use dsp::Noise;

use crate::shape::{Shape, wrap};

/// A low-frequency oscillator: a phase clock in cycles, advanced each frame, read through a
/// [`Shape`]. Readers can ask for the value at a phase offset from the clock, so left and right
/// channels, or several voices, stay locked to one clock. Never allocates.
#[derive(Debug, Clone, PartialEq)]
pub struct Lfo {
    sample_rate: f64,
    /// The frequency, in hertz: finite, from 0 to half the sample rate.
    frequency: f64,
    /// The phase step per frame, in cycles.
    increment: f64,
    /// The phase, `0 ≤ phase < 1`.
    phase: f64,
    /// How many times the phase has wrapped since the last reset.
    cycle: u64,
    shape: Shape,
    /// What the random values are drawn from.
    seed: u64,
}

impl Lfo {
    /// A sine LFO at `sample_rate` (raised to 1 if 0), at phase 0 and 0 Hz, its random values
    /// drawn from `seed`.
    pub fn new(sample_rate: u32, seed: u64) -> Self {
        Lfo {
            sample_rate: f64::from(sample_rate.max(1)),
            frequency: 0.0,
            increment: 0.0,
            phase: 0.0,
            cycle: 0,
            shape: Shape::Sine,
            seed,
        }
    }

    /// Sets the frequency in hertz. A negative or NaN frequency is 0; one above half the sample
    /// rate is half the sample rate. The phase continues from where it is.
    pub fn set_frequency(&mut self, hz: f64) {
        let nyquist = self.sample_rate / 2.0;
        self.frequency = if hz.is_nan() {
            0.0
        } else {
            hz.clamp(0.0, nyquist)
        };
        self.increment = self.frequency / self.sample_rate;
    }

    /// Sets the period in frames, as a tempo-synced LFO does; a period that is not positive (or
    /// is NaN) stops the clock.
    pub fn set_period_frames(&mut self, frames: f64) {
        if frames > 0.0 {
            // An infinite period gives 0 Hz.
            self.set_frequency(self.sample_rate / frames);
        } else {
            self.set_frequency(0.0);
        }
    }

    /// The frequency, in hertz.
    pub fn frequency(&self) -> f64 {
        self.frequency
    }

    /// Sets the shape; the phase continues.
    pub fn set_shape(&mut self, shape: Shape) {
        self.shape = shape;
    }

    /// The shape.
    pub fn shape(&self) -> Shape {
        self.shape
    }

    /// Moves the clock to `phase` (in cycles; its whole part is ignored) and restarts the cycle
    /// count, so the random values start again from the seed's first.
    pub fn reset(&mut self, phase: f64) {
        self.phase = wrap(phase);
        self.cycle = 0;
    }

    /// The phase, `0 ≤ phase < 1`.
    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Advances the clock by one frame.
    pub fn advance(&mut self) {
        let next = self.phase + self.increment;
        if next >= 1.0 {
            // An increment is at most ½, so the phase wraps at most once a frame.
            self.cycle = self.cycle.wrapping_add(1);
            self.phase = wrap(next);
        } else {
            self.phase = next;
        }
    }

    /// The value at the clock's phase, in stream 0.
    pub fn value(&self) -> f32 {
        self.value_at_offset(0.0, 0)
    }

    /// The value at `offset` cycles from the clock's phase (its whole part ignored; NaN reads as
    /// 0), from −1 to 1. A random shape takes the value of the offset phase's cycle in `stream`;
    /// different streams give uncorrelated values.
    pub fn value_at_offset(&self, offset: f64, stream: u64) -> f32 {
        let offset = wrap(offset);
        let shifted = self.phase + offset;
        let held = if self.shape.is_random() {
            let cycle = if shifted >= 1.0 {
                self.cycle.wrapping_add(1)
            } else {
                self.cycle
            };
            random(self.seed, cycle, stream)
        } else {
            0.0
        };
        self.shape.value(shifted, held)
    }

    /// The value at the clock's phase, then advances the clock: one frame of output.
    pub fn next_value(&mut self) -> f32 {
        let value = self.value();
        self.advance();
        value
    }
}

/// The random value, from −1 to 1, of `cycle` in `stream` for `seed`: a pure function, so any
/// reader can draw it without state.
fn random(seed: u64, cycle: u64, stream: u64) -> f32 {
    // Each of the three is spread by a different odd constant before `Noise::new` mixes them
    // through SplitMix64's finaliser.
    let key = seed
        ^ cycle.wrapping_mul(0xD1B5_4A32_D192_ED03)
        ^ stream.wrapping_mul(0x8CB9_2BA7_2F3D_8DD7);
    2.0 * Noise::new(key).uniform()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    #[test]
    fn test_next_completes_one_cycle_per_period() {
        let mut lfo = Lfo::new(RATE, 1);
        lfo.set_frequency(1.0);
        lfo.set_shape(Shape::SawUp);
        assert_eq!(lfo.next_value(), -1.0);
        for _ in 1..48_000 {
            lfo.next_value();
        }
        // Back at phase 0, give or take rounding of 48 000 additions.
        assert!(
            lfo.phase() < 1e-9 || lfo.phase() > 1.0 - 1e-9,
            "{}",
            lfo.phase()
        );
        assert_eq!(lfo.cycle, if lfo.phase() < 0.5 { 1 } else { 0 });
    }

    #[test]
    fn test_set_frequency_clamps_bad_values() {
        let mut lfo = Lfo::new(RATE, 0);
        lfo.set_frequency(f64::NAN);
        assert_eq!(lfo.frequency(), 0.0);
        lfo.set_frequency(-3.0);
        assert_eq!(lfo.frequency(), 0.0);
        lfo.set_frequency(1e9);
        assert_eq!(lfo.frequency(), 24_000.0);
        lfo.set_frequency(f64::INFINITY);
        assert_eq!(lfo.frequency(), 24_000.0);
    }

    #[test]
    fn test_advance_at_zero_frequency_holds_phase() {
        let mut lfo = Lfo::new(RATE, 0);
        lfo.reset(0.3);
        for _ in 0..1000 {
            lfo.advance();
        }
        assert_eq!(lfo.phase(), 0.3);
    }

    #[test]
    fn test_advance_at_nyquist_wraps_every_other_frame() {
        let mut lfo = Lfo::new(RATE, 0);
        lfo.set_frequency(24_000.0);
        for _ in 0..10 {
            lfo.advance();
            assert!((0.0..1.0).contains(&lfo.phase()));
        }
        assert_eq!(lfo.cycle, 5);
    }

    #[test]
    fn test_set_period_frames_sets_frequency() {
        let mut lfo = Lfo::new(RATE, 0);
        lfo.set_period_frames(96_000.0);
        assert_eq!(lfo.frequency(), 0.5);
        lfo.set_period_frames(0.0);
        assert_eq!(lfo.frequency(), 0.0);
        lfo.set_period_frames(f64::NAN);
        assert_eq!(lfo.frequency(), 0.0);
    }

    #[test]
    fn test_value_at_offset_reads_a_second_phase() {
        let mut lfo = Lfo::new(RATE, 0);
        lfo.set_shape(Shape::Triangle);
        lfo.reset(0.0);
        assert_eq!(lfo.value_at_offset(0.25, 0), 1.0);
        assert_eq!(lfo.value_at_offset(0.75, 0), -1.0);
        assert_eq!(lfo.value_at_offset(1.25, 0), 1.0);
        assert_eq!(lfo.value_at_offset(f64::NAN, 0), lfo.value());
        lfo.reset(0.5);
        assert_eq!(lfo.value_at_offset(0.75, 0), 1.0);
    }

    #[test]
    fn test_sample_and_hold_holds_within_a_cycle_and_changes_on_wrap() {
        let mut lfo = Lfo::new(RATE, 9);
        lfo.set_shape(Shape::SampleAndHold);
        lfo.set_frequency(93.75);
        let first = lfo.value();
        for _ in 0..511 {
            assert_eq!(lfo.next_value(), first);
        }
        // 512 frames is one cycle at 93.75 Hz, a step of exactly 1/512.
        lfo.advance();
        assert_ne!(lfo.value(), first);
    }

    #[test]
    fn test_sample_and_hold_is_deterministic_per_seed() {
        let run = |seed| {
            let mut lfo = Lfo::new(RATE, seed);
            lfo.set_shape(Shape::SampleAndHold);
            lfo.set_frequency(1000.0);
            (0..4800).map(|_| lfo.next_value()).collect::<Vec<_>>()
        };
        assert_eq!(run(3), run(3));
        assert_ne!(run(3), run(4));
        assert!(run(3).iter().all(|v| (-1.0..=1.0).contains(v)));
    }

    #[test]
    fn test_sample_and_hold_streams_are_uncorrelated() {
        // The correlation of 4000 cycles' values in streams 0 and 1 is near 0.
        let (mut sum, mut sum_a, mut sum_b) = (0.0_f64, 0.0_f64, 0.0_f64);
        let count = 4000_u64;
        for cycle in 0..count {
            let a = f64::from(random(5, cycle, 0));
            let b = f64::from(random(5, cycle, 1));
            sum += a * b;
            sum_a += a * a;
            sum_b += b * b;
        }
        let correlation = sum / (sum_a * sum_b).sqrt();
        assert!(correlation.abs() < 0.06, "{correlation}");
    }

    #[test]
    fn test_sample_and_hold_offset_read_takes_the_offset_cycle() {
        let mut lfo = Lfo::new(RATE, 2);
        lfo.set_shape(Shape::SampleAndHold);
        lfo.reset(0.9);
        // The offset phase is 1.1: the next cycle's value.
        assert_eq!(lfo.value_at_offset(0.2, 0), random(2, 1, 0));
        assert_eq!(lfo.value_at_offset(0.05, 0), random(2, 0, 0));
    }

    #[test]
    fn test_reset_restarts_random_values() {
        let mut lfo = Lfo::new(RATE, 4);
        lfo.set_shape(Shape::SampleAndHold);
        let first = lfo.value();
        lfo.set_frequency(1000.0);
        for _ in 0..1000 {
            lfo.advance();
        }
        lfo.reset(0.0);
        assert_eq!(lfo.value(), first);
    }
}
