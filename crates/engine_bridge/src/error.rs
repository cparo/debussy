//! The crate's error type.

/// Why a change could not reach the graph, or the project could not be prepared to play.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The model refused the change.
    #[error(transparent)]
    Model(#[from] project_model::Error),
    /// The processing graph refused the routing: a cycle without a feedback delay, a port the
    /// node does not have, an out-of-range delay, or similar. The model is unchanged.
    #[error("the routing was refused: {0}")]
    Refused(signal_graph::Error),
    /// The graph and the model disagree about an element that should exist; a bug, not a user
    /// error.
    #[error("the routing names an element the graph does not have")]
    Inconsistent,
    /// The graph failed for another reason.
    #[error(transparent)]
    Graph(signal_graph::Error),
    /// A clip could not be prepared for the engine.
    #[error(transparent)]
    Engine(#[from] audio_engine::Error),
}
