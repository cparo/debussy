//! The crate's error type.

use crate::routing::{ConnectionId, Endpoint, NodeId};
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
    /// A range of no frames: its start is not below its end.
    #[error("a range must be at least one frame long")]
    EmptyRange,
    /// A paste whose lanes run past the last track.
    #[error("the paste needs {needed} tracks from there but {available} remain")]
    NotEnoughTracks {
        /// The lanes in the excerpt.
        needed: usize,
        /// The tracks from the target track down, inclusive.
        available: usize,
    },
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
    /// No routing node has this id.
    #[error("no {0}")]
    UnknownNode(NodeId),
    /// No connection has this id.
    #[error("no {0}")]
    UnknownConnection(ConnectionId),
    /// A routing node with this id already exists.
    #[error("{0} already exists")]
    NodeExists(NodeId),
    /// A connection with this id already exists.
    #[error("{0} already exists")]
    ConnectionExists(ConnectionId),
    /// A connection names a side of a port that is mono.
    #[error("{0:?} has no left or right side there: the port is mono")]
    SideOfMonoPort(Endpoint),
    /// The same ports are already connected.
    #[error("those ports are already connected")]
    AlreadyConnected,
    /// A connection out of the project output, into a track, or to a port a track, bus, delay or
    /// the output does not have.
    #[error("no such port on {0:?}")]
    NoSuchPort(Endpoint),
    /// A track or node that still has connections cannot be removed.
    #[error("{0:?} still has connections")]
    StillConnected(Endpoint),
    /// The node is not of the kind the edit needs.
    #[error("{0} is not of that kind")]
    WrongNodeKind(NodeId),
    /// A parameter value, pan or delay time that is not finite or is out of range.
    #[error("the value is not finite or is out of range")]
    InvalidValue,
}
