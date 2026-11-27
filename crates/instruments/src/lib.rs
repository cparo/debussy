//! The built-in instrument nodes: nodes that turn events on an event input into audio
//! (`docs/architecture/signal-graph.md` "Instruments", ADR-0147). Each implements
//! [`signal_graph::Node`] with event input 0 and stereo audio output 0.
//!
//! # Modules
//!
//! - `registry`: [`instrument`], [`display_name`], [`parameter_infos`], [`note_curve`] and
//!   [`INSTRUMENTS`], the built-in instruments by kind id.
//! - `note_curve`: [`NoteCurve`], one note an instrument's parameters play, for its display
//!   (ADR-0172).
//! - `sampler`: [`Sampler`], which plays the project's own audio files by key-range [`Zone`]s
//!   (`docs/architecture/instruments.md` "Sampler").
//! - `silence`: [`Silence`], the stand-in for an instrument kind this build does not know.
//! - `synth`: [`Synth`], the polyphonic subtractive synthesizer
//!   (`docs/architecture/instruments.md` "Synthesizer").
//! - `voices`: the voice pool and amplitude-envelope parameters the instruments share.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod error;
mod note_curve;
mod registry;
mod sampler;
mod silence;
mod synth;
mod voices;

pub use error::Error;
pub use note_curve::{MAX_RATE as NOTE_MAX_RATE, MAX_SPAN as NOTE_MAX_SPAN, NOTE, Note, NoteCurve};
pub use registry::{
    DisplayFn, INSTRUMENTS, InstrumentKind, display_name, instrument, note_curve, parameter_infos,
    sampler,
};
pub use sampler::{Playback, SAMPLER_VOICES, Sampler, Zone};
pub use silence::Silence;
pub use synth::{MAX_VOICES, Synth};
