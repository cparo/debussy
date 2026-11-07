//! Signal-processing primitives for the real-time core. Every function here is real-time safe: it
//! allocates nothing, takes no locks and does no I/O (`docs/architecture/engine.md`
//! "Real-time thread").
//!
//! Functions work in place on plain `f32` slices, such as the channels of an
//! `audio_buffers::Block`. Where two slices are combined, the shorter one sets the length.
//!
//! # Modules
//!
//! - `noise`: [`Noise`], a seeded pseudo-random generator for dither.
//! - `dither`: [`Ditherer`], TPDF dither and quantisation to 16 or 24 bits (ADR-0025).
//! - `level`: gain in decibels, the −3 dB pan law (ADR-0018) and the mono fold-down (ADR-0030).

#![forbid(unsafe_code)]

mod dither;
mod level;
mod noise;

pub use dither::{Depth, Ditherer};
pub use level::{
    FOLD_DOWN_GAIN, apply_gain, db_to_gain, fold_to_mono, gain_to_db, pan_gains, pan_mono,
};
pub use noise::Noise;
