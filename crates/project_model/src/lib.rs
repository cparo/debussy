//! The project in memory: tracks, clips, edits as undoable commands, and named versions
//! (`docs/architecture/project.md` "Editing model", "Undo history" and "Named versions").
//! Saving and loading belong to `project_storage`.
//!
//! A [`State`] holds [`Track`]s in display order; each track has a fixed channel layout and its
//! [`Clip`]s from the bottom layer up. A clip plays a stretch of a [`Source`] (one mono file, or
//! a left and right pair for a stereo track) at a timeline position, with a gain and fades. All
//! times are frames at the project rate.
//!
//! The state also holds the routing (`docs/architecture/project.md` "Routing"): each track is a
//! source [`Endpoint`], the project has one stereo output, and [`Node`]s (effects named by a
//! stable id with parameter values by name, buses, feedback delays) are joined by
//! [`Connection`]s.
//!
//! Every change is an [`Edit`] that, when applied, returns the edit reversing it exactly. A
//! [`Session`] keeps the undo and redo histories, one step per gesture, and the named versions;
//! restoring a version is itself an undoable step.
//!
//! The state also holds the project's one [`Tempo`] and meter (`docs/architecture/project.md`
//! "Musical time"), whose grid of bars and beats the click and the lanes follow.
//!
//! Range edits (`docs/architecture/project.md` "Range edits") cut, copy or delete a [`Range`] of
//! frames across tracks, splitting clips at its edges, and paste the [`Excerpt`] a cut or copy
//! returns.
//!
//! # Modules
//!
//! - `types`: [`TrackId`], [`ClipId`], [`AudioFile`], [`Source`].
//! - `state`: [`State`], [`Track`], [`Clip`].
//! - `routing`: [`Node`], [`NodeKind`], [`Connection`], [`Endpoint`] and their ids.
//! - `edit`: [`Edit`].
//! - `range`: [`Range`], [`Excerpt`], [`Lane`].
//! - `tempo`: [`Tempo`], [`Beat`] and the grid math.
//! - `session`: [`Session`].
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod edit;
mod error;
mod range;
mod routing;
mod session;
mod state;
pub mod tempo;
mod types;

pub use edit::Edit;
pub use error::Error;
pub use range::{Excerpt, Lane, Range};
pub use routing::{
    Connection, ConnectionId, Endpoint, MAX_BOARD_COORDINATE, Node, NodeId, NodeKind, Position,
    Side,
};
pub use session::Session;
pub use state::{Clip, State, Track};
pub use tempo::{Beat, Tempo};
pub use types::{AudioFile, ClipId, Source, TrackId};
