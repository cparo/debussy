//! The built-in instrument nodes: nodes that turn events on an event input into audio
//! (`docs/architecture/signal-graph.md` "Instruments", ADR-0147). Each implements
//! [`signal_graph::Node`] with event input 0 and stereo audio output 0.
//!
//! # Modules
//!
//! - `registry`: [`instrument`], [`display_name`], [`parameter_infos`] and [`INSTRUMENTS`], the
//!   built-in instruments by kind id.
//! - `silence`: [`Silence`], the stand-in for an instrument kind this build does not know.

#![forbid(unsafe_code)]

mod registry;
mod silence;

pub use registry::{INSTRUMENTS, InstrumentKind, display_name, instrument, parameter_infos};
pub use silence::Silence;
