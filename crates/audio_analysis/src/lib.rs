//! Waveform summaries and spectrogram data for stored audio, and the cache files that hold them
//! (`docs/architecture/analysis.md`).
//!
//! Everything here works on one mono channel of `f32` samples, as stored audio is
//! (`docs/architecture/project.md` "Stored audio"). Builders take the samples in a single streaming
//! pass, in chunks of any size, and write their cache file as they go, so their memory does not
//! grow with the file's length. Readers fetch only the range a view needs. File I/O is the
//! caller's: builders and readers work over `Write`, `Read` and `Seek`.
//!
//! # Modules
//!
//! - `waveform`: [`Bucket`], [`WaveformWriter`] and [`WaveformReader`], the waveform summary
//!   pyramid (`docs/architecture/analysis.md` "Waveform summaries").
//! - `spectrogram`: [`Spectrogram`], on-demand frames in dB, and [`OverviewWriter`] and
//!   [`OverviewReader`], the cached coarse overview (`docs/architecture/analysis.md`
//!   "Spectrograms").
//! - `header`: the little-endian fields both cache files start with
//!   (`docs/architecture/analysis.md` "Analysis cache files").
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod error;
mod header;
mod spectrogram;
mod waveform;

pub use error::Error;
pub use spectrogram::{
    BINS, FLOOR_DB, FRAME_SIZE, HOP, OVERVIEW_COLUMN, OverviewReader, OverviewWriter, Spectrogram,
    overview_db,
};
pub use waveform::{BASE_BUCKET, Bucket, LEVEL_FACTOR, WaveformReader, WaveformWriter};
