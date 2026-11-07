//! One track channel's clips mixed into one signal (`docs/architecture/engine.md` "Clip
//! playback", ADR-0037, ADR-0046).
//!
//! Clips are painted bottom layer first. A clip replaces what is beneath it, except at each edge
//! where another clip lies beneath: there a crossfade of [`CROSSFADE_SECONDS`], inside the clip and
//! at most half its length, passes between the two with equal power. A clip's own fades are linear
//! in gain, and its gain applies throughout.
//!
//! Rendering runs on the disk-streaming reader thread and in offline render, not on the real-time
//! thread; it still allocates nothing once the renderer is made.

use std::f32::consts::FRAC_PI_2;

use crate::error::Error;

/// The length of the crossfade at each edge of an overlap, in seconds.
pub const CROSSFADE_SECONDS: f64 = 0.010;

/// Frames rendered from one clip at a time.
const SCRATCH_FRAMES: usize = 4096;

/// One clip as one channel plays it. All times are frames at the project rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelClip {
    /// The stored file it reads, as an index the [`ClipAudio`] understands.
    pub file: usize,
    /// Where in the file it starts.
    pub offset: u64,
    /// How many frames it plays.
    pub length: u64,
    /// Where on the timeline it starts.
    pub position: u64,
    /// Its linear gain.
    pub gain: f32,
    /// Its fade-in length.
    pub fade_in: u64,
    /// Its fade-out length.
    pub fade_out: u64,
}

impl ChannelClip {
    /// The timeline frame just after the clip.
    fn end(&self) -> u64 {
        self.position + self.length
    }

    /// Whether the clip plays timeline frame `frame`.
    fn covers(&self, frame: u64) -> bool {
        self.position <= frame && frame < self.end()
    }
}

/// Reads stored audio for the renderer.
pub trait ClipAudio {
    /// Fills `out` from frame `frame` of file `file`. The renderer asks only for frames the clip
    /// plays.
    ///
    /// # Errors
    ///
    /// Whatever stops the read; the renderer reports it as [`Error::ClipAudio`].
    fn read(
        &mut self,
        file: usize,
        frame: u64,
        out: &mut [f32],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// A clip with the crossfades its edges need.
#[derive(Debug, Clone, Copy)]
struct Placed {
    clip: ChannelClip,
    /// The crossfade at its start, in frames; 0 where nothing lies beneath.
    crossfade_in: u64,
    /// The crossfade at its end, in frames; 0 where nothing lies beneath.
    crossfade_out: u64,
}

/// Mixes one track channel's clips. Make one per track channel each time the clips change.
#[derive(Debug)]
pub struct TrackRenderer {
    clips: Vec<Placed>,
    frames: u64,
    scratch: Box<[f32]>,
}

impl TrackRenderer {
    /// A renderer of `clips`, bottom layer first, at `sample_rate`. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidClip`] for a clip of no frames, one that runs past the end of the
    /// timeline, a non-finite gain, or fades longer than the clip.
    pub fn new(clips: Vec<ChannelClip>, sample_rate: u32) -> Result<Self, Error> {
        for (index, clip) in clips.iter().enumerate() {
            let reason = if clip.length == 0 {
                Some("it has no frames")
            } else if clip.position.checked_add(clip.length).is_none()
                || clip.offset.checked_add(clip.length).is_none()
            {
                Some("it runs past the end of the timeline")
            } else if !clip.gain.is_finite() {
                Some("its gain is not finite")
            } else if clip.fade_in.saturating_add(clip.fade_out) > clip.length {
                Some("its fades are longer than it")
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(Error::InvalidClip { index, reason });
            }
        }
        let crossfade = (CROSSFADE_SECONDS * f64::from(sample_rate)).round() as u64;
        let placed = clips
            .iter()
            .enumerate()
            .map(|(index, &clip)| {
                let beneath = |frame: u64| clips[..index].iter().any(|c| c.covers(frame));
                let length = crossfade.min(clip.length / 2);
                Placed {
                    clip,
                    crossfade_in: if beneath(clip.position) { length } else { 0 },
                    crossfade_out: if beneath(clip.end() - 1) { length } else { 0 },
                }
            })
            .collect();
        Ok(TrackRenderer {
            frames: clips.iter().map(ChannelClip::end).max().unwrap_or(0),
            clips: placed,
            scratch: vec![0.0; SCRATCH_FRAMES].into_boxed_slice(),
        })
    }

    /// The timeline frame just after the last clip ends; silence follows.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Renders timeline frames `start..start + out.len()` into `out`.
    ///
    /// # Errors
    ///
    /// [`Error::ClipAudio`] if a read fails; `out` then holds silence where it was not rendered.
    pub fn render(
        &mut self,
        start: u64,
        out: &mut [f32],
        audio: &mut dyn ClipAudio,
    ) -> Result<(), Error> {
        out.fill(0.0);
        let end = start.saturating_add(out.len() as u64);
        for placed in &self.clips {
            let clip = &placed.clip;
            let from = start.max(clip.position);
            let to = end.min(clip.end());
            let mut frame = from;
            while frame < to {
                let n = ((to - frame) as usize).min(self.scratch.len());
                let samples = &mut self.scratch[..n];
                audio
                    .read(clip.file, clip.offset + (frame - clip.position), samples)
                    .map_err(Error::ClipAudio)?;
                let target = &mut out[(frame - start) as usize..][..n];
                for (i, (sample, mixed)) in samples.iter().zip(target).enumerate() {
                    let k = frame - clip.position + i as u64;
                    let (top, below) = weights(placed, k);
                    *mixed = top * clip.gain * fade(clip, k) * sample + below * *mixed;
                }
                frame += n as u64;
            }
        }
        Ok(())
    }
}

/// The clip's fade gain at its frame `k`: linear from 0 over the fade-in, and to 0 over the
/// fade-out.
fn fade(clip: &ChannelClip, k: u64) -> f32 {
    let from_end = clip.length - 1 - k;
    let mut gain = 1.0;
    if k < clip.fade_in {
        gain *= k as f32 / clip.fade_in as f32;
    }
    if from_end < clip.fade_out {
        gain *= from_end as f32 / clip.fade_out as f32;
    }
    gain
}

/// The equal-power weights of the clip and of what lies beneath it at its frame `k`.
fn weights(placed: &Placed, k: u64) -> (f32, f32) {
    let from_end = placed.clip.length - 1 - k;
    let x = if k < placed.crossfade_in {
        (k as f32 + 0.5) / placed.crossfade_in as f32
    } else if from_end < placed.crossfade_out {
        (from_end as f32 + 0.5) / placed.crossfade_out as f32
    } else {
        return (1.0, 0.0);
    };
    ((x * FRAC_PI_2).sin(), (x * FRAC_PI_2).cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(position: u64, length: u64) -> ChannelClip {
        ChannelClip {
            file: 0,
            offset: 0,
            length,
            position,
            gain: 1.0,
            fade_in: 0,
            fade_out: 0,
        }
    }

    #[test]
    fn test_new_crossfades_only_edges_over_another_clip() -> Result<(), Error> {
        // 10 ms at 1 kHz is 10 frames.
        let renderer = TrackRenderer::new(vec![clip(0, 100), clip(50, 100), clip(500, 8)], 1000)?;
        let edges: Vec<(u64, u64)> = renderer
            .clips
            .iter()
            .map(|p| (p.crossfade_in, p.crossfade_out))
            .collect();
        assert_eq!(edges, vec![(0, 0), (10, 0), (0, 0)]);
        assert_eq!(renderer.frames(), 508);
        Ok(())
    }

    #[test]
    fn test_new_shortens_crossfade_to_half_the_clip() -> Result<(), Error> {
        let renderer = TrackRenderer::new(vec![clip(0, 100), clip(20, 6)], 1000)?;
        assert_eq!(renderer.clips[1].crossfade_in, 3);
        assert_eq!(renderer.clips[1].crossfade_out, 3);
        Ok(())
    }

    #[test]
    fn test_new_rejects_invalid_clips() {
        let mut long_fades = clip(0, 10);
        long_fades.fade_in = 6;
        long_fades.fade_out = 5;
        let mut bad_gain = clip(0, 10);
        bad_gain.gain = f32::NAN;
        for bad in [clip(0, 0), clip(u64::MAX - 1, 5), long_fades, bad_gain] {
            assert!(matches!(
                TrackRenderer::new(vec![clip(0, 1), bad], 1000),
                Err(Error::InvalidClip { index: 1, .. })
            ));
        }
    }

    #[test]
    fn test_weights_are_equal_power() {
        let placed = Placed {
            clip: clip(0, 100),
            crossfade_in: 10,
            crossfade_out: 10,
        };
        for k in 0..100 {
            let (top, below) = weights(&placed, k);
            assert!((top * top + below * below - 1.0).abs() < 1e-6);
        }
        assert_eq!(weights(&placed, 50), (1.0, 0.0));
    }

    #[test]
    fn test_fade_is_linear_to_and_from_zero() {
        let mut c = clip(0, 10);
        c.fade_in = 4;
        c.fade_out = 2;
        let gains: Vec<f32> = (0..10).map(|k| fade(&c, k)).collect();
        assert_eq!(
            gains,
            vec![0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0, 0.5, 0.0]
        );
    }
}
