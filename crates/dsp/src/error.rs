//! The crate's error type.

/// Why a resampler or FFT could not be made, or a transform was given the wrong buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A sample rate of zero.
    #[error("sample rates must be positive")]
    ZeroRate,
    /// The ratio between the rates needs more filter phases than [`crate::MAX_PHASES`].
    #[error("converting {from} Hz to {to} Hz needs {phases} filter phases, over the limit")]
    RatioTooComplex {
        /// The input rate.
        from: u32,
        /// The output rate.
        to: u32,
        /// The phases the exact ratio needs.
        phases: u64,
    },
    /// An FFT size that is not a power of two within the supported range.
    #[error("FFT size {0} is not a power of two from {min} to {max}", min = crate::fft::MIN_SIZE, max = crate::fft::MAX_SIZE)]
    InvalidFftSize(usize),
    /// A buffer of the wrong length for the transform.
    #[error("buffer of {found} values given where {expected} are needed")]
    LengthMismatch {
        /// The length the transform needs.
        expected: usize,
        /// The length given.
        found: usize,
    },
}
