//! The built-in effect nodes, built from `dsp` (`docs/architecture/crates.md`). Each implements
//! [`signal_graph::Node`] (`docs/architecture/signal-graph.md` "Effect nodes").
//!
//! # Modules
//!
//! - `mixer`: [`Mixer`], channels of gain, pan and mute/solo summed to stereo
//!   (`docs/architecture/signal-graph.md` "Mixer"), and [`mixer_parameter_infos`], a mixer's
//!   parameters declared without building one.
//! - `filters`: [`Butterworth`], the high-pass and low-pass filters
//!   (`docs/architecture/effects.md` "High-pass and low-pass filters").
//! - `parametric`: [`ParametricEq`], the 4-band equaliser with optional high-pass and low-pass
//!   stages (`docs/architecture/effects.md` "Parametric equaliser").
//! - `dynamics`: [`Dynamics`], the compressor, expander, gate and limiter, and
//!   [`TransferCurve`], its curve (`docs/architecture/effects.md` "Dynamics").
//! - `amplifier`: [`Amplifier`], a pre-amp, waveshaper and DC filter with optional
//!   oversampling, and [`ShaperCurve`], its curve (`docs/architecture/effects.md` "Amplifier").
//! - `delay`: [`Delay`], a whole-frame tap delay, free or tempo-synced, in stereo or ping-pong
//!   (`docs/architecture/delay-and-reverb.md` "Delay").
//! - `reverb`: [`Reverb`], a feedback delay network or Dattorro's plate behind a pre-delay
//!   (`docs/architecture/delay-and-reverb.md` "Reverb").
//! - `tap`: a whole-frame tap that crossfades when it changes, for `delay` and `reverb`.
//! - `utility`: [`Bus`], [`Pick`], [`Place`] and [`Silence`].
//! - `registry`: [`effect`], [`display_name`], [`parameter_infos`] and [`EFFECTS`], the built-in
//!   effects by kind id, and [`PRESETS`], named starting values for them.
//! - `curve`: [`Curve`] and [`curve`], the curve an effect kind's parameter values ask for, for
//!   display (`docs/architecture/effects.md` "Effect curves").
//! - `response`: [`FrequencyResponse`], the magnitude response an equaliser's parameters ask
//!   for.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod amplifier;
mod curve;
mod delay;
mod dynamics;
mod error;
mod filters;
mod mixer;
mod parametric;
mod registry;
mod response;
mod reverb;
mod tap;
mod utility;

pub use amplifier::{Amplifier, Shape, ShaperCurve};
pub use curve::{Curve, CurveFn, curve};
pub use delay::{Delay, MAX_DELAY_MS, MAX_OFFSET_MS};
pub use dynamics::{Dynamics, MAX_LOOKAHEAD_MS, MAX_RATIO, TransferCurve};
pub use error::Error;
pub use filters::{Butterworth, MAX_CUTOFF, MAX_ORDER, MIN_CUTOFF, Response};
pub use mixer::{
    Control, MAX_CHANNELS, MAX_GAIN_DB, Mixer, Mode, SILENCE_DB, mixer_parameter_infos,
    parameter_name,
};
pub use parametric::{
    BANDS, BandControl, BandType, MAX_BAND_GAIN_DB, ParametricEq, PassControl, band_parameter_name,
    pass_parameter_name,
};
pub use registry::{
    EFFECTS, EffectKind, PRESETS, Preset, display_name, effect, matching_preset, parameter_infos,
    preset, presets,
};
pub use response::FrequencyResponse;
pub use reverb::{MAX_PRE_DELAY_MS, Reverb};
pub use utility::{Bus, Pick, Place, Silence};
