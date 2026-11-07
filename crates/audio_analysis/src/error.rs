//! The crate's error type.

/// Why a summary could not be built, a cache file could not be read, or a request was invalid.
///
/// A cache file that fails with [`Error::WrongMagic`], [`Error::UnsupportedVersion`],
/// [`Error::ParameterMismatch`] or [`Error::WrongLength`] should be discarded and rebuilt
/// (`docs/architecture/analysis.md` "Analysis cache files").
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading or writing the cache file failed.
    #[error("cache file I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// The transform could not be made or was given the wrong buffers.
    #[error(transparent)]
    Dsp(#[from] dsp::Error),
    /// The file does not start with the expected magic.
    #[error("not a {0} cache file")]
    WrongMagic(&'static str),
    /// The file's format version is not the one this build writes.
    #[error("unsupported cache format version {0}")]
    UnsupportedVersion(u32),
    /// The file was made with different analysis parameters.
    #[error("cache file has different analysis parameters")]
    ParameterMismatch,
    /// The file's length does not match its header.
    #[error("cache file is {found} bytes where its header implies {expected}")]
    WrongLength {
        /// The length the header implies.
        expected: u64,
        /// The actual length.
        found: u64,
    },
    /// A builder was given a different number of samples than it was created for.
    #[error("{found} samples given where {expected} were declared")]
    SampleCount {
        /// The count declared when the builder was made.
        expected: u64,
        /// The count given, or the count reached when it went over.
        found: u64,
    },
    /// A waveform level that the summary does not have.
    #[error("level {level} requested from a summary of {levels} levels")]
    LevelOutOfRange {
        /// The level asked for.
        level: usize,
        /// The number of levels.
        levels: usize,
    },
    /// Columns narrower than [`crate::BASE_BUCKET`] samples, which must be drawn from the audio.
    #[error("{0} samples per column is finer than the summary; read the audio instead")]
    TooZoomedIn(u64),
    /// A buffer of the wrong length.
    #[error("buffer of {found} values given where {expected} are needed")]
    BufferLength {
        /// The length needed (for overview columns, a multiple of this).
        expected: usize,
        /// The length given.
        found: usize,
    },
}
