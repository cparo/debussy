//! How track audio reaches the engine: [`TrackFeed`]s, grouped per track as [`TrackFeeds`], moved
//! by [`Streams`], and [`RenderFeed`] for offline render.

use signal_graph::NodeId;

use crate::timeline::{ClipAudio, TrackRenderer};

/// One track channel's audio, read in order from the position its stream was last moved to.
/// Normally a disk stream (`docs/architecture/engine.md` "Disk streaming").
pub trait TrackFeed: Send {
    /// Fills `out` with the next frames and returns how many were audio; the rest are silence.
    /// Called on the real-time thread: must not block, lock or allocate.
    fn read(&mut self, out: &mut [f32]) -> usize;

    /// Whether at least `frames` frames are ready at the current position, or everything left
    /// is. Called on the real-time thread.
    fn is_primed(&self, frames: u64) -> bool;
}

/// One track's feeds, one per channel (left first), and the graph source they fill.
pub struct TrackFeeds {
    /// The track's source node in the processing graph.
    pub source: NodeId,
    /// One feed per channel of the source's layout.
    pub feeds: Vec<Box<dyn TrackFeed>>,
}

/// Moves every track feed to a new timeline position, off the real-time thread. Implemented by
/// the disk streamer.
pub trait Streams {
    /// Starts every feed reading from timeline frame `frame`.
    ///
    /// # Errors
    ///
    /// Whatever stops the move; the engine reports it as [`crate::Error::Streams`].
    fn seek(&mut self, frame: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// A feed that renders its clips as it is read, for offline render. Always primed. A failed read
/// plays as silence and is kept for [`RenderFeed::take_error`].
pub struct RenderFeed<A: ClipAudio + Send> {
    renderer: TrackRenderer,
    audio: A,
    position: u64,
    error: Option<crate::Error>,
}

impl<A: ClipAudio + Send> RenderFeed<A> {
    /// A feed of `renderer`'s clips, read through `audio`, starting at timeline frame `start`.
    pub fn new(renderer: TrackRenderer, audio: A, start: u64) -> Self {
        RenderFeed {
            renderer,
            audio,
            position: start,
            error: None,
        }
    }

    /// The first read error, if any, clearing it.
    pub fn take_error(&mut self) -> Option<crate::Error> {
        self.error.take()
    }
}

impl<A: ClipAudio + Send> TrackFeed for RenderFeed<A> {
    fn read(&mut self, out: &mut [f32]) -> usize {
        if let Err(error) = self.renderer.render(self.position, out, &mut self.audio) {
            out.fill(0.0);
            self.error.get_or_insert(error);
        }
        self.position = self.position.saturating_add(out.len() as u64);
        out.len()
    }

    fn is_primed(&self, _frames: u64) -> bool {
        true
    }
}
