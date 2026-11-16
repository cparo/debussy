//! The errors building an instrument node can return.

/// Why an instrument node could not be built.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    /// A parameter could not be made.
    #[error("invalid parameter: {0}")]
    Parameter(#[from] signal_graph::Error),
}
