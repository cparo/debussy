//! The built-in effect nodes, built from `dsp` (`docs/architecture/crates.md`). Each implements
//! [`signal_graph::Node`] (`docs/architecture/signal-graph.md` "Effect nodes").
//!
//! # Modules
//!
//! - `mixer`: [`Mixer`], channels of gain, pan and mute/solo summed to stereo
//!   (`docs/architecture/signal-graph.md` "Mixer").
//! - `filters`: [`Butterworth`], the high-pass and low-pass filters
//!   (`docs/architecture/effects.md` "High-pass and low-pass filters").
//! - `parametric`: [`ParametricEq`], the 4-band equaliser with optional high-pass and low-pass
//!   stages (`docs/architecture/effects.md` "Parametric equaliser").
//! - `dynamics`: [`Dynamics`], the compressor, limiter, gate and saturator
//!   (`docs/architecture/effects.md` "Dynamics").
//! - `utility`: [`Bus`], [`Pick`], [`Place`] and [`Silence`].
//! - `registry`: [`effect`], [`display_name`], [`parameter_infos`] and [`EFFECTS`], the built-in
//!   effects by kind id.
//! - `response`: [`FrequencyResponse`] and [`frequency_response`], the magnitude response an
//!   equaliser's parameters ask for, for display.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod dynamics;
mod error;
mod filters;
mod mixer;
mod parametric;
mod registry;
mod response;
mod utility;

pub use dynamics::{
    Dynamics, DynamicsControl, MAX_ATTACK_MS, MAX_LOOKAHEAD_MS, MAX_RELEASE_MS,
    OVERSAMPLING_FACTORS, static_output, transfer, window_weight,
};
pub use error::Error;
pub use filters::{Butterworth, MAX_CUTOFF, MAX_ORDER, MIN_CUTOFF, Response};
pub use mixer::{Control, MAX_CHANNELS, MAX_GAIN_DB, Mixer, Mode, SILENCE_DB, parameter_name};
pub use parametric::{
    BANDS, BandControl, BandType, MAX_BAND_GAIN_DB, ParametricEq, PassControl, band_parameter_name,
    pass_parameter_name,
};
pub use registry::{EFFECTS, EffectKind, display_name, effect, parameter_infos};
pub use response::{FrequencyResponse, frequency_response};
pub use utility::{Bus, Pick, Place, Silence};
