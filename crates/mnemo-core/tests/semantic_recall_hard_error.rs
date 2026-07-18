//! Semantic recall must **fail loud, never silent-empty** when no real embedder
//! is configured (v0.5.13).
//!
//! With the no-op embedder every query embeds to an all-zero vector, so a
//! semantic / hybrid / auto recall would silently return an empty or meaningless
//! result set. The recall path refuses these with a typed
//! [`Error::EmbedderNotConfigured`] instead. Lexical (BM25) and exact recall
//! need no embedder and keep working. A real (here: deterministic, offline)
//! embedder makes semantic recall return results again.
//!
//! Derived from mnemo's OWN recall path (`crates/mnemo-core/src/query/recall.rs`).

use std::sync::Arc;

use async_trait::async_trait;

use mnemo_core::embedding::{EmbeddingProvider, NoopEmbedding};
use mnemo_core::error::{Error, Result as MnemoResult};
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;

const AGENT: &str = "semantic-hard-error-agent";
const DIM: usize = 64;

/// Deterministic bag-of-words hashing embedder — real, non-zero vectors, no
/// external model. Inherits `is_semantic_capable() == true` (the default), so it
/// stands in for OpenAI/ONNX in the "real embedder" case.
struct HashEmbedding;

fn fnv1a(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn embed_text(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; DIM];
    for tok in text.split_whitespace() {
        let idx = (fnv1a(tok) as usize) % DIM;
        v[idx] += 1.0;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

#[async_trait]
impl EmbeddingProvider for HashEmbedding {
    async fn embed(&self, text: &str) -> MnemoResult<Vec<f32>> {
        Ok(embed_text(text))
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnemoResult<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| embed_text(t)).collect())
    }
    fn dimensions(&self) -> usize {
        DIM
    }
}

fn engine_with(embedding: Arc<dyn EmbeddingProvider>) -> MnemoEngine {
    let storage = Arc::new(
        mnemo_core::storage::duckdb::DuckDbStorage::open_in_memory().expect("in-memory duckdb"),
    );
    let index = Arc::new(UsearchIndex::new(DIM).expect("usearch index"));
    let full_text = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy"));
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(full_text)
}

async fn seed(engine: &MnemoEngine) {
    for content in [
        "the mitochondria is the powerhouse of the cell",
        "clinician adjusted the dosage to five milligrams",
        "quarterly revenue rose on strong subscription growth",
    ] {
        engine
            .remember(RememberRequest::new(content.to_string()))
            .await
            .expect("remember");
    }
}

/// (a) Semantic recall with the no-op embedder returns the typed error — never a
/// silent empty set.
#[tokio::test]
async fn semantic_recall_with_noop_embedder_hard_errors() {
    let engine = engine_with(Arc::new(NoopEmbedding::new(DIM)));
    seed(&engine).await;

    let mut req = RecallRequest::new("cell biology".to_string());
    req.strategy = Some("semantic".to_string());

    match engine.recall(req).await {
        Err(Error::EmbedderNotConfigured { requested, backend }) => {
            assert_eq!(requested, "semantic");
            assert_eq!(backend, "duckdb");
        }
        Err(other) => panic!("expected EmbedderNotConfigured, got a different error: {other}"),
        Ok(resp) => panic!(
            "expected EmbedderNotConfigured, got {} silent results",
            resp.total
        ),
    }
}

/// The default `auto` strategy (and the hybrid RRF path) also refuse the no-op
/// embedder — its semantic leg cannot run.
#[tokio::test]
async fn auto_and_hybrid_recall_with_noop_embedder_hard_error() {
    let engine = engine_with(Arc::new(NoopEmbedding::new(DIM)));
    seed(&engine).await;

    // Default strategy == "auto".
    let auto = engine
        .recall(RecallRequest::new("dosage".to_string()))
        .await;
    assert!(
        matches!(auto, Err(Error::EmbedderNotConfigured { .. })),
        "auto recall under noop must hard-error, got: {auto:?}"
    );

    let mut hybrid_req = RecallRequest::new("dosage".to_string());
    hybrid_req.strategy = Some("hybrid".to_string());
    assert!(
        matches!(
            engine.recall(hybrid_req).await,
            Err(Error::EmbedderNotConfigured { .. })
        ),
        "hybrid recall under noop must hard-error"
    );
}

/// (b) Lexical (BM25) recall needs no embedder — it still returns results under
/// the no-op embedder. The non-semantic path must not be broken.
#[tokio::test]
async fn lexical_recall_with_noop_embedder_still_returns_results() {
    let engine = engine_with(Arc::new(NoopEmbedding::new(DIM)));
    seed(&engine).await;

    let mut req = RecallRequest::new("dosage".to_string());
    req.strategy = Some("lexical".to_string());

    let resp = engine
        .recall(req)
        .await
        .expect("lexical recall must not require an embedder");
    assert!(
        resp.total >= 1,
        "lexical BM25 recall should find the 'dosage' memory, got {}",
        resp.total
    );
    assert!(
        resp.memories.iter().any(|m| m.content.contains("dosage")),
        "expected the dosage memory in lexical results"
    );
}

/// (c) Semantic recall with a real (deterministic) embedder returns results.
#[tokio::test]
async fn semantic_recall_with_real_embedder_returns_results() {
    let engine = engine_with(Arc::new(HashEmbedding));
    seed(&engine).await;

    let mut req = RecallRequest::new("clinician adjusted the dosage".to_string());
    req.strategy = Some("semantic".to_string());

    let resp = engine
        .recall(req)
        .await
        .expect("semantic recall with a real embedder must succeed");
    assert!(
        resp.total >= 1,
        "semantic recall with a real embedder should return results, got {}",
        resp.total
    );
}
