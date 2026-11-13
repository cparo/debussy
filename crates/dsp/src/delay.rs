//! [`TapDelay`]: a delay line read at any whole-frame tap up to a fixed longest delay, or at a
//! fractional one by linear interpolation (`docs/architecture/delay-and-reverb.md` "Delay" and
//! "Reverb").

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
