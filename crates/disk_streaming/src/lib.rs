//! Read-ahead of project audio from disk into buffers for the real-time engine
//! (`docs/architecture/engine.md` "Disk streaming", ADR-0034).
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
//! # Modules
//!
//! - `source`: [`FrameSource`], the reader's view of a file, and [`open_wav`].
//! - `shared`: the atomics shared by the control, reader and real-time sides.
//! - `playback`: [`Playback`], the real-time side.
//! - `reader`: the reader thread.
//! - `streamer`: [`Streamer`], the control side: start, seek, events, stop.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod error;
mod playback;
mod reader;
mod shared;
mod source;
mod streamer;

pub use error::Error;
pub use playback::Playback;
pub use source::{FrameSource, open_wav};
pub use streamer::{Event, Plan, Streamer, StreamerConfig};
