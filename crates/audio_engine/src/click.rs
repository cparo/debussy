//! [`ClickTrack`]: the metronome click, mixed into the output after the plan
//! (`docs/architecture/recording.md` "Click track").
//!
//! The click is stateless: each output frame sums the sound started at every beat whose start
//! lies within the sound's length before that frame. So what a frame holds depends only on its
//! timeline position, never on block sizes, seeks or what played before.

use std::sync::Arc;

use crate::error::Error;

/// The gain of beats other than the downbeat: −6 dB.
pub const OFFBEAT_GAIN: f32 = 0.501_187_2;

/// A click sound on a beat grid. Cloning shares the sound.
#[derive(Debug, Clone)]
pub struct ClickTrack {
    /// The sound, mono at the engine's rate.
    sound: Arc<[f32]>,
    /// Frames per beat; beat `n` starts on frame `round(n × frames_per_beat)`.
    frames_per_beat: f64,
    /// Beats per bar; beats `0, beats_per_bar, …` are downbeats.
    beats_per_bar: u32,
}

impl ClickTrack {
    /// A click playing `sound` (mono, at the engine's rate) on every beat of a grid of
    /// `frames_per_beat` frames per beat, accenting every `beats_per_bar`th from beat 0, the
    /// downbeat on frame 0. Allocates nothing; the sound is shared.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidClick`] for an empty sound, fewer than one frame per beat, or no beats in
    /// a bar.
    pub fn new(
        sound: Arc<[f32]>,
        frames_per_beat: f64,
        beats_per_bar: u32,
    ) -> Result<ClickTrack, Error> {
        if sound.is_empty() {
            return Err(Error::InvalidClick("the sound is empty"));
        }
        if !frames_per_beat.is_finite() || frames_per_beat < 1.0 {
            return Err(Error::InvalidClick("a beat must be at least one frame"));
        }
        if beats_per_bar == 0 {
            return Err(Error::InvalidClick("a bar must have at least one beat"));
        }
        Ok(ClickTrack {
            sound,
            frames_per_beat,
            beats_per_bar,
        })
    }

    /// The frame beat `index` starts on.
    fn beat_frame(&self, index: i64) -> i64 {
        (index as f64 * self.frames_per_beat).round() as i64
    }

    /// Adds the click for timeline frames `start..start + n` to `left` and `right`, `n` being the
    /// shorter's length, at `gain`; `start` may be negative, before frame 0. Real-time safe.
    pub fn mix(&self, start: i64, gain: f32, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        let end = start.saturating_add(frames as i64);
        let length = self.sound.len() as i64;
        // One before the estimate, which rounding can put one beat late.
        let mut index =
            ((start.saturating_sub(length)) as f64 / self.frames_per_beat).floor() as i64 - 1;
        loop {
            let beat = self.beat_frame(index);
            if beat >= end {
                break;
            }
            let from = beat.max(start);
            let to = beat.saturating_add(length).min(end);
            if from < to {
                let accent = index.rem_euclid(i64::from(self.beats_per_bar)) == 0;
                let g = if accent { gain } else { gain * OFFBEAT_GAIN };
                // Within the sound and the block: `from..to` lies in both.
                let sound = &self.sound[(from - beat) as usize..(to - beat) as usize];
                let at = (from - start) as usize;
                let outputs = left[at..].iter_mut().zip(right[at..].iter_mut());
                for (&s, (l, r)) in sound.iter().zip(outputs) {
                    *l += s * g;
                    *r += s * g;
                }
            }
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn click(sound: &[f32], frames_per_beat: f64, beats_per_bar: u32) -> Result<ClickTrack, Error> {
        ClickTrack::new(sound.into(), frames_per_beat, beats_per_bar)
    }

    /// The click for frames `start..start + frames`, mixed in blocks of `block`.
    fn mixed(click: &ClickTrack, start: i64, frames: usize, block: usize) -> Vec<f32> {
        let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
        for (i, (l, r)) in left
            .chunks_mut(block)
            .zip(right.chunks_mut(block))
            .enumerate()
        {
            click.mix(start + (i * block) as i64, 1.0, l, r);
        }
        assert_eq!(left, right);
        left
    }

    #[test]
    fn test_new_refuses_empty_sound_and_bad_grid() {
        let empty: Arc<[f32]> = Arc::from(Vec::new());
        assert!(ClickTrack::new(empty, 100.0, 4).is_err());
        let sound: Arc<[f32]> = Arc::from(vec![1.0]);
        assert!(ClickTrack::new(Arc::clone(&sound), 0.5, 4).is_err());
        assert!(ClickTrack::new(Arc::clone(&sound), f64::NAN, 4).is_err());
        assert!(ClickTrack::new(sound, 100.0, 0).is_err());
    }

    #[test]
    fn test_mix_places_beats_and_accents_downbeats() -> TestResult {
        let c = click(&[1.0, 0.5], 10.0, 3)?;
        let out = mixed(&c, 0, 32, 32);
        assert_eq!((out[0], out[1], out[2]), (1.0, 0.5, 0.0));
        assert_eq!((out[10], out[11]), (OFFBEAT_GAIN, 0.5 * OFFBEAT_GAIN));
        assert_eq!(out[20], OFFBEAT_GAIN);
        assert_eq!(out[30], 1.0);
        assert_eq!(out.iter().filter(|&&s| s != 0.0).count(), 8);
        Ok(())
    }

    #[test]
    fn test_mix_is_the_same_for_any_block_size_and_start() -> TestResult {
        // A sound longer than a beat, so beats overlap, at a fractional beat length.
        let sound: Vec<f32> = (0..250)
            .map(|i| ((i * 7) % 13) as f32 / 13.0 - 0.5)
            .collect();
        let c = click(&sound, 97.3, 4)?;
        let whole = mixed(&c, -1_000, 4_000, 4_000);
        for block in [1, 7, 64, 128, 333, 512] {
            assert_eq!(mixed(&c, -1_000, 4_000, block), whole, "block {block}");
        }
        // Starting elsewhere, as after a seek, gives the same frames.
        let later = mixed(&c, 1_234, 1_000, 128);
        assert_eq!(later, whole[2_234..3_234]);
        Ok(())
    }

    #[test]
    fn test_mix_before_zero_counts_bars_back_from_frame_zero() -> TestResult {
        let c = click(&[1.0], 10.0, 4)?;
        let out = mixed(&c, -40, 41, 41);
        // Frame −40 is the downbeat of bar −1; frame 0 that of bar 0.
        assert_eq!(out[0], 1.0);
        assert_eq!(out[10], OFFBEAT_GAIN);
        assert_eq!(out[40], 1.0);
        Ok(())
    }
}
