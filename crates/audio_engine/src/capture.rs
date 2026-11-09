//! Recording: armed tracks, the device input they read, and where takes go
//! (`docs/architecture/recording.md` "Capture", "Input monitoring", "Take placement").

use signal_graph::NodeId;

/// The most device input channels the engine reads; later ones are silent.
pub const MAX_INPUTS: usize = 64;

/// Where one channel of a take goes. Normally a disk recorder's real-time end.
pub trait TakeSink: Send {
    /// Appends `samples` to the take and returns how many it kept; the rest were dropped (an
    /// overrun) and their place in the take is filled with silence. Called on the real-time
    /// thread: must not block, lock or allocate.
    fn write(&mut self, samples: &[f32]) -> usize;

    /// Ends the take; later writes are ignored. Called on the real-time thread: must not block,
    /// lock or allocate.
    fn end(&mut self);
}

/// One armed track.
pub struct ArmedTrack {
    /// The track's source node in the processing graph.
    pub source: NodeId,
    /// The device input channel (from 0) for each of the source's channels, left first.
    pub inputs: Vec<usize>,
    /// Whether the input replaces the track's clip playback in its source.
    pub monitor: bool,
    /// Where each channel's take goes, one per entry of `inputs`; empty to monitor without
    /// recording. A sink takes one take only: arm again with new sinks before each recording.
    pub sinks: Vec<Box<dyn TakeSink>>,
}

/// Where the last recording's capture began, published by the real-time side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakeInfo {
    /// The timeline frame the take's first frame was captured at.
    pub start_frame: u64,
    /// The plan's output latency then, in frames.
    pub latency: u64,
}

impl TakeInfo {
    /// Where to place the take: its timeline position and the offset into its files of the
    /// first frame to play. `device_latency` is the device's input plus output latency. The
    /// take is shifted back by that and the plan latency; a shift past frame 0 becomes the
    /// offset, so no recorded frame is lost.
    pub fn placement(&self, device_latency: u64) -> (u64, u64) {
        let shift = device_latency.saturating_add(self.latency);
        match self.start_frame.checked_sub(shift) {
            Some(position) => (position, 0),
            None => (0, shift - self.start_frame),
        }
    }
}

/// The deinterleaved device input of the current block, allocated once.
pub(crate) struct Input {
    channels: Vec<Box<[f32]>>,
    /// Device channels holding this block's input; later ones are silent.
    live: usize,
    silence: Box<[f32]>,
}

impl Input {
    /// Room for [`MAX_INPUTS`] channels of `max_frames`.
    pub(crate) fn new(max_frames: usize) -> Self {
        Input {
            channels: (0..MAX_INPUTS)
                .map(|_| vec![0.0; max_frames].into_boxed_slice())
                .collect(),
            live: 0,
            silence: vec![0.0; max_frames].into_boxed_slice(),
        }
    }

    /// Takes `frames` frames of `interleaved` input with `channels` channels. Input shorter than
    /// that is silence.
    pub(crate) fn load(&mut self, interleaved: &[f32], channels: usize, frames: usize) {
        let complete = channels > 0 && interleaved.len() >= frames.saturating_mul(channels);
        let live = if complete {
            channels.min(MAX_INPUTS)
        } else {
            0
        };
        for (c, buffer) in self.channels.iter_mut().take(live).enumerate() {
            let Some(buffer) = buffer.get_mut(..frames) else {
                continue;
            };
            for (f, sample) in buffer.iter_mut().enumerate() {
                *sample = interleaved[f * channels + c];
            }
        }
        self.live = live;
    }

    /// Device channel `channel`'s first `frames` frames, silence if it has none.
    pub(crate) fn channel(&self, channel: usize, frames: usize) -> &[f32] {
        let buffer = if channel < self.live {
            self.channels.get(channel).unwrap_or(&self.silence)
        } else {
            &self.silence
        };
        buffer.get(..frames).unwrap_or(&[])
    }
}

/// Whether `source` belongs to an armed track whose input or silence replaces its clips:
/// monitored, or recording.
pub(crate) fn replaces_clips(armed: &[ArmedTrack], source: NodeId, recording: bool) -> bool {
    armed
        .iter()
        .any(|a| a.source == source && (a.monitor || recording))
}

/// Ends every armed take.
pub(crate) fn end_takes(armed: &mut [ArmedTrack]) {
    for sink in armed.iter_mut().flat_map(|a| a.sinks.iter_mut()) {
        sink.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placement_shifts_back_by_every_latency() {
        let info = TakeInfo {
            start_frame: 10_000,
            latency: 300,
        };
        assert_eq!(info.placement(128), (9572, 0));
    }

    #[test]
    fn test_placement_before_zero_becomes_offset() {
        let info = TakeInfo {
            start_frame: 100,
            latency: 50,
        };
        assert_eq!(info.placement(128), (0, 78));
        assert_eq!(info.placement(50), (0, 0));
    }

    #[test]
    fn test_load_deinterleaves_and_silences_missing_channels() {
        let mut input = Input::new(4);
        input.load(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        assert_eq!(input.channel(0, 3), &[1.0, 3.0, 5.0]);
        assert_eq!(input.channel(1, 3), &[2.0, 4.0, 6.0]);
        assert_eq!(input.channel(2, 3), &[0.0; 3]);
        // Too short for the frames asked: silence.
        input.load(&[1.0, 2.0], 2, 3);
        assert_eq!(input.channel(0, 3), &[0.0; 3]);
        input.load(&[], 0, 3);
        assert_eq!(input.channel(1, 3), &[0.0; 3]);
    }
}
