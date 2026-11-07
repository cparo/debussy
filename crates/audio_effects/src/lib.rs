//! The built-in effect nodes, built from `dsp` (`docs/architecture/crates.md`). Each implements
//! [`signal_graph::Node`] (`docs/architecture/signal-graph.md` "Effect nodes").
//!
//! # Modules
//!
//! - `mixer`: [`Mixer`], channels of gain, pan and mute/solo summed to stereo
//!   (`docs/architecture/signal-graph.md` "Mixer").
//! - `utility`: [`Bus`], [`Silence`] and [`effect`], the effect registry by kind id.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod error;
mod mixer;
mod utility;

pub use error::Error;
pub use mixer::{Control, MAX_CHANNELS, MAX_GAIN_DB, Mixer, Mode, SILENCE_DB, parameter_name};
pub use utility::{Bus, Silence, effect};
