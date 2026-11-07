//! The errors building an effect node can return.

/// Why an effect node could not be built.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    /// A mixer was asked for no channels, or more than [`crate::MAX_CHANNELS`].
    #[error("a mixer has 1 to {max} channels, not {0}", max = crate::MAX_CHANNELS)]
    ChannelCount(usize),
    /// A parameter could not be made.
    #[error("invalid parameter: {0}")]
    Parameter(#[from] signal_graph::Error),
}
