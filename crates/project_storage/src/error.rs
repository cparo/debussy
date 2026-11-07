//! The crate's error type.

use std::path::PathBuf;

/// Why a bundle could not be read, written or cleaned up.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file or directory operation failed.
    #[error("{path}: {source}")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// What went wrong.
        source: std::io::Error,
    },
    /// A file is not valid JSON of the saved format.
    #[error("{path}: {source}")]
    Json {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: serde_json::Error,
    },
    /// The file was written by a newer Debussy.
    #[error("{path} is format version {found}; this build reads up to {supported}")]
    NewerFormat {
        /// The file.
        path: PathBuf,
        /// Its format version.
        found: u32,
        /// The newest this build reads.
        supported: u32,
    },
    /// The file is over a load limit: too large, too deeply nested, or holding too many items.
    #[error("{path}: {what} is over the limit of {limit}")]
    TooLarge {
        /// The file.
        path: PathBuf,
        /// What is over the limit.
        what: &'static str,
        /// The limit.
        limit: u64,
    },
    /// The file parses but describes an invalid project.
    #[error("{path}: {reason}")]
    Invalid {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        reason: String,
    },
    /// A bundle already exists where a new one was to be made.
    #[error("{0} already exists")]
    Exists(PathBuf),
    /// A sample rate other than 44.1, 48, 88.2 or 96 kHz (`docs/architecture/engine.md` "Sample
    /// rate").
    #[error("{0} Hz is not a project sample rate")]
    SampleRate(u32),
}
