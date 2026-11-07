//! The project in memory: tracks, clips, edits as undoable commands, and named versions
//! (`docs/architecture/project.md` "Editing model", "Undo history" and "Named versions").
//! Saving and loading belong to `project_storage`.
//!
//! A [`State`] holds [`Track`]s in display order; each track has a fixed channel layout and its
//! [`Clip`]s from the bottom layer up. A clip plays a stretch of a [`Source`] (one mono file, or
//! a left and right pair for a stereo track) at a timeline position, with a gain and fades. All
//! times are frames at the project rate.
//!
//! Every change is an [`Edit`] that, when applied, returns the edit reversing it exactly. A
//! [`Session`] keeps the undo and redo histories, one step per gesture, and the named versions;
//! restoring a version is itself an undoable step.
//!
//! # Modules
//!
//! - `types`: [`TrackId`], [`ClipId`], [`AudioFile`], [`Source`].
//! - `state`: [`State`], [`Track`], [`Clip`].
//! - `edit`: [`Edit`].
//! - `session`: [`Session`].
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod edit;
mod error;
mod session;
mod state;
mod types;

pub use edit::Edit;
pub use error::Error;
pub use session::Session;
pub use state::{Clip, State, Track};
pub use types::{AudioFile, ClipId, Source, TrackId};
