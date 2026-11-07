//! The crate's error type.

use std::path::PathBuf;

/// Why an import or export failed. A failed import leaves no files behind.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file operation failed.
    #[error("{path}: {source}")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// What went wrong.
        source: std::io::Error,
    },
    /// A WAV file could not be read or written.
    #[error("{path}: {source}")]
    Wav {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: wav_files::Error,
    },
    /// The platform codec failed or refused the file.
    #[error("{path}: {source}")]
    Codec {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: platform_interfaces::Error,
    },
    /// The file's sample rate cannot be converted to the project's.
    #[error("cannot convert {from} Hz to {to} Hz: {source}")]
    Resample {
        /// The file's rate.
        from: u32,
        /// The project rate.
        to: u32,
        /// Why.
        source: dsp::Error,
    },
    /// A decoded file has a channel count or rate that cannot be imported.
    #[error("{path}: {reason}")]
    Unsupported {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        reason: String,
    },
    /// The offline render failed.
    #[error(transparent)]
    Render(#[from] audio_engine::Error),
}
