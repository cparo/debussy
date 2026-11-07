//! The crate's error type.

/// Why a [`crate::Resampler`] could not be made.
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
}
