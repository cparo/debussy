//! Modulation sources for the real-time core, as plain DSP primitives a node can wrap
//! (`docs/architecture/modulation.md` "LFO", ADR-0118). Everything here is real-time safe once
//! made: it allocates nothing, takes no locks and does no I/O (`docs/architecture/engine.md`
//! "Real-time thread").
//!
//! # Modules
//!
//! - `shape`: [`Shape`], the LFO waveforms, each a value from −1 to 1 at a phase.
//! - `lfo`: [`Lfo`], a phase-accumulator low-frequency oscillator with offset reads for stereo
//!   spread and voices.
//! - `sync`: tempo-sync arithmetic shared by tempo-synced parameters: [`sync::whole_note`],
//!   [`sync::synced_frames`] and [`sync::period_frames`].

#![forbid(unsafe_code)]

mod lfo;
mod shape;
pub mod sync;

pub use lfo::Lfo;
pub use shape::Shape;
