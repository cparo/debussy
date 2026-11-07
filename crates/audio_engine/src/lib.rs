//! The audio engine: runs the processing plan each block, feeding each track's signal into its
//! graph source and taking the project output from its sink (`docs/architecture/engine.md`
//! "Clip playback", "Transport", "Offline render").
//!
//! The engine does no I/O and knows nothing of the project model. The layer above supplies:
//!
//! - each track channel's audio as a [`TrackFeed`], normally a disk stream whose source is a
//!   [`TrackRenderer`] reading the stored files through [`ClipAudio`];
//! - a [`Streams`] handle that moves every feed to a new position when the transport starts or
//!   jumps;
//! - the `signal_graph` [`Runner`](signal_graph::Runner) and the ids of the track sources and the
//!   output sink.
//!
//! [`Engine::new`] returns the control side, [`Engine`], and the real-time side, [`Processor`].
//! Offline render ([`render`]) runs the same per-block step over [`RenderFeed`]s, which call the
//! clip renderer directly, so it produces the same samples as playback.
//!
//! # Modules
//!
//! - `timeline`: [`ChannelClip`], [`ClipAudio`] and [`TrackRenderer`], a track channel's clips
//!   mixed into one signal.
//! - `feed`: [`TrackFeed`], [`TrackFeeds`], [`Streams`] and [`RenderFeed`].
//! - `block`: the per-block step shared by playback and render.
//! - `processor`: [`Processor`], the real-time side.
//! - `engine`: [`Engine`], the control side, and [`TransportState`].
//! - `render`: [`render`], offline render.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod block;
mod engine;
mod error;
mod feed;
mod processor;
mod render;
mod timeline;

pub use engine::{Engine, TransportState};
pub use error::Error;
pub use feed::{RenderFeed, Streams, TrackFeed, TrackFeeds};
pub use processor::Processor;
pub use render::render;
pub use timeline::{CROSSFADE_SECONDS, ChannelClip, ClipAudio, TrackRenderer};
