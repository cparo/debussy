//! The project as the engine runs it (ADR-0054): `project_model`'s routing kept in step with a
//! running `signal_graph` ([`Bridge`]), and its tracks turned into the engine's clip renderers
//! ([`track_renderers`]).
//!
//! # Modules
//!
//! - `target`: what the graph must hold for a state.
//! - `mirror`: the graph as last synced, building and diffing it.
//! - `bridge`: [`Bridge`], [`RenderGraph`] and [`render_graph`].
//! - `clips`: [`track_renderers`], [`TrackRenderers`] and [`FileTable`].
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod bridge;
mod clips;
mod error;
mod mirror;
mod target;

pub use bridge::{Bridge, RenderGraph, render_graph};
pub use clips::{FileTable, TrackRenderers, track_renderers};
pub use error::Error;
