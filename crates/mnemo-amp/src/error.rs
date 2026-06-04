//! AMP adapter error type.

use thiserror::Error;

/// Errors surfaced by the AMP adapter surface.
#[derive(Debug, Error)]
pub enum AmpError {
    /// The envelope was malformed for its op (missing `content`,
    /// `query`, `memory_ids`, etc.).
    #[error("invalid AMP envelope: {0}")]
    Validation(String),

    /// A referenced memory id did not resolve.
    #[error("not found: {0}")]
    NotFound(String),

    /// The underlying [`MnemoEngine`](mnemo_core::query::MnemoEngine)
    /// returned an error.
    #[error("engine error: {0}")]
    Engine(#[from] mnemo_core::error::Error),
}
