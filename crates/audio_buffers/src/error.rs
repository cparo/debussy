//! The crate's error type.

use crate::block::{MAX_FRAMES, MIN_FRAMES};

/// An invalid size given to a constructor or setter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A block capacity outside the engine's buffer sizes.
    #[error("block capacity of {0} frames is outside {MIN_FRAMES}..={MAX_FRAMES}")]
    CapacityOutOfRange(usize),
    /// A frame count larger than the block's capacity.
    #[error("{frames} frames is more than the block's capacity of {capacity}")]
    FramesOverCapacity {
        /// The requested frame count.
        frames: usize,
        /// The block's capacity.
        capacity: usize,
    },
    /// A queue or pool that could hold nothing.
    #[error("capacity must be at least 1")]
    ZeroCapacity,
    /// A queue too large for its positions to be counted.
    #[error("queue capacity of {0} is too large")]
    QueueTooLarge(usize),
}
