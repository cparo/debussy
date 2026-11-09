//! The bridge to `audio_engine` (`docs/architecture/engine.md` "Clip playback", ADR-0046): each
//! track channel streams as a [`TimelineSource`], whose reads render the channel's clips from the
//! stored files through [`WavClipAudio`] on the reader thread; [`Playback`] feeds the engine and
//! [`Streamer`] moves every stream when the transport starts or jumps. A [`Take`] is the
//! engine's sink for one recorded channel (`docs/architecture/recording.md` "Capture").

use std::path::PathBuf;

use audio_engine::{ClipAudio, Streams, TakeSink, TrackFeed, TrackRenderer};

use crate::error::Error;
use crate::playback::Playback;
use crate::source::{FrameSource, open_wav};
use crate::streamer::{Plan, Streamer};
use crate::take::Take;

impl TakeSink for Take {
    fn write(&mut self, samples: &[f32]) -> usize {
        Take::write(self, samples)
    }

    fn end(&mut self) {
        Take::end(self);
    }
}

impl TrackFeed for Playback {
    fn read(&mut self, out: &mut [f32]) -> usize {
        Playback::read(self, out)
    }

    fn is_primed(&mut self, frames: u64) -> bool {
        self.discard_stale();
        Playback::is_primed(self, frames)
    }
}

impl Streams for Streamer {
    /// Seeks every stream to `frame`, or to its end if it is shorter.
    fn seek(&mut self, frame: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for stream in 0..self.streams() {
            let start = frame.min(self.stream_frames(stream).unwrap_or(0));
            Streamer::seek(self, stream, Plan::from(start))?;
        }
        Ok(())
    }
}

/// Stored mono files, opened when first read, for the clip renderer.
pub struct WavClipAudio {
    paths: Vec<PathBuf>,
    open: Vec<Option<Box<dyn FrameSource>>>,
}

impl WavClipAudio {
    /// Reads the files at `paths`; a clip's `file` is an index into it.
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let open = paths.iter().map(|_| None).collect();
        WavClipAudio { paths, open }
    }

    fn read_file(&mut self, file: usize, frame: u64, out: &mut [f32]) -> Result<(), Error> {
        let (Some(path), Some(slot)) = (self.paths.get(file), self.open.get_mut(file)) else {
            return Err(Error::UnknownStream(file));
        };
        let source = match slot {
            Some(source) => source,
            None => slot.insert(open_wav(path)?),
        };
        let frames = source.frames();
        if frame.saturating_add(out.len() as u64) > frames {
            return Err(Error::PlanOutOfRange { frames });
        }
        source.seek(frame)?;
        let mut filled = 0;
        while filled < out.len() {
            let n = source.read(&mut out[filled..])?;
            if n == 0 {
                return Err(Error::PlanOutOfRange { frames });
            }
            filled += n;
        }
        Ok(())
    }
}

impl ClipAudio for WavClipAudio {
    fn read(
        &mut self,
        file: usize,
        frame: u64,
        out: &mut [f32],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.read_file(file, frame, out)?)
    }
}

/// One track channel's clips as a stream: reads render them at the read position.
pub struct TimelineSource<A: ClipAudio + Send> {
    renderer: TrackRenderer,
    audio: A,
    sample_rate: u32,
    position: u64,
}

impl<A: ClipAudio + Send> TimelineSource<A> {
    /// A stream of `renderer`'s clips read through `audio`, at the project's `sample_rate`.
    pub fn new(renderer: TrackRenderer, audio: A, sample_rate: u32) -> Self {
        TimelineSource {
            renderer,
            audio,
            sample_rate,
            position: 0,
        }
    }
}

impl<A: ClipAudio + Send> FrameSource for TimelineSource<A> {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn frames(&self) -> u64 {
        self.renderer.frames()
    }

    fn seek(&mut self, frame: u64) -> Result<(), Error> {
        let frames = self.renderer.frames();
        if frame > frames {
            return Err(Error::PlanOutOfRange { frames });
        }
        self.position = frame;
        Ok(())
    }

    fn read(&mut self, out: &mut [f32]) -> Result<usize, Error> {
        let left = self.renderer.frames().saturating_sub(self.position);
        let n = (out.len() as u64).min(left) as usize;
        self.renderer
            .render(self.position, &mut out[..n], &mut self.audio)?;
        self.position += n as u64;
        Ok(n)
    }
}
