pub mod usearch;

use crate::error::Result;
use uuid::Uuid;

/// A pluggable approximate-nearest-neighbour index over memory embeddings.
///
/// `search` / `filtered_search` are **async** (v0.5.18). The PostgreSQL
/// (pgvector) backend runs a real `sqlx` query on the *ambient* Tokio runtime,
/// so the read path `.await`s it directly instead of bridging through a
/// `block_on` — which, inside the server/CLI `#[tokio::main]` runtime, could
/// panic ("Cannot start a runtime from within a runtime") or deadlock. The
/// in-memory USearch backend does synchronous CPU work inside its async method,
/// so it needs no runtime and assumes no runtime flavor. `add` / `remove` /
/// `save` / `load` / `len` remain synchronous.
///
/// The `filter` for `filtered_search` must be `Send + Sync` because it is held
/// across the `.await` inside the resulting `Send` future.
#[async_trait::async_trait]
pub trait VectorIndex: Send + Sync {
    fn add(&self, id: Uuid, vector: &[f32]) -> Result<()>;
    fn remove(&self, id: Uuid) -> Result<()>;
    async fn search(&self, query: &[f32], limit: usize) -> Result<Vec<(Uuid, f32)>>;
    async fn filtered_search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &(dyn Fn(Uuid) -> bool + Send + Sync),
    ) -> Result<Vec<(Uuid, f32)>>;
    fn save(&self, path: &std::path::Path) -> Result<()>;
    fn load(&self, path: &std::path::Path) -> Result<()>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
