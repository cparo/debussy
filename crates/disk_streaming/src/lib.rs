//! Read-ahead of project audio from disk into buffers for the real-time engine
//! (`docs/architecture/engine.md` "Disk streaming", ADR-0034), and recorded takes written to disk
//! (`docs/architecture/recording.md` "Writer thread", ADR-0078).
//!
//! [`Streamer::start`] takes one [`FrameSource`] per stream (normally a mono WAV file of stored
//! audio, opened with [`open_wav`]), spawns one reader thread, and returns a [`Playback`] per
//! stream for the real-time thread. The reader keeps 2 seconds of each stream read ahead, serving
//! the stream that will run out soonest first.
//!
//! - The real-time side ([`Playback::read`]) never blocks, locks or allocates. A late block plays
//!   as silence and is counted as an underrun.
//! - [`Streamer::seek`] moves a stream and optionally sets a loop region, which is read ahead so it
//!   wraps without a gap. Each seek starts a new *generation*; blocks from an earlier generation
//!   are discarded when the real-time side meets them.
//! - [`Playback::is_primed`] tells the transport when a stream has enough buffered to start.
//!
//! [`Recorder::start`] creates one new mono WAV file per take channel, spawns one writer thread,
//! and returns a [`Take`] per file for the real-time thread. Each take holds 2 seconds of blocks;
//! the real-time side ([`Take::write`]) fills and queues them, and the writer appends them. If the
//! disk falls behind, frames are dropped, counted as an overrun, and written as silence so the
//! take stays aligned. [`Take::end`] ends a take; the writer then finishes its file and reports a
//! [`TakeEvent`].
//!
//! # Modules
//!
//! - `source`: [`FrameSource`], the reader's view of a file, and [`open_wav`].
//! - `shared`: the atomics shared by the control, reader and real-time sides.
//! - `playback`: [`Playback`], the real-time side.
//! - `reader`: the reader thread.
//! - `streamer`: [`Streamer`], the control side: start, seek, events, stop.
//! - `take`: [`Take`], the real-time end of one recorded channel.
//! - `recorder`: [`Recorder`], the control side of recording, and the writer thread.
//! - `engine`: the bridge to `audio_engine` (ADR-0046): [`TimelineSource`], a track channel's clips
//!   rendered as a stream; [`WavClipAudio`], stored files read for the renderer; and the engine's
//!   [`TrackFeed`](audio_engine::TrackFeed) and [`Streams`](audio_engine::Streams) for
//!   [`Playback`] and [`Streamer`], and its [`TakeSink`](audio_engine::TakeSink) for [`Take`].
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod engine;
mod error;
mod playback;
mod reader;
mod recorder;
mod shared;
mod source;
mod streamer;
mod take;

pub use engine::{TimelineSource, WavClipAudio};
pub use error::Error;
pub use playback::Playback;
pub use recorder::{Recorder, RecorderConfig, TakeEvent};
pub use source::{FrameSource, open_wav};
pub use streamer::{Event, Plan, Streamer, StreamerConfig};
pub use take::Take;
