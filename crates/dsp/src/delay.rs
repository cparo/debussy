//! [`TapDelay`]: a delay line read at any whole-frame tap up to a fixed longest delay, or at a
//! fractional one by linear or cubic Hermite interpolation
//! (`docs/architecture/delay-and-reverb.md` "Delay" and "Reverb",
//! `docs/architecture/modulation.md` "Modulated delay").

/// A circular buffer of past samples, read at a tap of a whole number of frames. Allocates only
/// in [`TapDelay::new`].
#[derive(Debug, Clone, PartialEq)]
pub struct TapDelay {
    buffer: Vec<f32>,
    /// Where the next sample is written.
    position: usize,
}

impl TapDelay {
    /// A delay line holding silence, readable at taps of 1 to `max_tap` frames. Allocates
    /// `max_tap + 1` samples; a `max_tap` of 0 is raised to 1.
    pub fn new(max_tap: usize) -> Self {
        TapDelay {
            buffer: vec![0.0; max_tap.max(1) + 1],
            position: 0,
        }
    }

    /// The longest tap, in frames.
    pub fn max_tap(&self) -> usize {
        self.buffer.len() - 1
    }

    /// The sample written `tap` frames ago, `tap` clamped to `1..=max_tap`: a tap of 1 is the
    /// latest sample written.
    pub fn read(&self, tap: usize) -> f32 {
        let tap = tap.clamp(1, self.max_tap());
        let len = self.buffer.len();
        // `position` and `tap` are both below `len`, so this neither overflows nor underflows.
        let index = (self.position + len - tap) % len;
        self.buffer.get(index).copied().unwrap_or(0.0)
    }

    /// The sample written `frames` frames ago, linearly interpolated between the two whole taps
    /// either side; `frames` is clamped to `1..=max_tap`, and NaN reads tap 1.
    pub fn read_fractional(&self, frames: f32) -> f32 {
        #[allow(clippy::cast_precision_loss)] // delay lines are far shorter than 2^24 frames
        let max = self.max_tap() as f32;
        let frames = if frames.is_nan() {
            1.0
        } else {
            frames.clamp(1.0, max)
        };
        let whole = frames.floor();
        let fraction = frames - whole;
        // Held within `1..=max_tap` above, so the cast is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let tap = whole as usize;
        let near = self.read(tap);
        if fraction == 0.0 {
            near
        } else {
            near + fraction * (self.read(tap + 1) - near)
        }
    }

    /// The sample written `frames` frames ago, by 4-point, 3rd-order Hermite (Catmull-Rom)
    /// interpolation across the whole taps `⌊frames⌋ − 1` to `⌊frames⌋ + 2`; `frames` is clamped
    /// to `2..=max_tap − 1` so all four exist, and NaN reads tap 2
    /// (`docs/architecture/modulation.md` "Modulated delay", ADR-0120).
    ///
    /// With `y₋₁, y₀, y₁, y₂` the taps from the newest to the oldest and `x` the fraction:
    /// `c₁ = (y₁ − y₋₁)/2`, `c₂ = y₋₁ − 2.5·y₀ + 2·y₁ − y₂/2`,
    /// `c₃ = (y₂ − y₋₁)/2 + 1.5·(y₀ − y₁)`, and the value is `((c₃·x + c₂)·x + c₁)·x + y₀`
    /// (the "4-point, 3rd-order Hermite, x-form" of Niemitalo, *Polynomial Interpolators for
    /// High-Quality Resampling of Oversampled Audio*, 2001). It passes every tap and reproduces
    /// any quadratic exactly.
    pub fn read_cubic(&self, frames: f32) -> f32 {
        #[allow(clippy::cast_precision_loss)] // delay lines are far shorter than 2^24 frames
        let max = (self.max_tap() as f32 - 1.0).max(2.0);
        let frames = if frames.is_nan() {
            2.0
        } else {
            frames.clamp(2.0, max)
        };
        let whole = frames.floor();
        let x = frames - whole;
        // Held within `2..=max_tap - 1` above, so the cast is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let tap = whole as usize;
        let y0 = self.read(tap);
        if x == 0.0 {
            return y0;
        }
        let newer = self.read(tap - 1);
        let y1 = self.read(tap + 1);
        let y2 = self.read(tap + 2);
        let c1 = 0.5 * (y1 - newer);
        let c2 = newer - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
        let c3 = 0.5 * (y2 - newer) + 1.5 * (y0 - y1);
        ((c3 * x + c2) * x + c1) * x + y0
    }

    /// Writes the next sample.
    pub fn write(&mut self, sample: f32) {
        if let Some(slot) = self.buffer.get_mut(self.position) {
            *slot = sample;
        }
        self.position += 1;
        if self.position == self.buffer.len() {
            self.position = 0;
        }
    }

    /// Fills the line with silence.
    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_returns_the_sample_written_tap_frames_ago() {
        let mut line = TapDelay::new(5);
        for x in 1..=8 {
            #[allow(clippy::cast_precision_loss)] // small whole numbers
            line.write(x as f32);
        }
        // Written 1..=8; the latest is 8.
        assert_eq!(line.read(1), 8.0);
        assert_eq!(line.read(3), 6.0);
        assert_eq!(line.read(5), 4.0);
    }

    #[test]
    fn test_read_clamps_the_tap() {
        let mut line = TapDelay::new(3);
        for x in [1.0, 2.0, 3.0, 4.0] {
            line.write(x);
        }
        assert_eq!(line.max_tap(), 3);
        assert_eq!(line.read(0), 4.0);
        assert_eq!(line.read(100), 2.0);
    }

    #[test]
    fn test_read_fractional_interpolates_between_taps() {
        let mut line = TapDelay::new(4);
        for x in [10.0, 20.0, 30.0, 40.0] {
            line.write(x);
        }
        assert_eq!(line.read_fractional(1.0), 40.0);
        assert_eq!(line.read_fractional(2.0), 30.0);
        assert!((line.read_fractional(1.25) - 37.5).abs() < 1e-5);
        assert!((line.read_fractional(3.5) - 15.0).abs() < 1e-5);
        // Clamped at both ends, and NaN reads the latest sample.
        assert_eq!(line.read_fractional(0.2), 40.0);
        assert_eq!(line.read_fractional(9.0), 10.0);
        assert_eq!(line.read_fractional(f32::NAN), 40.0);
    }

    /// A line of `max_tap` holding `f(tap)` at each tap from 1 to `max_tap`.
    fn line_of(max_tap: usize, f: impl Fn(f32) -> f32) -> TapDelay {
        let mut line = TapDelay::new(max_tap);
        for tap in (1..=max_tap).rev() {
            #[allow(clippy::cast_precision_loss)] // small whole numbers
            line.write(f(tap as f32));
        }
        line
    }

    #[test]
    fn test_read_cubic_is_exact_on_whole_taps() {
        let line = line_of(8, |t| t * t * t - 3.0 * t);
        for tap in 2..=7_u8 {
            let t = f32::from(tap);
            assert_eq!(line.read_cubic(t), line.read(usize::from(tap)));
        }
    }

    #[test]
    fn test_read_cubic_reproduces_linear_and_quadratic() {
        let ramp = line_of(10, |t| 3.0 * t - 1.0);
        let parabola = line_of(10, |t| 0.5 * t * t - 2.0 * t + 4.0);
        for f in [2.25_f32, 3.5, 4.75, 6.1, 8.9] {
            assert!((ramp.read_cubic(f) - (3.0 * f - 1.0)).abs() < 1e-4, "{f}");
            let want = 0.5 * f * f - 2.0 * f + 4.0;
            assert!((parabola.read_cubic(f) - want).abs() < 1e-4, "{f}");
        }
    }

    #[test]
    fn test_read_cubic_clamps() {
        let line = line_of(6, |t| 10.0 * t);
        assert_eq!(line.read_cubic(0.0), 20.0);
        assert_eq!(line.read_cubic(1.5), 20.0);
        assert_eq!(line.read_cubic(f32::NAN), 20.0);
        assert_eq!(line.read_cubic(100.0), 50.0);
        assert_eq!(line.read_cubic(f32::INFINITY), 50.0);
        // A line too short for four taps still reads without panicking.
        let short = line_of(2, |t| t);
        assert!(short.read_cubic(1.7).is_finite());
    }

    #[test]
    fn test_read_cubic_loses_less_than_linear_at_half_a_frame() {
        // A sine at a quarter of the rate, read half a frame between taps: linear interpolation
        // loses cos(π/4) ≈ 0.707 of its amplitude, Hermite far less.
        let line = line_of(64, |t| (std::f32::consts::FRAC_PI_2 * t + 0.3).sin());
        let (mut linear, mut cubic) = (0.0_f32, 0.0_f32);
        for i in 0..40_u8 {
            let f = f32::from(i) + 10.5;
            linear = linear.max(line.read_fractional(f).abs());
            cubic = cubic.max(line.read_cubic(f).abs());
        }
        // Their gains there are 0.707 and 0.884, whatever the phase.
        assert!(cubic > 1.2 * linear, "{cubic} vs {linear}");
    }

    #[test]
    fn test_new_holds_silence_and_clear_restores_it() {
        let mut line = TapDelay::new(0);
        assert_eq!(line.max_tap(), 1);
        assert_eq!(line.read(1), 0.0);
        line.write(1.0);
        assert_eq!(line.read(1), 1.0);
        line.clear();
        assert_eq!(line.read(1), 0.0);
    }
}
