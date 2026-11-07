//! Delay lines: feedback delays (`docs/architecture/signal-graph.md` "Feedback loops") and the
//! compensation delays on connections ("Delay compensation").

use audio_buffers::Block;

/// The shortest feedback delay, in seconds (ADR-0031).
pub const MIN_FEEDBACK_DELAY: f64 = 0.001;

/// The longest feedback delay, in seconds.
pub const MAX_FEEDBACK_DELAY: f64 = 10.0;

/// The length in frames of a feedback delay of `seconds` at `sample_rate`: raised to
/// [`MIN_FEEDBACK_DELAY`], then rounded up to whole frames (45 frames for 1 ms at 44.1 kHz).
/// `seconds` must be finite and at most [`MAX_FEEDBACK_DELAY`].
pub fn feedback_delay_frames(seconds: f64, sample_rate: u32) -> usize {
    let exact = seconds.max(MIN_FEEDBACK_DELAY) * f64::from(sample_rate);
    // A product such as 0.001 × 48 000 can land a hair above a whole number; that is not a frame.
    let frames = (exact - 1e-6).ceil().max(1.0);
    // At most 10 s at a `u32` rate, so it fits.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = frames as usize;
    frames
}

/// A fixed delay of `length` frames on every channel of a signal, as a ring per channel.
#[derive(Debug)]
pub(crate) struct DelayLine {
    /// `channels` rings of `length` samples, one after another.
    ring: Vec<f32>,
    length: usize,
    /// The next slot to read and then overwrite, the same in every ring.
    position: usize,
}

impl DelayLine {
    /// A silent line of `length` frames over `channels` channels. Allocates.
    pub(crate) fn new(channels: usize, length: usize) -> Self {
        DelayLine {
            ring: vec![0.0; channels * length],
            length,
            position: 0,
        }
    }

    /// The delay, in frames.
    pub(crate) fn length(&self) -> usize {
        self.length
    }

    /// Gives `channel`'s sample from `length` frames ago and stores `sample` in its place.
    /// [`DelayLine::advance`] moves on once every channel has been exchanged.
    pub(crate) fn exchange(&mut self, channel: usize, sample: f32) -> f32 {
        if self.length == 0 {
            return sample;
        }
        let slot = &mut self.ring[channel * self.length + self.position];
        std::mem::replace(slot, sample)
    }

    /// Moves on one frame.
    pub(crate) fn advance(&mut self) {
        if self.length > 0 {
            self.position = (self.position + 1) % self.length;
        }
    }

    /// Writes the next `frames` frames of delayed signal into `output` from `start`, without
    /// taking input; at most `length` frames. Pairs with [`DelayLine::absorb`].
    pub(crate) fn emit(&self, output: &mut Block, start: usize, frames: usize) {
        for channel in 0..output.channels() {
            let Some(out) = output.channel_mut(channel) else {
                continue;
            };
            let ring = &self.ring[channel * self.length..(channel + 1) * self.length];
            for (offset, sample) in out[start..start + frames].iter_mut().enumerate() {
                *sample = ring[(self.position + offset) % self.length];
            }
        }
    }

    /// Stores `frames` frames of `input` from `start`, in the slots [`DelayLine::emit`] just read,
    /// and moves on.
    pub(crate) fn absorb(&mut self, input: &Block, start: usize, frames: usize) {
        for channel in 0..input.channels() {
            let Some(samples) = input.channel(channel) else {
                continue;
            };
            let ring = &mut self.ring[channel * self.length..(channel + 1) * self.length];
            for (offset, &sample) in samples[start..start + frames].iter().enumerate() {
                ring[(self.position + offset) % self.length] = sample;
            }
        }
        self.position = (self.position + frames) % self.length;
    }

    /// Takes `other`'s contents if it has the same shape, leaving this line's in `other`.
    pub(crate) fn take_state(&mut self, other: &mut DelayLine) {
        if self.length == other.length && self.ring.len() == other.ring.len() {
            std::mem::swap(self, other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_buffers::Layout;

    #[test]
    fn test_feedback_delay_frames_rounds_one_ms_up() {
        let frames: Vec<usize> = [44_100, 48_000, 88_200, 96_000]
            .iter()
            .map(|&rate| feedback_delay_frames(0.001, rate))
            .collect();
        assert_eq!(frames, [45, 48, 89, 96]);
    }

    #[test]
    fn test_feedback_delay_frames_raises_short_delays() {
        assert_eq!(feedback_delay_frames(0.0, 48_000), 48);
        assert_eq!(feedback_delay_frames(-1.0, 48_000), 48);
        assert_eq!(feedback_delay_frames(0.0101, 48_000), 485);
    }

    #[test]
    fn test_exchange_delays_by_length() {
        let mut line = DelayLine::new(1, 3);
        let out: Vec<f32> = (1..=6)
            .map(|i| {
                let y = line.exchange(0, i as f32);
                line.advance();
                y
            })
            .collect();
        assert_eq!(out, [0.0, 0.0, 0.0, 1.0, 2.0, 3.0]);
        let mut through = DelayLine::new(2, 0);
        assert_eq!(through.exchange(1, 4.0), 4.0);
        through.advance();
    }

    #[test]
    fn test_emit_then_absorb_matches_exchange() -> Result<(), audio_buffers::Error> {
        let mut line = DelayLine::new(1, 40);
        let mut input = Block::new(Layout::Mono, 64)?;
        let mut output = Block::new(Layout::Mono, 64)?;
        let mut produced = Vec::new();
        for chunk in 0..4 {
            if let Some(samples) = input.channel_mut(0) {
                for (i, s) in samples.iter_mut().enumerate() {
                    *s = (chunk * 64 + i) as f32;
                }
            }
            for start in [0, 40] {
                let frames = if start == 0 { 40 } else { 24 };
                line.emit(&mut output, start, frames);
                line.absorb(&input, start, frames);
            }
            produced.extend_from_slice(output.channel(0).unwrap_or_default());
        }
        let expected: Vec<f32> = (0..256)
            .map(|i| if i < 40 { 0.0 } else { (i - 40) as f32 })
            .collect();
        assert_eq!(produced, expected);
        Ok(())
    }
}
