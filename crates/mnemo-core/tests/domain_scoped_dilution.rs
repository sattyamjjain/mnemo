//! Domain-scoped recall vs flat recall — vector-search-dilution curve
//! (MASDR-RAG, arXiv:2606.11350).
//!
//! Thesis: as a corpus grows, off-domain records that are *semantically
//! similar* to the query crowd into the dense top-k and dilute
//! precision — adding more documents makes retrieval **worse**, not
//! better. Restricting the candidate set to the metadata-defined
//! sub-corpus *before* the dense step (DOMAIN_SCOPED) recovers precision
//! regardless of how large the off-domain corpus grows.
//!
//! This eval reproduces the dilution curve on a synthetic corpus growing
//! 50 → 1,000 docs:
//!   - 10 fixed "gold" docs live in tenant `alpha` and carry the query's
//!     topical tokens;
//!   - the rest are off-domain `beta` distractors carrying the *same*
//!     topical tokens (so the dense lane cannot tell them apart) plus a
//!     domain-blind amount of filler.
//!
//! Flat semantic recall's P@10 collapses as the `beta` corpus grows;
//! DOMAIN_SCOPED (`org_id = alpha`) holds at ~1.0. The test asserts the
//! gap at the largest size is ≥ 0.05 (it is ~0.9) and that flat recall
//! visibly degraded.

use std::sync::Arc;

use async_trait::async_trait;

use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::Result as MnemoResult;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::retrieval::{DomainScope, RetrievalMode};

const DIM: usize = 256;
const GOLD: usize = 10;
const SIZES: [usize; 4] = [50, 200, 500, 1000];
const QUERY: &str = "alpha quantum flux resonance";

/// Deterministic bag-of-words hashing embedder: each whitespace token is
/// hashed into a bucket; the vector is L2-normalized. Tokens shared with
/// the query drive cosine similarity. No external model, fully
/// reproducible — what the dilution curve needs.
struct HashEmbedding {
    dim: usize,
}

fn fnv1a(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn embed_text(text: &str, dim: usize) -> Vec<f32> {
    let mut v = vec![0f32; dim];
    for tok in text.split_whitespace() {
        let bucket = (fnv1a(tok) as usize) % dim;
        v[bucket] += 1.0;
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
        Ok(embed_text(text, self.dim))
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnemoResult<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| embed_text(t, self.dim)).collect())
    }
    fn dimensions(&self) -> usize {
        self.dim
    }
}

fn build_engine() -> MnemoEngine {
    let storage =
        Arc::new(mnemo_core::storage::duckdb::DuckDbStorage::open_in_memory().expect("duckdb"));
    let index = Arc::new(UsearchIndex::new(DIM).expect("usearch"));
    let embedding = Arc::new(HashEmbedding { dim: DIM });
    MnemoEngine::new(
        storage,
        index,
        embedding,
        "dilution-agent".to_string(),
        None,
    )
}

/// Build doc content for index `i` in a corpus of `size`. Exactly `GOLD`
/// docs (evenly spread across the id space) are tenant `alpha` golds; the
/// rest are `beta` distractors carrying the same topical tokens plus a
/// domain-blind amount of filler, so the dense lane cannot separate them.
fn doc(i: usize, size: usize) -> (bool, String) {
    let stride = size / GOLD;
    let is_gold = i.is_multiple_of(stride) && (i / stride) < GOLD;
    let marker = if is_gold { "GOLDDOC" } else { "NOISEDOC" };
    // Domain-blind filler count (decorrelated from gold membership).
    let filler_n = (fnv1a(&format!("f{i}")) % 6) as usize;
    let filler: String = (0..filler_n).map(|k| format!(" fill{i}_{k}")).collect();
    (
        is_gold,
        format!("alpha quantum flux resonance {marker} u{i}{filler}"),
    )
}

async fn seed(engine: &MnemoEngine, size: usize) {
    for i in 0..size {
        let (is_gold, content) = doc(i, size);
        let mut req = RememberRequest::new(content);
        req.org_id = Some(if is_gold { "alpha" } else { "beta" }.to_string());
        engine.remember(req).await.expect("remember");
    }
}

/// Precision@10 = fraction of the top-10 hits that are gold docs.
fn p_at_10(contents: &[String]) -> f64 {
    let gold = contents.iter().filter(|c| c.contains("GOLDDOC")).count();
    gold as f64 / 10.0
}

async fn flat_p10(engine: &MnemoEngine) -> f64 {
    let mut req = RecallRequest::new(QUERY.to_string());
    req.limit = Some(10);
    req.strategy = Some("semantic".to_string());
    let hits: Vec<String> = engine
        .recall(req)
        .await
        .expect("recall")
        .memories
        .into_iter()
        .map(|m| m.content)
        .collect();
    p_at_10(&hits)
}

async fn scoped_p10(engine: &MnemoEngine) -> f64 {
    let mut req = RecallRequest::new(QUERY.to_string());
    req.limit = Some(10);
    req.mode = Some(RetrievalMode::DomainScoped);
    req.domain_scope = Some(DomainScope {
        org_id: Some("alpha".to_string()),
        ..Default::default()
    });
    let hits: Vec<String> = engine
        .recall(req)
        .await
        .expect("recall")
        .memories
        .into_iter()
        .map(|m| m.content)
        .collect();
    p_at_10(&hits)
}

#[tokio::test]
async fn domain_scoped_beats_flat_under_dilution() {
    println!("\n=== Vector-search dilution curve (MASDR-RAG, arXiv:2606.11350) ===");
    println!(
        "{GOLD} gold docs (tenant alpha) fixed; corpus grows with off-domain beta distractors"
    );
    println!("| corpus | flat P@10 | domain-scoped P@10 | gap |");
    println!("|-------:|----------:|-------------------:|----:|");

    let mut flat_curve = Vec::new();
    let mut scoped_curve = Vec::new();
    for size in SIZES {
        let engine = build_engine();
        seed(&engine, size).await;
        let flat = flat_p10(&engine).await;
        let scoped = scoped_p10(&engine).await;
        println!(
            "| {size:>6} | {flat:>9.3} | {scoped:>18.3} | {:>+.3} |",
            scoped - flat
        );
        flat_curve.push(flat);
        scoped_curve.push(scoped);
    }

    let largest = SIZES.len() - 1;
    // Core assertion: at the largest corpus, domain-scoped P@10 beats
    // flat recall by at least 0.05.
    let gap = scoped_curve[largest] - flat_curve[largest];
    assert!(
        gap >= 0.05,
        "domain-scoped P@10 ({:.3}) must beat flat ({:.3}) by >=0.05 at {} docs (gap {:.3})",
        scoped_curve[largest],
        flat_curve[largest],
        SIZES[largest],
        gap
    );
    // Dilution actually happened: flat recall degraded as the corpus grew.
    assert!(
        flat_curve[largest] < flat_curve[0],
        "flat P@10 should degrade under dilution ({:.3} at {} -> {:.3} at {})",
        flat_curve[0],
        SIZES[0],
        flat_curve[largest],
        SIZES[largest]
    );
    // Domain scoping holds precision regardless of corpus size.
    assert!(
        scoped_curve[largest] >= 0.9,
        "domain-scoped P@10 should stay high ({:.3} at {} docs)",
        scoped_curve[largest],
        SIZES[largest]
    );
}
