//! [`TapDelay`]: a delay line read at any whole-frame tap up to a fixed longest delay
//! (`docs/architecture/delay-and-reverb.md` "Delay").

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
