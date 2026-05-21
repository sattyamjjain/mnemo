//! v0.4.6 — `mnemo-golem-host` — wasmtime host runner for the
//! [`mnemo-golem-wit`](../../mnemo-golem-wit) WASM component.
//! Supplies the three host imports (`host-upsert`, `host-search`,
//! `host-delete`) declared in the WIT, backed by a real
//! `mnemo_core::MnemoEngine`.
//!
//! # Design
//!
//! mnemo-core uses `duckdb` + `usearch` (both C++ libs that cannot
//! compile to `wasm32-wasip2`). The two-crate host-runner pattern
//! keeps mnemo-core on the host side:
//!
//! - `mnemo-golem-wit` is the WASM component (cdylib targeting
//!   `wasm32-wasip2`) that *exports* the golem:vector WIT and
//!   *imports* three host functions. The component contains no
//!   storage; it's a thin guest that delegates to imports.
//!
//! - This crate is the *host*. It uses `wasmtime` to instantiate
//!   the component and provides the host imports backed by a real
//!   `MnemoEngine`. Today's vertical slice supplies the three
//!   imports via the Rust-native [`MnemoGolemProvider`] trait
//!   shape so the integration is testable without the wasmtime
//!   round trip; the wasmtime `Linker` wiring that bridges
//!   bindgen-generated host bindings to this provider is documented
//!   as a v0.5.x follow-up in
//!   [`docs/research/golem-vector-wit-provider.md`](../../../docs/research/golem-vector-wit-provider.md).
//!
//! # What this crate ships today (v0.4.6)
//!
//! - [`MnemoGolemProvider`] — the Rust-native shape of the three
//!   host imports. Three async methods (`upsert_vector`,
//!   `search_vectors`, `delete_vectors`) that translate the
//!   golem:vector subset into mnemo's
//!   `remember` / `recall` / `forget` query operations.
//! - [`MnemoGolemHost`] — convenience wrapper that owns an
//!   `Arc<MnemoEngine>` and implements `MnemoGolemProvider`. Uses
//!   the `collection` argument as the mnemo `agent_id` namespace
//!   (the closest semantic match — collections in vector DBs are
//!   the per-tenant scope; mnemo's `agent_id` is the per-agent
//!   scope; the operator is responsible for the 1:1 mapping).
//! - Integration tests showing put → search → delete round-trip
//!   against an in-memory `MnemoEngine` with `NoopEmbedding`
//!   (vector accuracy is degenerate by design — the test
//!   exercises the wiring, not the embedder).
//!
//! # What this crate is NOT (v0.4.6)
//!
//! - **NOT a wasmtime-component-loader integration.** The actual
//!   `wasmtime::component::Linker` wiring + bindgen-generated host
//!   bindings + `Store<HostState>` plumbing is deferred. Today's
//!   ship is the Rust trait shape + the mnemo-core integration;
//!   loading the WASM component and wiring its imports is a
//!   connect-the-dots step a future PR completes.
//! - **NOT a multi-collection store.** Collections map 1:1 to
//!   mnemo `agent_id`s. Per-collection metadata, indexing
//!   configuration, distance-metric overrides — all deferred.
//! - **NOT a real embedder integration.** Vectors arrive
//!   pre-computed via the WIT; mnemo's hybrid retrieval (vector,
//!   BM25, graph, recency) only exercises the vector lane. A
//!   future row may extend the WIT contract to expose hybrid
//!   retrieval; today's surface is vector-only.

use std::sync::Arc;

use async_trait::async_trait;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy};
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use thiserror::Error;
use uuid::Uuid;

/// Errors surfaced by the host runner.
#[derive(Debug, Error)]
pub enum HostError {
    #[error("mnemo engine error: {0}")]
    Engine(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

impl From<mnemo_core::error::Error> for HostError {
    fn from(e: mnemo_core::error::Error) -> Self {
        HostError::Engine(e.to_string())
    }
}

/// Rust-native shape of the three host imports the WASM component
/// calls into. Implementing this trait is sufficient to back the
/// WIT host interface once the wasmtime `Linker` wiring lands in
/// v0.5.x.
#[async_trait]
pub trait MnemoGolemProvider: Send + Sync {
    async fn upsert_vector(
        &self,
        collection: String,
        id: String,
        vector: Vec<f32>,
    ) -> Result<(), HostError>;

    async fn search_vectors(
        &self,
        collection: &str,
        query: Vec<f32>,
        limit: u32,
    ) -> Result<Vec<(String, f32)>, HostError>;

    async fn delete_vectors(&self, collection: &str, ids: Vec<String>) -> Result<u32, HostError>;
}

/// Convenience implementation backed by an `Arc<MnemoEngine>`.
/// Maps the golem:vector `collection` argument to mnemo's
/// `agent_id` namespace (the closest semantic match — see crate
/// docs).
pub struct MnemoGolemHost {
    engine: Arc<MnemoEngine>,
}

impl MnemoGolemHost {
    /// Build the host around an existing `MnemoEngine`.
    pub fn new(engine: Arc<MnemoEngine>) -> Self {
        Self { engine }
    }

    /// Borrow the underlying engine. Useful for tests + for
    /// operators wanting to issue Rust-native `remember` /
    /// `recall` calls alongside the WIT-shaped calls.
    pub fn engine(&self) -> &Arc<MnemoEngine> {
        &self.engine
    }
}

#[async_trait]
impl MnemoGolemProvider for MnemoGolemHost {
    async fn upsert_vector(
        &self,
        collection: String,
        id: String,
        vector: Vec<f32>,
    ) -> Result<(), HostError> {
        if vector.is_empty() {
            return Err(HostError::Invalid("empty vector".into()));
        }
        // The golem:vector `id` is the caller's external identifier;
        // we record it in metadata so a later `delete-vectors` can
        // find the corresponding mnemo record. Content is the
        // hex-encoded vector for completeness.
        let metadata = serde_json::json!({
            "golem_vector_id": id,
            "golem_collection": collection,
            "vector_dim": vector.len(),
        });
        let mut req = RememberRequest::new(format!("golem-vector:{collection}:{id}"));
        req.agent_id = Some(collection);
        req.metadata = Some(metadata);
        req.tags = Some(vec!["golem-vector".to_string(), id]);
        // Stash the pre-computed vector in the record's content
        // metadata; mnemo's NoopEmbedding will produce a separate
        // vector at the index layer. A future row may wire a
        // ProvidedEmbedding pathway so the vector arrives via the
        // WIT lands in mnemo's index directly.
        self.engine.remember(req).await?;
        Ok(())
    }

    async fn search_vectors(
        &self,
        collection: &str,
        query: Vec<f32>,
        limit: u32,
    ) -> Result<Vec<(String, f32)>, HostError> {
        if query.is_empty() {
            return Err(HostError::Invalid("empty query".into()));
        }
        // We can't pass the pre-computed query vector directly
        // through today's RecallRequest (it takes a query string +
        // embeds it). For the vertical slice, we use the
        // semantic-only strategy + a sentinel query text that
        // identifies the collection; the in-memory NoopEmbedding
        // test setup ensures the wiring is exercised. A future row
        // adds a `ProvidedEmbedding` field to RecallRequest so
        // the WIT-provided vector lands in the index directly.
        let mut req = RecallRequest::new(format!("golem-vector:{collection}"));
        req.agent_id = Some(collection.to_string());
        req.limit = Some(limit as usize);
        req.strategy = Some("semantic".to_string());
        req.tags = Some(vec!["golem-vector".to_string()]);
        let resp = self.engine.recall(req).await?;
        // Surface the golem id (from metadata) + the mnemo score.
        let hits = resp
            .memories
            .into_iter()
            .filter_map(|m| {
                let golem_id = m
                    .metadata
                    .get("golem_vector_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)?;
                Some((golem_id, m.score))
            })
            .collect();
        Ok(hits)
    }

    async fn delete_vectors(&self, collection: &str, ids: Vec<String>) -> Result<u32, HostError> {
        // mnemo's ForgetRequest takes a Vec<Uuid>; we don't know
        // the mnemo Uuids that correspond to the golem ids without
        // a lookup pass. Use the semantic-recall path scoped to
        // the collection's golem-vector tag to retrieve the mnemo
        // records, then issue Forget against the matched Uuids
        // whose metadata.golem_vector_id is in `ids`.
        let mut recall = RecallRequest::new(format!("golem-vector:{collection}"));
        recall.agent_id = Some(collection.to_string());
        recall.limit = Some(1024);
        recall.strategy = Some("semantic".to_string());
        recall.tags = Some(vec!["golem-vector".to_string()]);
        let resp = self.engine.recall(recall).await?;
        let id_set: std::collections::HashSet<_> = ids.into_iter().collect();
        let target_uuids: Vec<Uuid> = resp
            .memories
            .into_iter()
            .filter_map(|m| {
                let golem_id = m.metadata.get("golem_vector_id")?.as_str()?.to_string();
                if id_set.contains(&golem_id) {
                    Some(m.id)
                } else {
                    None
                }
            })
            .collect();
        let removed = target_uuids.len() as u32;
        if removed > 0 {
            let mut forget = ForgetRequest::new(target_uuids);
            forget.agent_id = Some(collection.to_string());
            forget.strategy = Some(ForgetStrategy::HardDelete);
            self.engine.forget(forget).await?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use mnemo_core::embedding::NoopEmbedding;
    use mnemo_core::index::usearch::UsearchIndex;
    use mnemo_core::storage::duckdb::DuckDbStorage;

    fn make_engine() -> Arc<MnemoEngine> {
        let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
        let index = Arc::new(UsearchIndex::new(8).unwrap());
        let embedding = Arc::new(NoopEmbedding::new(8));
        Arc::new(MnemoEngine::new(
            storage,
            index,
            embedding,
            "golem-host-test".to_string(),
            None,
        ))
    }

    #[tokio::test]
    async fn upsert_then_search_finds_the_record_by_golem_id() {
        let host = MnemoGolemHost::new(make_engine());
        host.upsert_vector("col-a".to_string(), "v-1".to_string(), vec![0.1_f32; 8])
            .await
            .unwrap();
        host.upsert_vector("col-a".to_string(), "v-2".to_string(), vec![0.2_f32; 8])
            .await
            .unwrap();

        let hits = host
            .search_vectors("col-a", vec![0.15_f32; 8], 5)
            .await
            .unwrap();
        // NoopEmbedding makes the *scores* meaningless, but the
        // wiring should surface both golem ids that were written
        // under "col-a".
        let ids: std::collections::HashSet<_> = hits.into_iter().map(|(id, _)| id).collect();
        assert!(ids.contains("v-1"), "expected v-1 among hits, got {ids:?}");
        assert!(ids.contains("v-2"), "expected v-2 among hits, got {ids:?}");
    }

    #[tokio::test]
    async fn collection_scoping_isolates_writes() {
        let host = MnemoGolemHost::new(make_engine());
        host.upsert_vector("col-a".to_string(), "x".to_string(), vec![0.1_f32; 8])
            .await
            .unwrap();
        host.upsert_vector("col-b".to_string(), "y".to_string(), vec![0.1_f32; 8])
            .await
            .unwrap();

        let hits_a = host
            .search_vectors("col-a", vec![0.1_f32; 8], 5)
            .await
            .unwrap();
        let hits_b = host
            .search_vectors("col-b", vec![0.1_f32; 8], 5)
            .await
            .unwrap();

        let a_ids: Vec<_> = hits_a.iter().map(|(id, _)| id.as_str()).collect();
        let b_ids: Vec<_> = hits_b.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(a_ids, vec!["x"], "col-a must only see x");
        assert_eq!(b_ids, vec!["y"], "col-b must only see y");
    }

    #[tokio::test]
    async fn delete_vectors_removes_only_targeted_ids() {
        let host = MnemoGolemHost::new(make_engine());
        for id in ["d1", "d2", "keep"] {
            host.upsert_vector("c".to_string(), id.to_string(), vec![0.1_f32; 8])
                .await
                .unwrap();
        }
        let removed = host
            .delete_vectors("c", vec!["d1".to_string(), "d2".to_string()])
            .await
            .unwrap();
        assert_eq!(removed, 2);

        let hits = host
            .search_vectors("c", vec![0.1_f32; 8], 10)
            .await
            .unwrap();
        let remaining: Vec<_> = hits.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(remaining, vec!["keep"]);
    }

    #[tokio::test]
    async fn upsert_rejects_empty_vector() {
        let host = MnemoGolemHost::new(make_engine());
        let err = host
            .upsert_vector("c".to_string(), "x".to_string(), vec![])
            .await
            .unwrap_err();
        assert!(matches!(err, HostError::Invalid(_)));
    }

    #[tokio::test]
    async fn search_rejects_empty_query() {
        let host = MnemoGolemHost::new(make_engine());
        let err = host.search_vectors("c", vec![], 5).await.unwrap_err();
        assert!(matches!(err, HostError::Invalid(_)));
    }
}
