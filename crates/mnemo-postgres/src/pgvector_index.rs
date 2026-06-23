use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use mnemo_core::error::{Error, Result};
use mnemo_core::index::VectorIndex;
use uuid::Uuid;

/// A pgvector-backed [`VectorIndex`] placeholder for the PostgreSQL backend.
///
/// PostgreSQL stores each memory's embedding in a pgvector `vector` column
/// (written by [`crate::PgStorage`]) and an HNSW index
/// (`idx_memories_embedding_hnsw`) is created over it. **ANN *search* is not
/// yet wired**, however: the [`VectorIndex`] trait is synchronous and this
/// type holds no database handle, so it cannot execute the pgvector SQL a
/// real search requires.
///
/// Consequently `search` / `filtered_search` **return an error** instead of
/// silently returning an empty result set — silent-empty would make
/// `semantic` / `auto` (hybrid) / `graph` / `domain_scoped` recall look like
/// it legitimately "found nothing", which is the most dangerous failure
/// mode for a memory database. Use the embedded **DuckDB** backend for
/// vector recall, or `strategy = "lexical"` / `"exact"` on Postgres.
/// Implementing real pgvector ANN is tracked in
/// <https://github.com/sattyamjjain/mnemo/issues/99>.
///
/// `add` / `remove` are intentional no-ops: the embedding is maintained by
/// PostgreSQL on the `vector` column, not by an in-process index. `len()`
/// tracks an approximate element count for `is_empty()` callers.
pub struct PgVectorIndex {
    count: AtomicUsize,
}

impl PgVectorIndex {
    /// Create a new pgvector index wrapper.
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

/// The typed error returned by the not-yet-implemented ANN search paths.
/// Centralised so the backend/capability/detail stay consistent. Uses the
/// structured [`Error::BackendUnsupported`] variant so callers can match on
/// `backend` / `capability` programmatically instead of string-sniffing the
/// message.
fn ann_unsupported() -> Error {
    Error::BackendUnsupported {
        backend: "postgres".to_string(),
        capability: "semantic_recall".to_string(),
        detail: "pgvector ANN search is not implemented: semantic / auto \
                 (hybrid) / graph / domain-scoped recall are unsupported on \
                 the PostgreSQL backend. Embeddings are persisted to the \
                 pgvector column, but the synchronous VectorIndex trait \
                 cannot run pgvector SQL. Use the embedded DuckDB backend for \
                 vector recall, or strategy=\"lexical\"/\"exact\" on Postgres. \
                 Tracking: https://github.com/sattyamjjain/mnemo/issues/99"
            .to_string(),
    }
}

impl VectorIndex for PgVectorIndex {
    fn add(&self, _id: Uuid, _vector: &[f32]) -> Result<()> {
        // No-op: PostgreSQL maintains the embedding on the `vector` column.
        self.count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn remove(&self, _id: Uuid) -> Result<()> {
        let _ = self
            .count
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_sub(1))
            });
        Ok(())
    }

    fn search(&self, _query: &[f32], _limit: usize) -> Result<Vec<(Uuid, f32)>> {
        // Fail loud rather than silently returning an empty result set.
        Err(ann_unsupported())
    }

    fn filtered_search(
        &self,
        _query: &[f32],
        _limit: usize,
        _filter: &dyn Fn(Uuid) -> bool,
    ) -> Result<Vec<(Uuid, f32)>> {
        Err(ann_unsupported())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ann_search_fails_loud_not_silent_empty() {
        let idx = PgVectorIndex::new();

        // add/remove remain no-ops that maintain the approximate count.
        idx.add(Uuid::nil(), &[0.1, 0.2, 0.3]).unwrap();
        assert_eq!(idx.len(), 1);
        idx.remove(Uuid::nil()).unwrap();
        assert_eq!(idx.len(), 0);

        // Both ANN paths MUST error, not return Ok(empty) — silent-empty is
        // the exact bug this guards against.
        assert!(
            idx.search(&[0.1, 0.2, 0.3], 5).is_err(),
            "search must fail loud, not return Ok(empty)"
        );
        assert!(
            idx.filtered_search(&[0.1, 0.2, 0.3], 5, &|_| true).is_err(),
            "filtered_search must fail loud, not return Ok(empty)"
        );

        // It must be the structured, typed variant — callers match on
        // backend/capability, not the message string.
        match idx.search(&[0.0], 1).unwrap_err() {
            Error::BackendUnsupported {
                backend,
                capability,
                detail,
            } => {
                assert_eq!(backend, "postgres");
                assert_eq!(capability, "semantic_recall");
                // detail still names the tracking issue so operators find the
                // path forward.
                assert!(
                    detail.contains("issues/99"),
                    "detail should reference the tracking issue: {detail}"
                );
            }
            other => panic!("expected BackendUnsupported, got: {other}"),
        }
    }
}
