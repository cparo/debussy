//! The project as the engine runs it (ADR-0054): `project_model`'s routing kept in step with a
//! running `signal_graph` ([`Bridge`]), and its tracks turned into the engine's clip renderers
//! ([`track_renderers`]). It also prepares the bundled click sounds for the engine
//! ([`click_sounds`]).
//!
//! # Modules
//!
//! - `target`: what the graph must hold for a state.
//! - `mirror`: the graph as last synced, building and diffing it.
//! - `bridge`: [`Bridge`], [`RenderGraph`] and [`render_graph`].
//! - `click_sounds`: [`ClickSound`], [`prepare_click`] and [`click_track`].
//! - `clips`: [`track_renderers`], [`TrackRenderers`] and [`FileTable`].
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod bridge;
pub mod click_sounds;
mod clips;
mod error;
mod mirror;
mod target;

pub use bridge::{Bridge, RenderGraph, render_graph};
pub use click_sounds::{ClickSound, click_track, prepare as prepare_click};
pub use clips::{FileTable, TrackRenderers, track_renderers};
pub use error::Error;
