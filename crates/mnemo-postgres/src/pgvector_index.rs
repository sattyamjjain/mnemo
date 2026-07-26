use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use mnemo_core::error::{Error, Result};
use mnemo_core::index::VectorIndex;
use pgvector::Vector;
use sqlx::Row;
use uuid::Uuid;

/// A pgvector-backed [`VectorIndex`] for the PostgreSQL backend.
///
/// PostgreSQL stores each memory's embedding in a pgvector `vector` column
/// (written by [`crate::PgStorage`]) and an HNSW index over it
/// (`idx_memories_embedding_hnsw`, built with `vector_cosine_ops`). When
/// constructed with a pool via [`PgVectorIndex::with_pool`], `search` /
/// `filtered_search` run a real cosine-distance ANN query (`<=>`) against that
/// index and return the top-k memory ids + distances — the same
/// `(id, distance)` shape the USearch backend returns, so recall's
/// `score = 1.0 - distance` conversion is identical across backends.
///
/// Constructed with [`PgVectorIndex::new`] (no pool) the ANN paths return the
/// typed [`Error::BackendUnsupported`] rather than a silent empty set —
/// silent-empty would make `semantic` / `auto` (hybrid) / `graph` /
/// `domain_scoped` recall look like it legitimately "found nothing", the most
/// dangerous failure mode for a memory database.
///
/// ## Runtime (v0.5.18)
///
/// [`VectorIndex::search`] / [`filtered_search`](VectorIndex::filtered_search)
/// are **async**, so this backend `.await`s its `sqlx` query directly on the
/// caller's ambient Tokio runtime. There is **no `block_on` bridge** — semantic
/// recall works from inside the server/CLI `#[tokio::main]` runtime (any flavor,
/// single- or multi-threaded) without the "Cannot start a runtime from within a
/// runtime" panic or deadlock the old synchronous bridge risked. Integration
/// tests run under `#[tokio::test]` / `#[tokio::test(flavor = "multi_thread")]`
/// alike.
///
/// `add` / `remove` are intentional no-ops: the embedding is maintained by
/// PostgreSQL on the `vector` column (via `PgStorage::insert_memory`), not by
/// an in-process index. `len()` tracks an approximate element count for
/// `is_empty()` callers.
pub struct PgVectorIndex {
    /// When `Some`, ANN search runs real pgvector SQL against this pool. When
    /// `None`, the ANN paths fail loud with [`Error::BackendUnsupported`].
    pool: Option<sqlx::PgPool>,
    /// Width of the pgvector `vector(dim)` column; used to reject a
    /// dimension-mismatched query with a clear message instead of a raw
    /// Postgres error.
    dimensions: usize,
    count: AtomicUsize,
}

impl PgVectorIndex {
    /// Create a pgvector index wrapper **without** a pool. The ANN search
    /// paths return [`Error::BackendUnsupported`] (fail loud, never
    /// silent-empty). Prefer [`PgVectorIndex::with_pool`] for a wired backend.
    pub fn new() -> Self {
        Self {
            pool: None,
            dimensions: 0,
            count: AtomicUsize::new(0),
        }
    }

    /// Create a pgvector index that runs real ANN search against `pool`.
    ///
    /// `dimensions` must match the `vector(dim)` column width the schema was
    /// migrated with (the same value passed to `PgStorage::connect`).
    pub fn with_pool(pool: sqlx::PgPool, dimensions: usize) -> Self {
        Self {
            pool: Some(pool),
            dimensions,
            count: AtomicUsize::new(0),
        }
    }

    /// The cosine-distance ANN query against the HNSW index. Returns up to
    /// `limit` `(id, distance)` rows, nearest first. `$1` (the query vector) is
    /// referenced twice — once for the projected distance, once for the
    /// index-ordered `ORDER BY` — from a single bind.
    async fn ann_query(
        pool: &sqlx::PgPool,
        query: &Vector,
        limit: usize,
    ) -> Result<Vec<(Uuid, f32)>> {
        let rows = sqlx::query(
            "SELECT id, (embedding <=> $1) AS dist \
             FROM memories \
             WHERE embedding IS NOT NULL AND deleted_at IS NULL \
             ORDER BY embedding <=> $1 \
             LIMIT $2",
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(pool)
        .await
        .map_err(map_ann_error)?;

        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: Uuid = row.try_get("id").map_err(|e| Error::Index(e.to_string()))?;
            let dist: f64 = row
                .try_get("dist")
                .map_err(|e| Error::Index(e.to_string()))?;
            out.push((id, dist as f32));
        }
        Ok(out)
    }

    /// Validate the query dimension and resolve the pool, or fail loud.
    fn pool_for(&self, query: &[f32]) -> Result<&sqlx::PgPool> {
        let pool = self.pool.as_ref().ok_or_else(ann_unsupported)?;
        if self.dimensions != 0 && query.len() != self.dimensions {
            return Err(Error::Index(format!(
                "query embedding has {} dims but the pgvector column is {} — \
                 re-embed with the configured model",
                query.len(),
                self.dimensions
            )));
        }
        Ok(pool)
    }
}

impl Default for PgVectorIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// The typed error returned when ANN search is genuinely unavailable — the
/// index was constructed without a pool, or the pgvector extension / operator
/// is absent at runtime. Uses the structured [`Error::BackendUnsupported`]
/// variant so callers can match on `backend` / `capability` programmatically
/// instead of string-sniffing the message.
fn ann_unsupported() -> Error {
    Error::BackendUnsupported {
        backend: "postgres".to_string(),
        capability: "semantic_recall".to_string(),
        detail: "pgvector ANN search is unavailable: the index has no database \
                 pool, or the pgvector extension / `<=>` operator is not \
                 installed. Ensure the `vector` extension and the \
                 `idx_memories_embedding_hnsw` index exist (created by \
                 migrations), or use strategy=\"lexical\"/\"exact\". \
                 Tracking: https://github.com/sattyamjjain/mnemo/issues/99"
            .to_string(),
    }
}

/// Map a query error: a missing pgvector extension / `<=>` operator / `vector`
/// type is a *capability-absent* condition → typed [`Error::BackendUnsupported`];
/// anything else is a real, loud [`Error::Index`]. Never silent-empty.
fn map_ann_error(e: sqlx::Error) -> Error {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    let capability_absent = (lower.contains("operator does not exist") && lower.contains("<=>"))
        || lower.contains("type \"vector\" does not exist")
        || lower.contains("extension \"vector\"");
    if capability_absent {
        ann_unsupported()
    } else {
        Error::Index(format!("pgvector ANN query failed: {msg}"))
    }
}

#[async_trait::async_trait]
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

    // Truly async: awaits the pgvector `sqlx` query on the ambient runtime, so
    // it can never re-enter or deadlock the caller's `#[tokio::main]` runtime.
    async fn search(&self, query: &[f32], limit: usize) -> Result<Vec<(Uuid, f32)>> {
        let pool = self.pool_for(query)?;
        let vec = Vector::from(query.to_vec());
        Self::ann_query(pool, &vec, limit).await
    }

    async fn filtered_search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &(dyn Fn(Uuid) -> bool + Send + Sync),
    ) -> Result<Vec<(Uuid, f32)>> {
        let pool = self.pool_for(query)?;
        let vec = Vector::from(query.to_vec());
        if limit == 0 {
            return Ok(Vec::new());
        }

        // Permission-safe iterative oversample: start at 3x, double until we
        // have `limit` accessible hits or the underlying table is exhausted
        // (the ANN query returned fewer rows than we asked for). Mirrors the
        // USearch backend so filtered recall never under-returns.
        let mut oversample = limit.saturating_mul(3).max(1);
        loop {
            let candidates = Self::ann_query(pool, &vec, oversample).await?;
            let exhausted = candidates.len() < oversample;
            let filtered: Vec<(Uuid, f32)> = candidates
                .into_iter()
                .filter(|(id, _)| filter(*id))
                .take(limit)
                .collect();
            if filtered.len() >= limit || exhausted {
                return Ok(filtered);
            }
            oversample = oversample.saturating_mul(2);
        }
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

    #[tokio::test]
    async fn ann_search_fails_loud_not_silent_empty() {
        // Constructed without a pool: ANN is genuinely unavailable and MUST
        // fail loud with the typed variant, never Ok(empty).
        let idx = PgVectorIndex::new();

        // add/remove remain no-ops that maintain the approximate count.
        idx.add(Uuid::nil(), &[0.1, 0.2, 0.3]).unwrap();
        assert_eq!(idx.len(), 1);
        idx.remove(Uuid::nil()).unwrap();
        assert_eq!(idx.len(), 0);

        assert!(
            idx.search(&[0.1, 0.2, 0.3], 5).await.is_err(),
            "search must fail loud, not return Ok(empty)"
        );
        assert!(
            idx.filtered_search(&[0.1, 0.2, 0.3], 5, &|_| true)
                .await
                .is_err(),
            "filtered_search must fail loud, not return Ok(empty)"
        );

        // It must be the structured, typed variant — callers match on
        // backend/capability, not the message string.
        match idx.search(&[0.0], 1).await.unwrap_err() {
            Error::BackendUnsupported {
                backend,
                capability,
                detail,
            } => {
                assert_eq!(backend, "postgres");
                assert_eq!(capability, "semantic_recall");
                assert!(
                    detail.contains("issues/99"),
                    "detail should reference the tracking issue: {detail}"
                );
            }
            other => panic!("expected BackendUnsupported, got: {other}"),
        }
    }

    #[tokio::test]
    async fn dimension_mismatch_is_loud() {
        // A pool-less index can't reach the dim check, but we can assert the
        // helper's contract via a constructed-with-dims instance is not
        // reachable without a live pool; the no-pool path already errors.
        // (Live-pool dimension + ANN behaviour is covered by the
        // MNEMO_TEST_POSTGRES_URL integration test.)
        let idx = PgVectorIndex::new();
        assert!(idx.search(&[0.1; 4], 3).await.is_err());
    }
}
