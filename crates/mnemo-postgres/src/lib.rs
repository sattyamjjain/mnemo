//! PostgreSQL + pgvector backend for mnemo (`StorageBackend` + `VectorIndex`).
//!
//! **Capability note — semantic recall is supported (v0.5.7, #99).** Embeddings
//! are persisted to the pgvector `vector` column and, when [`PgVectorIndex`] is
//! constructed with a pool ([`PgVectorIndex::with_pool`]), `semantic` / `auto`
//! (hybrid) / `graph` / `domain_scoped` / `reconstruct` recall run a real
//! cosine-distance ANN query against the `idx_memories_embedding_hnsw` HNSW
//! index. The synchronous [`VectorIndex`](mnemo_core::index::VectorIndex) trait
//! is bridged to async `sqlx` with `block_in_place` + `Handle::block_on`, so the
//! Postgres vector path **requires the multi-threaded Tokio runtime** (the
//! CLI/server is `#[tokio::main]`). `filtered_search` uses the same
//! permission-safe oversample-then-filter as the USearch backend.
//!
//! If the pgvector extension / `<=>` operator is genuinely absent at runtime, or
//! the index is built without a pool, the vector path still **fails loud** with a
//! typed [`Error::BackendUnsupported`](mnemo_core::error::Error::BackendUnsupported)
//! (`backend = "postgres"`, `capability = "semantic_recall"`) — never a silent
//! empty result. The long-term async-`VectorIndex` refactor is tracked at
//! <https://github.com/sattyamjjain/mnemo/issues/99>.

pub mod migrations;
pub mod pgvector_index;
pub mod storage;

pub use pgvector_index::PgVectorIndex;
pub use storage::PgStorage;
