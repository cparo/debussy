//! The crate's error type.

use crate::types::{ConnectionId, NodeId};

/// Why a graph edit, a commit or a block was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    /// The sample rate is zero.
    #[error("the sample rate must be above 0 Hz")]
    ZeroSampleRate,
    /// No node has this id.
    #[error("no {0}")]
    UnknownNode(NodeId),
    /// No connection has this id.
    #[error("no {0}")]
    UnknownConnection(ConnectionId),
    /// The node has no output with this index.
    #[error("{node} has no output {port}")]
    NoSuchOutput {
        /// The node.
        node: NodeId,
        /// The output index asked for.
        port: usize,
    },
    /// The node has no input with this index.
    #[error("{node} has no input {port}")]
    NoSuchInput {
        /// The node.
        node: NodeId,
        /// The input index asked for.
        port: usize,
    },
    /// The two ports carry different signal types.
    #[error("the ports carry different signal types")]
    PortTypeMismatch,
    /// The same output is already connected to the same input.
    #[error("those ports are already connected")]
    AlreadyConnected,
    /// The connection would close a cycle that passes through no feedback delay.
    #[error("a cycle must pass through a feedback delay")]
    CycleWithoutDelay,
    /// The node is not a feedback delay.
    #[error("{0} is not a feedback delay")]
    NotAFeedbackDelay(NodeId),
    /// A feedback-delay time that is not finite or is longer than the longest allowed.
    #[error("a feedback delay of {0} s is not allowed")]
    InvalidDelay(f64),
    /// A parameter range that is not finite or whose minimum is above its maximum.
    #[error("a parameter's range must be finite, with its minimum at most its maximum")]
    InvalidParameterRange,
    /// A block longer than the graph's largest.
    #[error("{frames} frames is more than the largest block of {max}")]
    FramesOutOfRange {
        /// The frames asked for.
        frames: usize,
        /// The graph's largest block.
        max: usize,
    },
    /// The real-time thread has not yet taken the plans already committed; try again shortly.
    #[error("the real-time thread has not taken earlier plans; try again")]
    Busy,
    /// A block or queue could not be made.
    #[error(transparent)]
    Buffers(#[from] audio_buffers::Error),
}
