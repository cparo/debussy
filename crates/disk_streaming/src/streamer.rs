//! The control side: starting the reader thread, seeking, events and stopping.
//!
//! These calls run on a non-real-time thread (the engine's control or UI thread).
// Starting and joining the reader thread are blocking, which `clippy.toml` bans on the real-time
// side; the control side is not real-time.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Duration;

use audio_buffers::{BlockPool, Consumer, Layout, Producer, queue};

use crate::error::Error;
use crate::playback::Playback;
use crate::reader::{self, BLOCK_FRAMES, ReaderStream};
use crate::shared::StreamShared;
use crate::source::FrameSource;

/// Where a stream reads from after a seek.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// The first source frame to play.
    pub start: u64,
    /// A region `start..end` of source frames to repeat once reached, read ahead so it wraps
    /// without a gap; `None` plays to the end of the source.
    pub loop_region: Option<(u64, u64)>,
}

impl Plan {
    /// Plays from `start` to the end.
    pub fn from(start: u64) -> Self {
        Self {
            start,
            loop_region: None,
        }
    }
}

/// Settings for a [`Streamer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamerConfig {
    /// The project sample rate; every source must match it.
    pub sample_rate: u32,
    /// How much of each stream to keep read ahead: 2 s by default (ADR-0034).
    pub read_ahead: Duration,
}

impl StreamerConfig {
    /// The default read-ahead of 2 s at `sample_rate`.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            read_ahead: Duration::from_secs(2),
        }
    }
}

/// Something the reader thread reports to the control side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Reading a stream's source failed; the stream plays silence until its next seek.
    ReadFailed {
        /// The stream's index.
        stream: usize,
        /// What went wrong.
        message: String,
    },
}

/// Requests from the control side to the reader thread.
#[derive(Debug)]
pub(crate) enum Command {
    Seek {
        stream: usize,
        generation: u16,
        plan: Plan,
    },
    Stop,
}

/// The control side of disk streaming. Dropping it stops the reader thread.
#[derive(Debug)]
pub struct Streamer {
    shared: Vec<Arc<StreamShared>>,
    frames: Vec<u64>,
    commands: Producer<Command>,
    events: Consumer<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Streamer {
    /// Starts streaming `sources` from frame 0, returning the control side and one [`Playback`]
    /// per source, in order, for the real-time thread.
    ///
    /// # Errors
    ///
    /// [`Error::SampleRateMismatch`] if a source is at another rate, [`Error::Buffers`] if buffers
    /// cannot be made, or [`Error::Thread`] if the reader thread cannot start.
    pub fn start(
        sources: Vec<Box<dyn FrameSource>>,
        config: StreamerConfig,
    ) -> Result<(Self, Vec<Playback>), Error> {
        let read_ahead = (config.read_ahead.as_secs_f64() * f64::from(config.sample_rate)).ceil();
        // Saturating: a read-ahead beyond u64 frames is not meaningful anyway.
        let read_ahead = read_ahead as u64;
        // Enough blocks for the read-ahead, plus one being played and one being filled.
        let blocks = usize::try_from(read_ahead.div_ceil(BLOCK_FRAMES as u64))
            .unwrap_or(usize::MAX / 2)
            .saturating_add(2);

        let mut shared = Vec::with_capacity(sources.len());
        let mut frames = Vec::with_capacity(sources.len());
        let mut readers = Vec::with_capacity(sources.len());
        let mut playbacks = Vec::with_capacity(sources.len());
        for source in sources {
            if source.sample_rate() != config.sample_rate {
                return Err(Error::SampleRateMismatch {
                    expected: config.sample_rate,
                    found: source.sample_rate(),
                });
            }
            let stream = Arc::new(StreamShared::default());
            let (chunks_tx, chunks_rx) = queue::channel(blocks)?;
            let (returns_tx, returns_rx) = queue::channel(blocks)?;
            frames.push(source.frames());
            readers.push(ReaderStream {
                source,
                shared: Arc::clone(&stream),
                pool: BlockPool::new(Layout::Mono, BLOCK_FRAMES, blocks)?,
                chunks: chunks_tx,
                returns: returns_rx,
                generation: 0,
                plan: Plan::from(0),
                position: 0,
                source_position: None,
                queued: 0,
                done: false,
            });
            playbacks.push(Playback::new(Arc::clone(&stream), chunks_rx, returns_tx));
            shared.push(stream);
        }
        let (commands, commands_rx) = queue::channel(64)?;
        let (events_tx, events) = queue::channel(64)?;
        let thread = std::thread::Builder::new()
            .name("disk-streaming".into())
            .spawn(move || reader::run(readers, commands_rx, events_tx, read_ahead))
            .map_err(Error::Thread)?;
        Ok((
            Self {
                shared,
                frames,
                commands,
                events,
                thread: Some(thread),
            },
            playbacks,
        ))
    }

    /// Moves stream `stream` to `plan`. Blocks already queued for the old position are discarded
    /// by the real-time side; check [`Playback::is_primed`] before playing again.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownStream`], [`Error::PlanOutOfRange`], or [`Error::Busy`] if the reader has
    /// not yet taken earlier requests.
    pub fn seek(&mut self, stream: usize, plan: Plan) -> Result<(), Error> {
        let (Some(shared), Some(&frames)) = (self.shared.get(stream), self.frames.get(stream))
        else {
            return Err(Error::UnknownStream(stream));
        };
        let loop_ok = plan
            .loop_region
            .is_none_or(|(start, end)| start < end && end <= frames);
        if plan.start > frames || !loop_ok {
            return Err(Error::PlanOutOfRange { frames });
        }
        let generation = shared.generation.load(Ordering::Acquire).wrapping_add(1);
        self.commands
            .try_push(Command::Seek {
                stream,
                generation,
                plan,
            })
            .map_err(|_| Error::Busy)?;
        shared.generation.store(generation, Ordering::Release);
        Ok(())
    }

    /// Underruns counted on stream `stream` so far, for the UI's notice; `None` for an unknown
    /// stream.
    pub fn underruns(&self, stream: usize) -> Option<u64> {
        self.shared
            .get(stream)
            .map(|s| s.underruns.load(Ordering::Relaxed))
    }

    /// The next event from the reader thread, if any.
    pub fn poll_event(&mut self) -> Option<Event> {
        self.events.try_pop()
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        // If the queue is full, keep trying: the reader drains it every pass.
        while self.commands.try_push(Command::Stop).is_err() {
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
