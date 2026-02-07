use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use mnemo_core::error::Result;
use mnemo_core::index::VectorIndex;
use uuid::Uuid;

/// A no-op `VectorIndex` for use with PostgreSQL + pgvector.
///
/// Because pgvector handles vector storage and similarity search natively
/// inside PostgreSQL, there is no need for a separate in-process HNSW index.
/// All vector operations are performed via SQL in `PgStorage`.
///
/// The only meaningful state tracked here is the element count, maintained
/// with an `AtomicUsize` so callers can still query `len()` / `is_empty()`.
pub struct PgVectorIndex {
    count: AtomicUsize,
}

impl PgVectorIndex {
    /// Create a new no-op pgvector index wrapper.
    pub fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
        }
    }
}

impl Default for PgVectorIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorIndex for PgVectorIndex {
    fn add(&self, _id: Uuid, _vector: &[f32]) -> Result<()> {
        self.count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn remove(&self, _id: Uuid) -> Result<()> {
        // Saturating subtract: do not wrap below zero.
        let _ = self
            .count
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                if n > 0 { Some(n - 1) } else { Some(0) }
            });
        Ok(())
    }

    fn search(&self, _query: &[f32], _limit: usize) -> Result<Vec<(Uuid, f32)>> {
        // Vector search is handled via SQL in PgStorage.
        Ok(Vec::new())
    }

    fn filtered_search(
        &self,
        _query: &[f32],
        _limit: usize,
        _filter: &dyn Fn(Uuid) -> bool,
    ) -> Result<Vec<(Uuid, f32)>> {
        // Vector search is handled via SQL in PgStorage.
        Ok(Vec::new())
    }

    fn save(&self, _path: &Path) -> Result<()> {
        // No local state to persist -- vectors live in PostgreSQL.
        Ok(())
    }

    fn load(&self, _path: &Path) -> Result<()> {
        // No local state to restore -- vectors live in PostgreSQL.
        Ok(())
    }

    fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
}
