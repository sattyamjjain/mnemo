//! PostgreSQL + pgvector backend for mnemo (`StorageBackend` + `VectorIndex`).
//!
//! **Capability note — semantic recall is NOT supported on this backend.**
//! Embeddings are persisted to the pgvector `vector` column and the HNSW index
//! is created, but ANN *search* is not yet wired: the synchronous
//! [`VectorIndex`](mnemo_core::index::VectorIndex) trait cannot run pgvector
//! SQL. So `semantic` / `auto` (hybrid) / `graph` / `domain_scoped` /
//! `reconstruct` recall on Postgres **fail loud** with a typed
//! [`Error::BackendUnsupported`](mnemo_core::error::Error::BackendUnsupported)
//! (`backend = "postgres"`, `capability = "semantic_recall"`) — never a silent
//! empty result. `lexical` / `exact` recall and all CRUD / ACL / checkpoint /
//! audit features work. Use the DuckDB backend for vector recall. Real
//! pgvector ANN is tracked at <https://github.com/sattyamjjain/mnemo/issues/99>.

pub mod migrations;
pub mod pgvector_index;
pub mod storage;

pub use pgvector_index::PgVectorIndex;
pub use storage::PgStorage;
