//! The built-in effect nodes, built from `dsp` (`docs/architecture/crates.md`). Each implements
//! [`signal_graph::Node`] (`docs/architecture/signal-graph.md` "Effect nodes").
//!
//! # Modules
//!
//! - `mixer`: [`Mixer`], channels of gain, pan and mute/solo summed to stereo
//!   (`docs/architecture/signal-graph.md` "Mixer").
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod error;
mod mixer;

pub use error::Error;
pub use mixer::{Control, MAX_CHANNELS, MAX_GAIN_DB, Mixer, Mode, SILENCE_DB, parameter_name};
