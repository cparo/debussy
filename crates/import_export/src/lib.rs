//! Import and export (`docs/architecture/import-export.md`).
//!
//! - [`import`] decodes a WAV (in-house) or compressed file (through
//!   [`Codecs`](platform_interfaces::Codecs)), converts it to the project rate, and stores each
//!   channel as a mono 32-bit float WAV in the bundle's `audio/` (ADR-0014, ADR-0053).
//! - [`export`] runs an offline render ([`OfflineRender`]), folds to mono if asked, dithers to an
//!   integer depth (ADR-0025), and writes WAV in-house or FLAC or AAC through the codecs.
//!
//! Neither runs on the real-time thread.
//!
//! # Modules
//!
//! - `import`: [`import`], [`Imported`], [`ImportFormat`].
//! - `export`: [`export`], [`ExportFormat`], [`WavDepth`], [`ExportChannels`], [`OfflineRender`].
//! - `naming`: names for imported files.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod error;
mod export;
mod import;
mod naming;

pub use error::Error;
pub use export::{ExportChannels, ExportFormat, OfflineRender, WavDepth, export};
pub use import::{ImportFormat, Imported, import};
