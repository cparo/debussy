//! The crate's error type.

/// Why an engine call failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The graph refused the block.
    #[error(transparent)]
    Graph(#[from] signal_graph::Error),
    /// A queue could not be made.
    #[error(transparent)]
    Buffers(#[from] audio_buffers::Error),
    /// Clip and track changes are accepted only while the transport is stopped or paused.
    #[error("clips can be changed only while the transport is stopped or paused")]
    Playing,
    /// The real-time side has not yet taken earlier requests; try again shortly.
    #[error("the engine is busy; try again")]
    Busy,
    /// Moving the track feeds failed.
    #[error("moving the track streams failed: {0}")]
    Streams(Box<dyn std::error::Error + Send + Sync>),
    /// Reading a clip's stored audio failed.
    #[error("reading clip audio failed: {0}")]
    ClipAudio(Box<dyn std::error::Error + Send + Sync>),
    /// The destination of an offline render failed.
    #[error("writing the render failed: {0}")]
    Output(Box<dyn std::error::Error + Send + Sync>),
    /// A clip refers to audio it cannot play.
    #[error("clip {index} is invalid: {reason}")]
    InvalidClip {
        /// The clip's index, bottom layer first.
        index: usize,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// Output buffers of different lengths, or of none.
    #[error("output buffers of {left} and {right} frames")]
    BufferLength {
        /// The left buffer's length.
        left: usize,
        /// The right buffer's length.
        right: usize,
    },
}
