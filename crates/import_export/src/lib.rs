//! Import and export (`docs/architecture/import-export.md`).
//!
//! - [`import`] decodes a WAV (in-house) or compressed file (through
//!   [`Codecs`](platform_interfaces::Codecs)), converts it to the project rate, and stores each
//!   channel as a mono 32-bit float WAV in the bundle's `audio/` (ADR-0014, ADR-0053).
//! - [`export`] runs an offline render ([`OfflineRender`]), folds to mono if asked, dithers to an
//!   integer depth (ADR-0025), and writes WAV in-house, FLAC or AAC through the codecs, or MP3
//!   through a host-installed `lame` process ([`Lame`], ADR-0129).
//!
//! Neither runs on the real-time thread.
//!
//! # Modules
//!
//! - `import`: [`import`], [`Imported`], [`ImportFormat`].
//! - `export`: [`export`], [`ExportFormat`], [`WavDepth`], [`Mp3Bitrate`], [`ExportChannels`],
//!   [`OfflineRender`].
//! - `lame`: [`Lame`], found by probing, and its [`LameEncoder`].
//! - `naming`: names for imported and recorded files: [`safe_stem`] and [`unique_names`].
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod error;
mod export;
mod import;
mod lame;
mod naming;

pub use error::Error;
pub use export::{ExportChannels, ExportFormat, Mp3Bitrate, OfflineRender, WavDepth, export};
pub use import::{ImportFormat, Imported, import};
pub use lame::{Lame, LameEncoder};
pub use naming::{safe_stem, unique_names};
