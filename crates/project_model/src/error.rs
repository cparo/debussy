//! The crate's error type.

use crate::types::{ClipId, TrackId};

/// Why an edit was refused. A refused edit leaves the project unchanged.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    /// No track has this id.
    #[error("no {0}")]
    UnknownTrack(TrackId),
    /// No clip has this id.
    #[error("no {0}")]
    UnknownClip(ClipId),
    /// A track with this id already exists.
    #[error("{0} already exists")]
    TrackExists(TrackId),
    /// A clip with this id already exists.
    #[error("{0} already exists")]
    ClipExists(ClipId),
    /// A track or layer index out of range.
    #[error("index {index} is not below {limit}")]
    IndexOutOfRange {
        /// The index asked for.
        index: usize,
        /// The index must be below this.
        limit: usize,
    },
    /// A clip's source is mono on a stereo track, or the reverse.
    #[error("the clip's channel layout does not match the track's")]
    LayoutMismatch,
    /// A clip of no frames.
    #[error("a clip must be at least one frame long")]
    EmptyClip,
    /// A clip reaching past the end of its source.
    #[error("the clip reaches frame {end} of a source of {frames}")]
    ClipOutsideSource {
        /// Where the clip ends in its source.
        end: u64,
        /// The source's length.
        frames: u64,
    },
    /// A clip ending past the last representable frame of the timeline.
    #[error("the clip ends past the end of the timeline")]
    ClipOutsideTimeline,
    /// Fades that together are longer than the clip.
    #[error("fades of {fades} frames are longer than the clip ({length})")]
    FadesTooLong {
        /// The fade-in and fade-out lengths together.
        fades: u64,
        /// The clip's length.
        length: u64,
    },
    /// A clip gain that is not a finite number of decibels.
    #[error("clip gain must be a finite number of decibels")]
    InvalidGain,
    /// No named version has this name.
    #[error("no version named {0:?}")]
    UnknownVersion(String),
    /// A named version with this name already exists.
    #[error("a version named {0:?} already exists")]
    VersionExists(String),
}
