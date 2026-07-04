//! pgvector ANN integration test (#99).
//!
//! Proves that on the PostgreSQL backend `semantic` and `auto` (RRF hybrid)
//! recall return real top-k results from the pgvector HNSW index, in rank
//! order, and that a permission-scoped recall respects the filter (a nearer
//! but private record owned by another agent is excluded — which also exercises
//! the oversample-then-filter path).
//!
//! # Running
//!
//! Gated at runtime on `MNEMO_TEST_POSTGRES_URL`. Without it the test **skips
//! (passes)** so `cargo test --workspace` stays green with no database. With a
//! live pgvector Postgres:
//!
//! ```bash
//! # e.g. docker run -e POSTGRES_PASSWORD=pw -p 5432:5432 pgvector/pgvector:pg16
//! MNEMO_TEST_POSTGRES_URL=postgres://postgres:pw@localhost:5432/postgres \
//!   cargo test -p mnemo-postgres --test pgvector_ann -- --nocapture
//! ```
//!
//! The ANN bridge (`block_in_place` + `Handle::block_on`) requires a
//! multi-threaded runtime, hence `#[tokio::test(flavor = "multi_thread")]`.

use std::sync::Arc;

use async_trait::async_trait;
use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::Result as MnResult;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_postgres::{PgStorage, PgVectorIndex};

const DIM: usize = 4;
const AGENT_A: &str = "pgann-A";
const AGENT_B: &str = "pgann-B";

/// Deterministic `content -> vector` map, so the memories carry *known*
/// embeddings written through the real `remember` path and the query vector is
/// controlled. Cosine distances to the query `[0.8, 0.5, 0.2, 0]`:
/// `secret` (0.015) < `alpha` (0.170) < `beta` (0.481) < `gamma` (0.793`).
fn vec_for(text: &str) -> Vec<f32> {
    match text {
        "alpha" => vec![1.0, 0.0, 0.0, 0.0],
        "beta" => vec![0.0, 1.0, 0.0, 0.0],
        "gamma" => vec![0.0, 0.0, 1.0, 0.0],
        // Nearest of all to the query, but owned by AGENT_B and private.
        "secret" => vec![0.9, 0.4, 0.1, 0.0],
        "query" => vec![0.8, 0.5, 0.2, 0.0],
        _ => vec![0.0, 0.0, 0.0, 0.0],
    }
}

struct MapEmbedding;

#[async_trait]
impl EmbeddingProvider for MapEmbedding {
    async fn embed(&self, text: &str) -> MnResult<Vec<f32>> {
        Ok(vec_for(text))
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnResult<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| vec_for(t)).collect())
    }
    fn dimensions(&self) -> usize {
        DIM
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pgvector_ann_semantic_auto_and_permission_filter() {
    let Ok(url) = std::env::var("MNEMO_TEST_POSTGRES_URL") else {
        eprintln!(
            "skipping pgvector ANN test: set MNEMO_TEST_POSTGRES_URL=postgres://... \
             (needs the pgvector extension) to run it"
        );
        return;
    };

    let storage = Arc::new(
        PgStorage::connect(&url, DIM)
            .await
            .expect("connect + run migrations"),
    );

    // Best-effort clean of any prior run's rows so the fixture is deterministic.
    let _ = sqlx::query("DELETE FROM memories WHERE agent_id = ANY($1)")
        .bind(vec![AGENT_A.to_string(), AGENT_B.to_string()])
        .execute(&storage.pool())
        .await;

    let index = Arc::new(PgVectorIndex::with_pool(storage.pool(), DIM));
    let engine = Arc::new(MnemoEngine::new(
        storage.clone(),
        index,
        Arc::new(MapEmbedding),
        AGENT_A.to_string(),
        None,
    ));

    // 3 known-embedding memories for AGENT_A, written via the real remember
    // path (which persists the embedding into the pgvector column).
    for word in ["alpha", "beta", "gamma"] {
        engine
            .remember(RememberRequest::new(word.to_string()))
            .await
            .expect("remember");
    }
    // A *nearer* record than any of A's, but private and owned by AGENT_B.
    let mut secret = RememberRequest::new("secret".to_string());
    secret.agent_id = Some(AGENT_B.to_string());
    let secret_id = engine.remember(secret).await.expect("remember secret").id;

    // --- semantic: exact rank order alpha > beta > gamma. AGENT_B's nearer
    //     private record must be excluded (proves the permission filter AND
    //     that oversample looked past the top hit to fill the page).
    let mut req = RecallRequest::new("query".to_string());
    req.strategy = Some("semantic".to_string());
    req.limit = Some(5);
    let resp = engine.recall(req).await.expect("semantic recall");
    let contents: Vec<String> = resp.memories.iter().map(|m| m.content.clone()).collect();
    assert!(
        !contents.iter().any(|c| c == "secret"),
        "AGENT_B's private record must be filtered from AGENT_A's recall, got {contents:?}"
    );
    assert_eq!(
        contents,
        vec!["alpha", "beta", "gamma"],
        "semantic recall must return the nearest in rank order"
    );

    // --- auto (RRF hybrid; vector-only fallback with no full-text index):
    //     the nearest accessible record is still first.
    let mut areq = RecallRequest::new("query".to_string());
    areq.strategy = Some("auto".to_string());
    areq.limit = Some(3);
    let aresp = engine.recall(areq).await.expect("auto recall");
    assert_eq!(
        aresp.memories.first().map(|m| m.content.as_str()),
        Some("alpha"),
        "auto recall must rank the nearest first"
    );

    // --- AGENT_B can see its own private record (filter is per-owner, not a
    //     blanket exclusion).
    let mut breq = RecallRequest::new("query".to_string());
    breq.strategy = Some("semantic".to_string());
    breq.agent_id = Some(AGENT_B.to_string());
    breq.limit = Some(5);
    let bresp = engine.recall(breq).await.expect("agent B recall");
    assert!(
        bresp.memories.iter().any(|m| m.id == secret_id),
        "AGENT_B must see its own private record"
    );
}
