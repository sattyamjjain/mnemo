use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::model::TemporalEdge;

pub mod duckdb;

#[derive(Debug, Error)]
pub enum Error {
    #[error("graph store: {0}")]
    Store(String),
    #[error("graph store: serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl From<::duckdb::Error> for Error {
    fn from(e: ::duckdb::Error) -> Self {
        Error::Store(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Persistent home for [`TemporalEdge`] rows.
///
/// Today the only impl is [`duckdb::DuckGraphStore`]. The trait stays
/// minimal on purpose — we add methods only when retrieval needs them
/// rather than guessing.
#[async_trait]
pub trait GraphStore: Send + Sync {
    /// Persist `edge` (or upsert if `edge.id` already exists).
    async fn insert_edge(&self, edge: &TemporalEdge) -> Result<()>;

    /// Close the validity window of `edge_id` at `closed_at` — i.e.
    /// "as of this moment we no longer believe the relation is true".
    /// Idempotent: closing an already-closed edge no-ops.
    async fn close_edge(&self, edge_id: Uuid, closed_at: DateTime<Utc>) -> Result<()>;

    /// Edges leaving `node` that are valid at `as_of`. Used by the
    /// BFS in [`crate::graph_expand`].
    async fn outgoing_at(&self, node: Uuid, as_of: DateTime<Utc>) -> Result<Vec<TemporalEdge>>;

    /// Every edge in the store — for tests and admin tooling. Not
    /// suitable for hot-path retrieval; production walkers should
    /// stick to [`outgoing_at`].
    async fn all_edges(&self) -> Result<Vec<TemporalEdge>>;
}
