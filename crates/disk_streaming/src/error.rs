//! The crate's error type.

/// Why streaming could not start or a request was refused.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be opened.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A WAV file could not be read.
    #[error(transparent)]
    Wav(#[from] wav_files::Error),
    /// Stored audio is mono (ADR-0014); this file is not.
    #[error("stored audio must be mono")]
    NotMono,
    /// A source's sample rate differs from the streamer's.
    #[error("source is at {found} Hz, the project at {expected} Hz")]
    SampleRateMismatch {
        /// The project rate.
        expected: u32,
        /// The source's rate.
        found: u32,
    },
    /// No stream has this index.
    #[error("no stream {0}")]
    UnknownStream(usize),
    /// A plan's start or loop region is outside the source.
    #[error("plan is outside the source ({frames} frames)")]
    PlanOutOfRange {
        /// The source's length.
        frames: u64,
    },
    /// The reader has not yet taken earlier requests; try again shortly.
    #[error("the reader is busy; try again")]
    Busy,
    /// The reader thread could not be started.
    #[error("could not start the reader thread: {0}")]
    Thread(std::io::Error),
    /// Rendering a track's clips failed.
    #[error(transparent)]
    Engine(#[from] audio_engine::Error),
    /// A queue or pool could not be created.
    #[error(transparent)]
    Buffers(#[from] audio_buffers::Error),
}
