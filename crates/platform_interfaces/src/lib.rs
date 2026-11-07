//! The traits a platform implements, so that core and I/O crates never depend on platform code
//! (`docs/architecture/crates.md` "Dependency rules", `docs/architecture/platform.md`).
//!
//! - **Audio devices** ([`AudioDevices`], [`AudioStream`], [`AudioCallback`]): enumerate devices
//!   and run one duplex stream on one device. Input and output share that device's clock,
//!   because sample-rate conversion has no drift correction (ADR-0036).
//! - **Codecs** ([`Codecs`], [`Decoder`], [`Encoder`]): compressed formats, which go through the
//!   platform's licensed codecs (ADR-0024).
//!
//! Nothing here names a platform type. `macos_platform` implements these traits, and
//! [`test_doubles`] provides in-memory implementations for other crates' tests.
//!
//! # Modules
//!
//! - `device`: audio device traits and types.
//! - `codec`: codec traits and types.
//! - [`test_doubles`]: a fake device and fake codecs.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod codec;
mod device;
mod error;
pub mod test_doubles;

pub use codec::{CodecFormat, Codecs, Decoder, EncodeSettings, Encoder, FlacDepth, SourceInfo};
pub use device::{
    AudioCallback, AudioDevices, AudioStream, Buffers, DeviceId, DeviceInfo, Latency, StreamConfig,
};
pub use error::Error;
