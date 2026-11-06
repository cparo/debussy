//! WAV and RF64 reading and writing over [`std::io::Read`], [`std::io::Write`] and
//! [`std::io::Seek`], shared by project audio storage, import and export
//! (`docs/architecture/import-export.md` "WAV files", `docs/architecture/project.md` "Stored audio").
//!
//! Every file read is treated as untrusted: the limits in ADR-0029 bound every size the reader
//! acts on, and no allocation is sized from a header field.
//!
//! # Modules
//!
//! - `format`: [`Spec`], [`Channels`] and [`SampleFormat`], and sample encoding and decoding.
//! - `header`: parsing the chunks before the samples.
//! - `reader`: [`WavReader`], streaming reads of any frame range.
//! - `writer`: [`WavWriter`], which switches to RF64 when the file passes 4 GB.
//! - `error`: [`Error`].
//!
//! Wrap files in [`std::io::BufReader`] or [`std::io::BufWriter`]: the reader and writer make
//! one call per chunk of up to 64 KiB, and the header is read in small pieces.

#![forbid(unsafe_code)]

mod error;
mod format;
mod header;
mod reader;
mod writer;

pub use error::{ChunkId, Error};
pub use format::{Channels, MAX_SAMPLE_RATE, SampleFormat, Spec};
pub use reader::WavReader;
pub use writer::WavWriter;
