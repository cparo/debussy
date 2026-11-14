//! Signal-processing primitives for the real-time core. Every function here is real-time safe: it
//! allocates nothing, takes no locks and does no I/O (`docs/architecture/engine.md`
//! "Real-time thread").
//!
//! Functions work in place on plain `f32` slices, such as the channels of an
//! `audio_buffers::Block`. Where two slices are combined, the shorter one sets the length.
//!
//! # Modules
//!
//! - `noise`: [`Noise`], a seeded pseudo-random generator for dither, and [`Pink`], seeded pink
//!   noise (ADR-0122).
//! - `dither`: [`Ditherer`], TPDF dither and quantisation to 16 or 24 bits (ADR-0025).
//! - `level`: gain in decibels, the −3 dB pan law (ADR-0018) and the mono fold-down (ADR-0030).
//! - `fft`: [`Fft`] and [`RealFft`], radix-2 transforms, and [`Stft`], Hann-windowed spectra
//!   (ADR-0039).
//! - `filter`: [`Svf`] and [`OnePole`], trapezoidal filters, their coefficient designers and
//!   [`butterworth_q`].
//! - `dynamics`: [`Ballistics`], the [`Curve`] and [`Ceiling`] of a compressor, expander, gate
//!   or limiter, and [`DelayLine`] for lookahead (ADR-0099).
//! - `delay`: [`TapDelay`], a delay line read at any whole-frame tap, or at a fractional one by
//!   linear or cubic Hermite interpolation.
//! - `resample`: [`Resampler`], exact rational polyphase sample-rate conversion (ADR-0036).
//! - `oversample`: [`Oversampler`], 2× or 4× oversampling around a nonlinearity (ADR-0102).
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod delay;
mod dither;
mod dynamics;
mod error;
mod fft;
mod filter;
mod level;
mod noise;
mod oversample;
mod resample;

pub use delay::TapDelay;
pub use dither::{Depth, Ditherer};
pub use dynamics::{
    Ballistics, Ceiling, Curve, DelayLine, FLOOR_DB, ceiling_gain_db, curve_gain_db,
    smoothing_coefficient, transfer_db,
};
pub use error::Error;
pub use fft::{
    Complex, Fft, MAX_SIZE as MAX_FFT_SIZE, MIN_SIZE as MIN_FFT_SIZE, RealFft, Stft, hann,
};
pub use filter::{
    MAX_FREQUENCY_RATIO, OnePole, OnePoleCoefficients, Svf, SvfCoefficients, butterworth_q, prewarp,
};
pub use level::{
    FOLD_DOWN_GAIN, apply_gain, db_to_gain, fold_to_mono, gain_to_db, pan_gains, pan_mono,
};
pub use noise::{Noise, Pink};
pub use oversample::{Factor, MAX_FACTOR as MAX_OVERSAMPLING, Oversampler};
pub use resample::{MAX_PHASES, Quality, Resampler};
