//! v0.4.9 — Embedding-backend selection bench + SLA-aware recommender.
//!
//! # Anchor
//!
//! [arXiv:2605.23618](https://arxiv.org/abs/2605.23618) (GE2 vs local
//! encoders — quality + latency) motivates choosing an embedding
//! backend by *measured* quality and tail-latency on the operator's
//! workload, not by reputation. This crate runs each available
//! backend against a small labeled fixture and reports:
//!
//! - nDCG@10 and recall@10 (quality)
//! - p50 / p95 single-vector embed latency (latency)
//! - throughput at batch sizes 1 / 8 / 32 (vectors/sec)
//!
//! Then the recommender picks the **highest-nDCG backend whose p95
//! is ≤ the SLO** and reports the nDCG gap vs the absolute
//! best-quality backend (so the operator sees the explicit quality
//! tradeoff for choosing the fast one).
//!
//! # Backends
//!
//! - [`mnemo_core::embedding::NoopEmbedding`] — zero vectors (always
//!   available; quality is degenerate by design — a floor reference).
//! - [`HashingBaseline`] — bench-local deterministic
//!   hashing-trick baseline (always available; not added to
//!   `mnemo-core` and not a production backend — a lexical sanity
//!   floor for default builds).
//! - [`mnemo_core::embedding::openai::OpenAiEmbedding`] — runs only
//!   if `OPENAI_API_KEY` is set; gated dataset bench, network-bound.
//! - [`mnemo_core::embedding::onnx::OnnxEmbedding`] — runs only if
//!   `MNEMO_ONNX_MODEL_PATH` is set AND mnemo-core is built with
//!   the `onnx` feature; local inference.
//!
//! # What this bench is NOT
//!
//! - **Not a faithful arXiv:2605.23618 reproduction.** That paper
//!   uses a curated MTEB-shaped dataset + multiple downstream tasks;
//!   this bench uses a 50-document / 10-query fixture checked into
//!   the bench dir, scored by nDCG@10 with binary relevance. The
//!   *shape* of the quality-vs-latency tradeoff is what carries over.
//! - **Not a managed-cloud recommendation.** Default builds do not
//!   require `OPENAI_API_KEY`; the recommender will pick a local
//!   backend when no remote one is configured. mnemo's embedded-first
//!   wedge is preserved — the bench is measurement and recommendation
//!   only, never a default-change.
//! - **Not a change to retrieval defaults.** The default read path,
//!   RRF weights, and engine wiring are untouched. This bench
//!   consumes `EmbeddingProvider` impls; it does not modify them.
//! - **`HashingBaseline` is not a real semantic embedder.** It is a
//!   feature-hashing-trick character-n-gram bag whose cosine
//!   similarity reflects lexical overlap, not meaning. It exists so
//!   default builds (no API key, no ONNX model) report a non-trivial
//!   row instead of a single all-zero Noop row.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::Result;
use serde::{Deserialize, Serialize};

/// One row of the labeled corpus.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CorpusDoc {
    pub id: String,
    pub topic: String,
    pub text: String,
}

/// One row of the labeled query set with its gold relevant doc IDs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GoldQuery {
    pub query: String,
    pub relevant_ids: Vec<String>,
}

/// Discoverable backend kinds — the bench enumerates these at
/// runtime and skips any whose construction fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackendKind {
    Noop,
    HashingBaseline,
    OpenAi,
    Onnx,
}

impl BackendKind {
    pub fn label(self) -> &'static str {
        match self {
            BackendKind::Noop => "noop",
            BackendKind::HashingBaseline => "hashing-baseline",
            BackendKind::OpenAi => "openai",
            BackendKind::Onnx => "onnx",
        }
    }
}

/// Bench-local deterministic baseline: feature-hashing-trick over
/// character n-grams. Not in `mnemo-core` and not a production
/// backend — only here so default builds (no API key, no ONNX
/// model) report a non-degenerate quality row alongside Noop.
///
/// Algorithm:
///
/// 1. Lowercase the text.
/// 2. Extract overlapping character 3-grams.
/// 3. For each n-gram, hash it (SHA-256 → first 8 bytes → u64) and
///    take the bucket = `hash % dimensions`.
/// 4. Increment `vector[bucket]` by 1.
/// 5. L2-normalize the resulting vector.
///
/// Cosine similarity between two such vectors approximates Jaccard
/// over the n-gram set with a slight density correction. It is
/// deterministic, in-process, and good enough to differentiate the
/// recommender's "lexical floor" arm from the Noop floor.
pub struct HashingBaseline {
    dimensions: usize,
}

impl HashingBaseline {
    pub fn new(dimensions: usize) -> Self {
        assert!(dimensions > 0, "HashingBaseline dimensions must be > 0");
        Self { dimensions }
    }

    fn embed_sync(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dimensions];
        let lower = text.to_lowercase();
        let bytes = lower.as_bytes();
        if bytes.len() < 3 {
            // Degenerate edge case: still emit a deterministic vector.
            let bucket = (bytes.first().copied().unwrap_or(0) as usize) % self.dimensions;
            v[bucket] = 1.0;
            return l2_normalize(v);
        }
        for window in bytes.windows(3) {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(window);
            let out = h.finalize();
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&out[..8]);
            let h64 = u64::from_le_bytes(buf);
            let bucket = (h64 as usize) % self.dimensions;
            v[bucket] += 1.0;
        }
        l2_normalize(v)
    }
}

fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
    v
}

#[async_trait]
impl EmbeddingProvider for HashingBaseline {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self.embed_sync(text))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_sync(t)).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Construction result for a backend: either a working provider or a
/// recorded reason it was skipped. Callers iterate over the list and
/// only measure the `Some(provider)` rows.
pub struct DiscoveredBackend {
    pub kind: BackendKind,
    pub provider: Option<Arc<dyn EmbeddingProvider>>,
    /// Human-readable reason this backend was unavailable. `None`
    /// when `provider` is `Some`.
    pub skipped_reason: Option<String>,
    /// Configured dimensions (for the result table).
    pub dimensions: usize,
}

/// Discover every backend the host can run right now. Always includes
/// Noop + HashingBaseline; conditionally includes OpenAI / ONNX based
/// on env vars + feature flags. Never panics — a backend that can't
/// be constructed is returned with `provider = None` and a reason.
pub fn discover_backends(dimensions: usize) -> Vec<DiscoveredBackend> {
    let mut out = Vec::new();

    out.push(DiscoveredBackend {
        kind: BackendKind::Noop,
        provider: Some(Arc::new(mnemo_core::embedding::NoopEmbedding::new(
            dimensions,
        ))),
        skipped_reason: None,
        dimensions,
    });

    out.push(DiscoveredBackend {
        kind: BackendKind::HashingBaseline,
        provider: Some(Arc::new(HashingBaseline::new(dimensions))),
        skipped_reason: None,
        dimensions,
    });

    match std::env::var("OPENAI_API_KEY") {
        Ok(k) if !k.is_empty() => {
            let model = std::env::var("MNEMO_EMBEDDING_MODEL")
                .unwrap_or_else(|_| "text-embedding-3-small".to_string());
            out.push(DiscoveredBackend {
                kind: BackendKind::OpenAi,
                provider: Some(Arc::new(
                    mnemo_core::embedding::openai::OpenAiEmbedding::new(k, model, dimensions),
                )),
                skipped_reason: None,
                dimensions,
            });
        }
        _ => out.push(DiscoveredBackend {
            kind: BackendKind::OpenAi,
            provider: None,
            skipped_reason: Some("OPENAI_API_KEY not set".to_string()),
            dimensions,
        }),
    }

    match std::env::var("MNEMO_ONNX_MODEL_PATH") {
        Ok(p) if !p.is_empty() => {
            match mnemo_core::embedding::onnx::OnnxEmbedding::new(&p, dimensions) {
                Ok(provider) => out.push(DiscoveredBackend {
                    kind: BackendKind::Onnx,
                    provider: Some(Arc::new(provider)),
                    skipped_reason: None,
                    dimensions,
                }),
                Err(e) => out.push(DiscoveredBackend {
                    kind: BackendKind::Onnx,
                    provider: None,
                    skipped_reason: Some(format!("OnnxEmbedding::new failed: {e}")),
                    dimensions,
                }),
            }
        }
        _ => out.push(DiscoveredBackend {
            kind: BackendKind::Onnx,
            provider: None,
            skipped_reason: Some("MNEMO_ONNX_MODEL_PATH not set".to_string()),
            dimensions,
        }),
    }

    out
}

/// Quality measurement for a single backend.
#[derive(Debug, Clone, Serialize)]
pub struct QualityResult {
    /// nDCG@10 averaged across the query set (binary relevance, log2-discount).
    pub ndcg_at_10: f64,
    /// recall@10 averaged across the query set.
    pub recall_at_10: f64,
    /// Per-query nDCG (same order as the query set), for diagnostics.
    pub per_query_ndcg: Vec<f64>,
}

/// Latency + throughput measurement for a single backend.
#[derive(Debug, Clone, Serialize)]
pub struct LatencyResult {
    pub p50_ms: f64,
    pub p95_ms: f64,
    /// Throughput at batch sizes 1, 8, 32. Each field carries
    /// vectors/second computed as `(batch_size / wall_time_for_batch)`.
    pub vec_per_sec_batch_1: f64,
    pub vec_per_sec_batch_8: f64,
    pub vec_per_sec_batch_32: f64,
}

/// The full row the result table renders.
#[derive(Debug, Clone, Serialize)]
pub struct BackendRow {
    pub backend: &'static str,
    pub dimensions: usize,
    pub ndcg_at_10: f64,
    pub recall_at_10: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub vec_per_sec_batch_1: f64,
    pub vec_per_sec_batch_8: f64,
    pub vec_per_sec_batch_32: f64,
}

/// Bench-run options. Latency-sample budgets are tunable so the
/// criterion bench and the CLI can share the same code with
/// different sample sizes.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub dimensions: usize,
    /// Number of single-vector embed calls to time for p50/p95.
    pub latency_samples: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            dimensions: 384,
            latency_samples: 32,
        }
    }
}

fn load_corpus() -> Vec<CorpusDoc> {
    let raw = include_str!("../data/corpus.json");
    serde_json::from_str(raw).expect("bundled corpus.json must parse")
}

fn load_queries() -> Vec<GoldQuery> {
    let raw = include_str!("../data/queries.json");
    serde_json::from_str(raw).expect("bundled queries.json must parse")
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

/// Score one query against a corpus that's already been embedded.
/// Returns the top-k doc IDs by cosine similarity.
fn rank_top_k(query_vec: &[f32], doc_vecs: &[(String, Vec<f32>)], k: usize) -> Vec<String> {
    let mut scored: Vec<(f32, &str)> = doc_vecs
        .iter()
        .map(|(id, v)| (cosine(query_vec, v), id.as_str()))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(k)
        .map(|(_, id)| id.to_string())
        .collect()
}

fn dcg_at_k(ranking: &[String], relevant: &[String], k: usize) -> f64 {
    let mut dcg = 0.0;
    for (i, id) in ranking.iter().take(k).enumerate() {
        if relevant.iter().any(|r| r == id) {
            // Binary relevance, log2(rank+1) discount where rank is 1-indexed.
            dcg += 1.0 / ((i as f64 + 2.0).log2());
        }
    }
    dcg
}

fn ideal_dcg_at_k(relevant_count: usize, k: usize) -> f64 {
    let mut idcg = 0.0;
    for i in 0..relevant_count.min(k) {
        idcg += 1.0 / ((i as f64 + 2.0).log2());
    }
    idcg
}

/// Measure quality for one backend on the bundled fixture.
pub async fn measure_quality(
    provider: &dyn EmbeddingProvider,
    corpus: &[CorpusDoc],
    queries: &[GoldQuery],
) -> Result<QualityResult> {
    let doc_texts: Vec<&str> = corpus.iter().map(|d| d.text.as_str()).collect();
    let doc_vecs_raw = provider.embed_batch(&doc_texts).await?;
    let doc_vecs: Vec<(String, Vec<f32>)> = corpus
        .iter()
        .zip(doc_vecs_raw)
        .map(|(d, v)| (d.id.clone(), v))
        .collect();

    let mut ndcgs = Vec::with_capacity(queries.len());
    let mut recalls = Vec::with_capacity(queries.len());

    for q in queries {
        let qv = provider.embed(&q.query).await?;
        let top10 = rank_top_k(&qv, &doc_vecs, 10);
        let dcg = dcg_at_k(&top10, &q.relevant_ids, 10);
        let idcg = ideal_dcg_at_k(q.relevant_ids.len(), 10);
        let ndcg = if idcg > 0.0 { dcg / idcg } else { 0.0 };
        let hit = top10
            .iter()
            .filter(|id| q.relevant_ids.iter().any(|r| r == *id))
            .count();
        let recall = if q.relevant_ids.is_empty() {
            0.0
        } else {
            hit as f64 / q.relevant_ids.len() as f64
        };
        ndcgs.push(ndcg);
        recalls.push(recall);
    }

    let mean = |xs: &[f64]| -> f64 {
        if xs.is_empty() {
            0.0
        } else {
            xs.iter().sum::<f64>() / xs.len() as f64
        }
    };
    Ok(QualityResult {
        ndcg_at_10: mean(&ndcgs),
        recall_at_10: mean(&recalls),
        per_query_ndcg: ndcgs,
    })
}

/// Measure single-vector embed latency (p50, p95) and batch throughput.
pub async fn measure_latency(
    provider: &dyn EmbeddingProvider,
    queries: &[GoldQuery],
    samples: usize,
) -> Result<LatencyResult> {
    // Single-vector latency: cycle through the query set until we
    // have `samples` measurements.
    let mut latencies_ms = Vec::with_capacity(samples);
    for i in 0..samples {
        let q = &queries[i % queries.len()];
        let started = Instant::now();
        let _ = provider.embed(&q.query).await?;
        latencies_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let p50 = percentile(&latencies_ms, 0.50);
    let p95 = percentile(&latencies_ms, 0.95);

    // Throughput at batch sizes 1, 8, 32. Use cycled query strings.
    let throughput = |batch_size: usize| -> Vec<&str> {
        (0..batch_size)
            .map(|i| queries[i % queries.len()].query.as_str())
            .collect()
    };

    let vps_1 = run_throughput(provider, &throughput(1)).await?;
    let vps_8 = run_throughput(provider, &throughput(8)).await?;
    let vps_32 = run_throughput(provider, &throughput(32)).await?;

    Ok(LatencyResult {
        p50_ms: p50,
        p95_ms: p95,
        vec_per_sec_batch_1: vps_1,
        vec_per_sec_batch_8: vps_8,
        vec_per_sec_batch_32: vps_32,
    })
}

async fn run_throughput(provider: &dyn EmbeddingProvider, batch: &[&str]) -> Result<f64> {
    let started = Instant::now();
    let _ = provider.embed_batch(batch).await?;
    let elapsed = started.elapsed().as_secs_f64();
    if elapsed <= 0.0 {
        Ok(f64::INFINITY)
    } else {
        Ok(batch.len() as f64 / elapsed)
    }
}

fn percentile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut s = values.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = (q * (s.len() - 1) as f64).round() as usize;
    s[idx.min(s.len() - 1)]
}

/// Convenience: run the full bench (quality + latency) over every
/// discovered backend.
pub async fn run_all(opts: &RunOptions) -> Vec<(BackendKind, Option<BackendRow>, Option<String>)> {
    let corpus = load_corpus();
    let queries = load_queries();
    let backends = discover_backends(opts.dimensions);
    let mut out = Vec::with_capacity(backends.len());
    for b in backends {
        let dims = b.dimensions;
        let kind = b.kind;
        let label = kind.label();
        let Some(provider) = b.provider else {
            out.push((kind, None, b.skipped_reason));
            continue;
        };
        let quality = match measure_quality(provider.as_ref(), &corpus, &queries).await {
            Ok(q) => q,
            Err(e) => {
                out.push((kind, None, Some(format!("measure_quality failed: {e}"))));
                continue;
            }
        };
        let latency = match measure_latency(provider.as_ref(), &queries, opts.latency_samples).await
        {
            Ok(l) => l,
            Err(e) => {
                out.push((kind, None, Some(format!("measure_latency failed: {e}"))));
                continue;
            }
        };
        out.push((
            kind,
            Some(BackendRow {
                backend: label,
                dimensions: dims,
                ndcg_at_10: quality.ndcg_at_10,
                recall_at_10: quality.recall_at_10,
                p50_ms: latency.p50_ms,
                p95_ms: latency.p95_ms,
                vec_per_sec_batch_1: latency.vec_per_sec_batch_1,
                vec_per_sec_batch_8: latency.vec_per_sec_batch_8,
                vec_per_sec_batch_32: latency.vec_per_sec_batch_32,
            }),
            None,
        ));
    }
    out
}

/// SLA-aware recommendation. Picks the highest-nDCG backend whose
/// `p95_ms <= slo_ms` and reports the gap vs the absolute best.
#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub slo_ms: f64,
    /// Picked backend label, or `None` if no row meets the SLO.
    pub picked: Option<String>,
    pub picked_ndcg: Option<f64>,
    pub picked_p95_ms: Option<f64>,
    /// Best-quality backend by nDCG (regardless of SLO), as a reference point.
    pub best_quality_backend: Option<String>,
    pub best_quality_ndcg: Option<f64>,
    pub best_quality_p95_ms: Option<f64>,
    /// `best_quality_ndcg - picked_ndcg`. `None` if either is missing.
    pub ndcg_gap_vs_best: Option<f64>,
    /// Human-readable tradeoff sentence ("you give up X nDCG for Yx
    /// lower latency"). Empty when no comparison is possible.
    pub tradeoff_sentence: String,
}

pub fn recommend(rows: &[BackendRow], slo_ms: f64) -> Recommendation {
    let mut best_q: Option<&BackendRow> = None;
    for r in rows {
        if best_q.is_none_or(|b| r.ndcg_at_10 > b.ndcg_at_10) {
            best_q = Some(r);
        }
    }

    let mut picked: Option<&BackendRow> = None;
    for r in rows {
        if r.p95_ms <= slo_ms && picked.is_none_or(|p| r.ndcg_at_10 > p.ndcg_at_10) {
            picked = Some(r);
        }
    }

    let tradeoff_sentence = match (picked, best_q) {
        (Some(p), Some(b)) if p.backend != b.backend => {
            let gap = b.ndcg_at_10 - p.ndcg_at_10;
            let ratio = if p.p95_ms > 0.0 {
                b.p95_ms / p.p95_ms
            } else {
                f64::INFINITY
            };
            format!(
                "you give up {gap:.3} nDCG for {ratio:.1}x lower p95 latency ({:.1} ms vs {:.1} ms)",
                p.p95_ms, b.p95_ms
            )
        }
        (Some(p), Some(b)) if p.backend == b.backend => format!(
            "best-quality backend ({}) also meets the SLO — no tradeoff",
            p.backend
        ),
        (None, Some(b)) => format!(
            "no backend's p95 fits under SLO {slo_ms:.1} ms; best-quality ({}) p95 = {:.1} ms",
            b.backend, b.p95_ms
        ),
        _ => String::new(),
    };

    Recommendation {
        slo_ms,
        picked: picked.map(|p| p.backend.to_string()),
        picked_ndcg: picked.map(|p| p.ndcg_at_10),
        picked_p95_ms: picked.map(|p| p.p95_ms),
        best_quality_backend: best_q.map(|b| b.backend.to_string()),
        best_quality_ndcg: best_q.map(|b| b.ndcg_at_10),
        best_quality_p95_ms: best_q.map(|b| b.p95_ms),
        ndcg_gap_vs_best: match (picked, best_q) {
            (Some(p), Some(b)) => Some(b.ndcg_at_10 - p.ndcg_at_10),
            _ => None,
        },
        tradeoff_sentence,
    }
}

/// Pretty-print the result table for the CLI. Returns the table as a
/// `String` so callers can also write it to a report file.
pub fn render_table(
    results: &[(BackendKind, Option<BackendRow>, Option<String>)],
    rec: &Recommendation,
) -> String {
    let mut s = String::new();
    s.push_str("\nbackend            | dim  | nDCG@10 | recall@10 | p50 ms | p95 ms | vec/s b1 | vec/s b8 | vec/s b32\n");
    s.push_str("------------------ | ---- | ------- | --------- | ------ | ------ | -------- | -------- | ---------\n");
    for (kind, row, skipped) in results {
        match (row, skipped) {
            (Some(r), _) => {
                s.push_str(&format!(
                    "{:<18} | {:<4} | {:>7.3} | {:>9.3} | {:>6.1} | {:>6.1} | {:>8.0} | {:>8.0} | {:>9.0}\n",
                    r.backend,
                    r.dimensions,
                    r.ndcg_at_10,
                    r.recall_at_10,
                    r.p50_ms,
                    r.p95_ms,
                    r.vec_per_sec_batch_1,
                    r.vec_per_sec_batch_8,
                    r.vec_per_sec_batch_32,
                ));
            }
            (None, Some(reason)) => {
                s.push_str(&format!(
                    "{:<18} | --   | skipped: {}\n",
                    kind.label(),
                    reason
                ));
            }
            (None, None) => {}
        }
    }
    s.push_str("\nRecommendation\n");
    s.push_str(&format!("  SLO p95: {:.1} ms\n", rec.slo_ms));
    match (&rec.picked, rec.picked_ndcg, rec.picked_p95_ms) {
        (Some(p), Some(n), Some(p95)) => {
            s.push_str(&format!(
                "  Picked: {p} (nDCG@10 = {n:.3}, p95 = {p95:.1} ms)\n"
            ));
        }
        _ => {
            s.push_str("  Picked: none (no backend fits under the SLO)\n");
        }
    }
    if let (Some(b), Some(n), Some(p95)) = (
        &rec.best_quality_backend,
        rec.best_quality_ndcg,
        rec.best_quality_p95_ms,
    ) {
        s.push_str(&format!(
            "  Best quality (reference): {b} (nDCG@10 = {n:.3}, p95 = {p95:.1} ms)\n"
        ));
    }
    if !rec.tradeoff_sentence.is_empty() {
        s.push_str(&format!("  Tradeoff: {}\n", rec.tradeoff_sentence));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_and_queries_parse() {
        let corpus = load_corpus();
        let queries = load_queries();
        assert!(corpus.len() >= 30, "corpus must be reasonably sized");
        assert!(queries.len() >= 5, "queries must be reasonably sized");
        // Every gold ID must exist in the corpus — else the bench
        // is silently mis-scored.
        let ids: std::collections::HashSet<&str> = corpus.iter().map(|d| d.id.as_str()).collect();
        for q in &queries {
            for r in &q.relevant_ids {
                assert!(
                    ids.contains(r.as_str()),
                    "gold relevant id {r} missing from corpus"
                );
            }
        }
    }

    #[test]
    fn ndcg_perfect_ranking_is_one() {
        let ranking = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let relevant = vec!["a".to_string(), "b".to_string()];
        let dcg = dcg_at_k(&ranking, &relevant, 10);
        let idcg = ideal_dcg_at_k(relevant.len(), 10);
        assert!(idcg > 0.0);
        assert!((dcg / idcg - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ndcg_no_hit_is_zero() {
        let ranking = vec!["x".to_string(), "y".to_string()];
        let relevant = vec!["a".to_string()];
        let dcg = dcg_at_k(&ranking, &relevant, 10);
        assert_eq!(dcg, 0.0);
    }

    #[tokio::test]
    async fn hashing_baseline_beats_noop_on_fixture() {
        // Sanity floor: HashingBaseline must outperform Noop on the
        // bundled fixture, otherwise the bench has no useful default-build
        // signal.
        let corpus = load_corpus();
        let queries = load_queries();
        let noop = mnemo_core::embedding::NoopEmbedding::new(384);
        let hashing = HashingBaseline::new(384);
        let q_noop = measure_quality(&noop, &corpus, &queries).await.unwrap();
        let q_hash = measure_quality(&hashing, &corpus, &queries).await.unwrap();
        assert!(
            q_hash.ndcg_at_10 > q_noop.ndcg_at_10,
            "expected hashing-baseline nDCG ({}) > noop nDCG ({})",
            q_hash.ndcg_at_10,
            q_noop.ndcg_at_10
        );
    }

    #[test]
    fn recommender_picks_fastest_acceptable_quality() {
        let rows = vec![
            BackendRow {
                backend: "noop",
                dimensions: 384,
                ndcg_at_10: 0.0,
                recall_at_10: 0.0,
                p50_ms: 0.01,
                p95_ms: 0.02,
                vec_per_sec_batch_1: 100000.0,
                vec_per_sec_batch_8: 800000.0,
                vec_per_sec_batch_32: 3200000.0,
            },
            BackendRow {
                backend: "hashing-baseline",
                dimensions: 384,
                ndcg_at_10: 0.45,
                recall_at_10: 0.50,
                p50_ms: 0.05,
                p95_ms: 0.10,
                vec_per_sec_batch_1: 20000.0,
                vec_per_sec_batch_8: 160000.0,
                vec_per_sec_batch_32: 640000.0,
            },
            BackendRow {
                backend: "openai",
                dimensions: 384,
                ndcg_at_10: 0.85,
                recall_at_10: 0.90,
                p50_ms: 100.0,
                p95_ms: 250.0,
                vec_per_sec_batch_1: 10.0,
                vec_per_sec_batch_8: 60.0,
                vec_per_sec_batch_32: 200.0,
            },
        ];
        // SLO < OpenAI p95: should pick hashing-baseline (highest nDCG under SLO).
        let r = recommend(&rows, 50.0);
        assert_eq!(r.picked.as_deref(), Some("hashing-baseline"));
        assert_eq!(r.best_quality_backend.as_deref(), Some("openai"));
        let gap = r.ndcg_gap_vs_best.unwrap();
        assert!((gap - 0.40).abs() < 1e-6, "expected gap ~0.40, got {gap}");
        assert!(r.tradeoff_sentence.contains("nDCG"));

        // Very tight SLO: nothing fits.
        let r2 = recommend(&rows, 0.001);
        assert!(r2.picked.is_none());
        assert!(r2.tradeoff_sentence.starts_with("no backend"));

        // Generous SLO: best-quality also wins → no-tradeoff sentence.
        let r3 = recommend(&rows, 10_000.0);
        assert_eq!(r3.picked.as_deref(), Some("openai"));
        assert!(r3.tradeoff_sentence.contains("no tradeoff"));
    }
}
