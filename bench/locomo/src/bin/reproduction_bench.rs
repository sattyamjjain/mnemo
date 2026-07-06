//! `reproduction_bench` — claimed-vs-observed LoCoMo reproduction.
//!
//! # What this is
//!
//! A **reproducible, deterministic, offline** re-run of one well-known LoCoMo
//! subtask — **single-hop retrieval** — under mnemo's *default* hybrid recall
//! (`strategy = "auto"`: semantic vector + BM25 + graph-expansion + recency,
//! RRF-fused), reported next to the LoCoMo numbers competitors have *published*.
//!
//! The point is **reproducibility by disclosure**, riding the 2026
//! memory-benchmark reproducibility crisis: several headline LoCoMo scores fell
//! sharply under independent re-evaluation (Zep's 84% → 58.44% corrected;
//! MemPalace's 100% → 60.3% R@10 without an oversized `top_k`; Mem0's 92.5 is
//! materially higher than community re-runs). So this bin publishes mnemo's
//! **observed** number with a fixed seed + a Wilson-95 interval that **anyone
//! can re-run offline**, and tables it against the vendors' **own published,
//! cited figures — which are NOT re-run in this harness**. Only mnemo's row is
//! reproducible here. No "best" / "first" claim is made.
//!
//! # Metric
//!
//! The bundled LongMemEval_M slice is a LoCoMo-style single-hop set: each
//! record's `query` is answerable from its own `content`, so the record is its
//! own gold document (matched by `lme_id` metadata). Observed accuracy =
//! recall@1/@3/@5 + MRR of the gold document, with a **Wilson 95%** interval on
//! recall@1 ([`mnemo_locomo_bench::stats::wilson_95`]).
//!
//! # This is NOT the published LoCoMo QA score
//!
//! It is **retrieval** quality on a small bundled slice, not the LLM-judged
//! end-to-end QA accuracy the vendors report (gated datasets + judge model,
//! [#44](https://github.com/sattyamjjain/mnemo/issues/44)). The observed number
//! is therefore **not comparable** to the claimed column as a ranking — it is a
//! disclosure of what mnemo's mechanism recovers on a fixture you can re-run.
//!
//! # Embedder
//!
//! Default: a **deterministic offline** hashed bag-of-tokens embedder (no
//! network, no LLM, identical output every run). `--ollama-model <name>` gates a
//! real semantic embedder (higher fidelity, NOT deterministic); it fails loud if
//! Ollama is unreachable rather than emitting a silent number — matching the
//! sibling benches.
//!
//! Reproduce: `cargo run --release -p mnemo-locomo-bench --bin reproduction_bench`

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use clap::Parser;

use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::{Error, Result as MnResult};
use mnemo_core::index::VectorIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_locomo_bench::dataset::{LongMemRecord, dataset_sha, default_dataset_path, load_dataset};
use mnemo_locomo_bench::stats::wilson_95;

const AGENT: &str = "reproduction-bench-agent";
/// Fixed default seed → reproducibility signal (the offline path is
/// deterministic on the fixed fixture regardless; the seed is pinned in the
/// report for provenance and used only on the gated real-embedder path).
const DEFAULT_SEED: u64 = 0x10C0_2026_2026_u64;
const EMBED_DIM: usize = 128;

#[derive(Parser, Debug)]
#[command(name = "reproduction_bench")]
struct Cli {
    /// Fresh-engine repeats pooled into the point estimate. On the deterministic
    /// offline path a single pass suffices (repeats agree); raise it with
    /// `--ollama-model` to average the real embedder's approximate-NN noise.
    /// Wilson is always computed over the distinct query count, never `n*repeats`.
    #[arg(long, default_value_t = 1)]
    repeats: usize,
    /// Deterministic seed (pinned in the report).
    #[arg(long, default_value_t = DEFAULT_SEED)]
    seed: u64,
    /// Output directory for the byte-stable Markdown + JSON report.
    #[arg(long, default_value = "bench/locomo/results")]
    out_dir: PathBuf,
    /// Override the bundled LongMemEval_M slice.
    #[arg(long, env = "MNEMO_LONGMEMEVAL_PATH")]
    dataset: Option<PathBuf>,
    /// Report date `YYYY-MM-DD`; defaults to today (UTC). Pinned so the report
    /// filename + body are byte-stable within a run.
    #[arg(long)]
    date: Option<String>,
    /// Use a real Ollama embedder instead of the deterministic offline one.
    /// Higher fidelity, NOT deterministic, NOT CI-safe; fails loud if Ollama is
    /// unreachable.
    #[arg(long)]
    ollama_model: Option<String>,
    /// Ollama base URL (used only with `--ollama-model`).
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,
}

// ---------------------------------------------------------------------------
// Embedders (mirrors beam_bench: offline default, gated real path)
// ---------------------------------------------------------------------------

fn fnv1a(t: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for b in t.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Deterministic, offline, network-free embedder: L2-normalised hashed
/// bag-of-tokens. Lexical (no synonymy) — the point is reproducibility.
struct HashEmbedding {
    dim: usize,
}

impl HashEmbedding {
    fn embed_sync(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        for tok in text
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
        {
            let idx = (fnv1a(&tok.to_ascii_lowercase()) as usize) % self.dim;
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
}

#[async_trait]
impl EmbeddingProvider for HashEmbedding {
    async fn embed(&self, text: &str) -> MnResult<Vec<f32>> {
        Ok(self.embed_sync(text))
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnResult<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_sync(t)).collect())
    }
    fn dimensions(&self) -> usize {
        self.dim
    }
}

/// Real semantic embedder via a local Ollama server (gated behind
/// `--ollama-model`). Fails loud, never silent.
struct OllamaEmbedding {
    client: reqwest::Client,
    url: String,
    model: String,
    dim: usize,
}

impl OllamaEmbedding {
    async fn connect(url: String, model: String) -> MnResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Embedding(e.to_string()))?;
        let mut me = Self {
            client,
            url,
            model,
            dim: 0,
        };
        let probe = me.embed("dimension probe").await?;
        me.dim = probe.len();
        Ok(me)
    }
    async fn embed_one(&self, text: &str) -> MnResult<Vec<f32>> {
        let resp = self
            .client
            .post(format!("{}/api/embeddings", self.url))
            .json(&serde_json::json!({ "model": self.model, "prompt": text }))
            .send()
            .await
            .map_err(|e| {
                Error::Embedding(format!(
                    "{e} — is Ollama running and the model pulled? Try: `ollama pull {}`",
                    self.model
                ))
            })?;
        if !resp.status().is_success() {
            return Err(Error::Embedding(format!(
                "ollama returned HTTP {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Error::Embedding(format!("ollama response decode: {e}")))?;
        let embedding = body
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| Error::Embedding("ollama response missing `embedding`".to_string()))?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();
        Ok(embedding)
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbedding {
    async fn embed(&self, text: &str) -> MnResult<Vec<f32>> {
        self.embed_one(text).await
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnResult<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed_one(t).await?);
        }
        Ok(out)
    }
    fn dimensions(&self) -> usize {
        self.dim
    }
}

// ---------------------------------------------------------------------------
// Deterministic vector index
// ---------------------------------------------------------------------------

/// Exact brute-force cosine index. mnemo's default index is USearch **HNSW** —
/// an *approximate* NN structure whose internal level-assignment RNG makes its
/// ranking (and therefore any recall@k built on it) jitter run-to-run on data
/// with tight margins (real dialogue text under a lexical embedder). HNSW is,
/// by construction, a fast approximation of **exact** nearest-neighbour search;
/// this index computes that exact search deterministically (distance, then
/// stable insertion order on ties). Swapping it in for this bench makes the
/// published number **reproducible** while leaving every other lane of mnemo's
/// default `auto`/RRF recall (BM25, recency, graph, the fusion itself)
/// unchanged. It is the deterministic reference the HNSW index tracks; a
/// production HNSW deployment sees values within its approximate-NN noise floor.
struct BruteForceIndex {
    rows: std::sync::RwLock<Vec<(uuid::Uuid, Vec<f32>)>>,
}

impl BruteForceIndex {
    fn new() -> Self {
        Self {
            rows: std::sync::RwLock::new(Vec::new()),
        }
    }
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
    1.0 - dot / denom
}

impl VectorIndex for BruteForceIndex {
    fn add(&self, id: uuid::Uuid, vector: &[f32]) -> MnResult<()> {
        self.rows.write().unwrap().push((id, vector.to_vec()));
        Ok(())
    }
    fn remove(&self, id: uuid::Uuid) -> MnResult<()> {
        self.rows.write().unwrap().retain(|(x, _)| *x != id);
        Ok(())
    }
    fn search(&self, query: &[f32], limit: usize) -> MnResult<Vec<(uuid::Uuid, f32)>> {
        self.filtered_search(query, limit, &|_| true)
    }
    fn filtered_search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &dyn Fn(uuid::Uuid) -> bool,
    ) -> MnResult<Vec<(uuid::Uuid, f32)>> {
        let rows = self.rows.read().unwrap();
        let mut scored: Vec<(usize, uuid::Uuid, f32)> = rows
            .iter()
            .enumerate()
            .filter(|(_, (id, _))| filter(*id))
            .map(|(i, (id, v))| (i, *id, cosine_distance(query, v)))
            .collect();
        // Deterministic total order: distance asc, then stable insertion index.
        scored.sort_by(|a, b| {
            a.2.partial_cmp(&b.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        Ok(scored
            .into_iter()
            .take(limit)
            .map(|(_, id, d)| (id, d))
            .collect())
    }
    fn save(&self, _path: &std::path::Path) -> MnResult<()> {
        Ok(())
    }
    fn load(&self, _path: &std::path::Path) -> MnResult<()> {
        Ok(())
    }
    fn len(&self) -> usize {
        self.rows.read().unwrap().len()
    }
}

// ---------------------------------------------------------------------------
// Engine + subtask
// ---------------------------------------------------------------------------

fn build_engine(embedding: Arc<dyn EmbeddingProvider>, _dim: usize) -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(BruteForceIndex::new());
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft)
}

async fn seed(engine: &MnemoEngine, dataset: &[LongMemRecord]) {
    for r in dataset {
        let mut req = RememberRequest::new(r.content.clone());
        req.importance = Some(0.5);
        req.tags = Some(r.tags.clone());
        req.thread_id = Some(r.conversation_id.clone());
        req.metadata = Some(serde_json::json!({
            "lme_id": r.id,
            "conversation_id": r.conversation_id,
            "turn": r.turn,
        }));
        engine.remember(req).await.expect("seed remember failed");
    }
}

fn auto_recall(query: &str, limit: usize) -> RecallRequest {
    let mut req = RecallRequest::new(query.to_string());
    req.strategy = Some("auto".to_string()); // mnemo's DEFAULT hybrid/RRF
    req.limit = Some(limit);
    // Neutralise the recency lane. The whole corpus is seeded in a single batch,
    // so every memory is equally recent — a wall-clock recency signal carries no
    // information here and only injects run-to-run noise (its `created_at`
    // inputs differ every run). A half-life of ~ages makes `recency_score ≡ 1.0`
    // for all records, so the recency lane is a constant and the fused ranking
    // is reproducible. Vector + BM25 + graph fusion is otherwise the default.
    req.recency_half_life_hours = Some(1.0e12);
    req
}

/// Deterministic rank of the gold `lme_id` among the recalled candidates.
///
/// We retrieve the FULL candidate set (limit = corpus size) and re-rank it by
/// `(fused score desc, lme_id asc)`. mnemo's default top-k truncation tie-breaks
/// equal fused scores in a run-varying order (HNSW level-RNG + hash-map
/// iteration), which makes a raw `recall@1` jitter run-to-run. Re-ranking by the
/// **stable** `lme_id` on score ties removes that jitter without changing which
/// documents mnemo's `auto` fusion surfaced or their scores — so the published
/// number is reproducible. Returns `None` if the gold was not returned at all.
fn gold_rank(
    memories: &[mnemo_core::query::recall::ScoredMemory],
    gold_lme_id: &str,
) -> Option<usize> {
    let mut ranked: Vec<(&str, f32)> = memories
        .iter()
        .filter_map(|m| {
            m.metadata
                .get("lme_id")
                .and_then(|v| v.as_str())
                .map(|lme| (lme, m.score))
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    ranked
        .iter()
        .position(|(lme, _)| *lme == gold_lme_id)
        .map(|i| i + 1)
}

/// One pass over the whole single-hop split on a fresh engine. Returns
/// per-cutoff hit counts, the reciprocal-rank sum, and the number of queries
/// that errored in recall (disclosed, and counted as misses — see below).
struct Pass {
    r1: usize,
    r3: usize,
    r5: usize,
    rr_sum: f64,
    errored: usize,
}

async fn run_pass(
    make_embedding: &dyn Fn() -> Arc<dyn EmbeddingProvider>,
    dim: usize,
    dataset: &[LongMemRecord],
) -> Pass {
    let engine = build_engine(make_embedding(), dim);
    seed(&engine, dataset).await;
    let mut p = Pass {
        r1: 0,
        r3: 0,
        r5: 0,
        rr_sum: 0.0,
        errored: 0,
    };
    // Retrieve the full candidate set so the deterministic re-rank never loses a
    // gold to nondeterministic top-k truncation. `k` remains the metric cutoff.
    let retrieve = dataset.len();
    for r in dataset {
        // mnemo's DEFAULT `auto` recall fans the raw query into a BM25 lane
        // whose Tantivy query parser rejects some punctuation (e.g. an
        // apostrophe in "patient's"). We do NOT sanitize the query — that would
        // measure a non-default path. A recall that errors surfaces no gold, so
        // it is an honest MISS; we count it and disclose the total. (This
        // matches how the sibling `semantic_recall_bench` treats recall
        // failures, and keeps the number a truthful picture of default recall.)
        let resp = match engine.recall(auto_recall(&r.query, retrieve)).await {
            Ok(resp) => resp,
            Err(_) => {
                p.errored += 1;
                continue;
            }
        };
        if let Some(rank) = gold_rank(&resp.memories, &r.id) {
            if rank <= 1 {
                p.r1 += 1;
            }
            if rank <= 3 {
                p.r3 += 1;
            }
            if rank <= 5 {
                p.r5 += 1;
            }
            p.rr_sum += 1.0 / rank as f64;
        }
    }
    p
}

/// A published competitor claim — cited, dated, and explicitly NOT re-run here.
struct Claim {
    system: &'static str,
    claimed: &'static str,
    note: &'static str,
    source: &'static str,
}

fn published_claims() -> Vec<Claim> {
    vec![
        Claim {
            system: "Mem0",
            claimed: "92.5 (LoCoMo, LLM-judged QA)",
            note: "vendor-published; independent/community re-runs land materially lower (the reproducibility gap this bench rides)",
            source: "https://mem0.ai/research",
        },
        Claim {
            system: "Zep",
            claimed: "84 → 58.44 (corrected)",
            note: "the 84% LoCoMo claim was re-scored to 58.44% under corrected evaluation",
            source: "https://github.com/getzep/zep-papers/issues/5",
        },
        Claim {
            system: "MemPalace",
            claimed: "100 → 60.3 R@10 (corrected)",
            note: "100% used top_k=50 (> sessions/conversation, i.e. every session returned); honest retrieval R@10 is 60.3%",
            source: "https://github.com/MemPalace/mempalace/issues/29",
        },
        Claim {
            system: "Supermemory",
            claimed: "~99 (self-reported, not verified)",
            note: "QA accuracy from an 8-/12-agent ensemble the authors frame as an experimental proof-of-concept, not production",
            source: "https://dev.to/varun_pratapbhardwaj_b13/5-ai-agent-memory-systems-compared-mem0-zep-letta-supermemory-superlocalmemory-2026-benchmark-59p3",
        },
    ]
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let dataset_path = cli.dataset.clone().unwrap_or_else(default_dataset_path);
    let dataset = load_dataset(&dataset_path);
    assert!(!dataset.is_empty(), "dataset is empty");
    let sha = dataset_sha(&dataset_path);
    let n = dataset.len();

    // Resolve the embedder (offline default; --ollama-model gated + fail-loud).
    let (embed_backend, dim): (String, usize) = if let Some(ref model) = cli.ollama_model {
        let probe = OllamaEmbedding::connect(cli.ollama_url.clone(), model.clone()).await?;
        (format!("ollama:{model}"), probe.dim)
    } else {
        (
            "hash-bag-of-tokens (deterministic, offline)".to_string(),
            EMBED_DIM,
        )
    };
    let url = cli.ollama_url.clone();
    let model = cli.ollama_model.clone();
    let make_embedding = move || -> Arc<dyn EmbeddingProvider> {
        if let Some(ref m) = model {
            let url = url.clone();
            let m = m.clone();
            let handle = tokio::runtime::Handle::current();
            Arc::new(
                tokio::task::block_in_place(|| handle.block_on(OllamaEmbedding::connect(url, m)))
                    .expect("ollama connect"),
            )
        } else {
            Arc::new(HashEmbedding { dim: EMBED_DIM })
        }
    };

    // Pool `repeats` fresh passes into the point estimate. On the offline path a
    // single pass is deterministic; repeats matter only for --ollama-model.
    let reps = cli.repeats.max(1);
    let mut r1 = 0usize;
    let mut r3 = 0usize;
    let mut r5 = 0usize;
    let mut rr = 0.0f64;
    let mut errored = 0usize;
    for _ in 0..reps {
        let p = run_pass(&make_embedding, dim, &dataset).await;
        r1 += p.r1;
        r3 += p.r3;
        r5 += p.r5;
        rr += p.rr_sum;
        errored += p.errored;
    }
    let errored_per_pass = errored / reps;
    // Mean per-pass hits (integer on the deterministic path); Wilson over the
    // distinct query count n, never n*reps — repeats stabilise, they are not
    // independent samples.
    let hits1 = (r1 as f64 / reps as f64).round() as usize;
    let rate1 = r1 as f64 / (n * reps) as f64;
    let rate3 = r3 as f64 / (n * reps) as f64;
    let rate5 = r5 as f64 / (n * reps) as f64;
    let mrr = rr / (n * reps) as f64;
    let (lo1, hi1) = wilson_95(hits1, n);

    let date = cli
        .date
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    print_stdout(
        &cli,
        &embed_backend,
        n,
        rate1,
        lo1,
        hi1,
        rate3,
        rate5,
        mrr,
        errored_per_pass,
    );
    write_reports(
        &cli,
        &date,
        &sha,
        &embed_backend,
        n,
        rate1,
        lo1,
        hi1,
        rate3,
        rate5,
        mrr,
        errored_per_pass,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn print_stdout(
    cli: &Cli,
    embed_backend: &str,
    n: usize,
    rate1: f64,
    lo1: f64,
    hi1: f64,
    rate3: f64,
    rate5: f64,
    mrr: f64,
    errored: usize,
) {
    println!(
        "\n=== reproduction_bench (LoCoMo single-hop, mnemo default auto/RRF) — n={n}, \
         seed {:#x}, embedder={embed_backend} ===",
        cli.seed
    );
    println!(
        "observed recall@1 = {:.1}%  [Wilson95 {:.1}%, {:.1}%]   recall@3 = {:.1}%   recall@5 = {:.1}%   MRR = {:.3}   (errored-as-miss: {errored}/{n})",
        rate1 * 100.0,
        lo1 * 100.0,
        hi1 * 100.0,
        rate3 * 100.0,
        rate5 * 100.0,
        mrr
    );
    println!(
        "\nObserved is mnemo's OWN reproducible number (offline, fixed seed). The claimed \
         competitor figures are the vendors' published, cited numbers — NOT re-run in this harness."
    );
}

#[allow(clippy::too_many_arguments)]
fn write_reports(
    cli: &Cli,
    date: &str,
    sha: &str,
    embed_backend: &str,
    n: usize,
    rate1: f64,
    lo1: f64,
    hi1: f64,
    rate3: f64,
    rate5: f64,
    mrr: f64,
    errored: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let claims = published_claims();
    let mut claim_rows = String::new();
    for c in &claims {
        claim_rows.push_str(&format!(
            "| {} | {} | {} | [source]({}) |\n",
            c.system, c.claimed, c.note, c.source
        ));
    }

    let md = format!(
        "# reproduction_bench — claimed vs observed (LoCoMo single-hop)\n\n\
         > {date} — one well-known LoCoMo subtask (**single-hop retrieval**) re-run under mnemo's \
         **default** hybrid recall (`strategy=\"auto\"`: semantic + BM25 + graph-expansion + \
         recency, RRF-fused), **deterministic and offline**. Published next to competitors' \
         **own** LoCoMo figures — *cited, and NOT re-run in this harness*. Only mnemo's row is \
         reproducible here. Reproducibility-by-disclosure, riding the 2026 memory-benchmark \
         reproducibility crisis. No \"best\"/\"first\" claim.\n\n\
         - Dataset: LongMemEval_M single-hop slice (n={n}), SHA-256 `{sha}`.\n\
         - Embedder: `{embed_backend}`; seed `{seed:#x}`.\n\
         - Metric: gold-document recall@K + MRR (each query answerable from its own turn's \
         content; gold matched by `lme_id`). **Retrieval** quality — NOT the LLM-judged \
         end-to-end QA accuracy the vendors report (gated; [#44](https://github.com/sattyamjjain/mnemo/issues/44)).\n\n\
         ### Reproducibility method (two disclosed choices)\n\n\
         The default recall path is time- and approximation-dependent; two changes make the \
         number bit-reproducible without altering mnemo's fusion:\n\n\
         1. **Exact vector index.** mnemo's default index is USearch **HNSW** — an *approximate* \
         NN structure whose level-assignment RNG makes recall@k jitter run-to-run on tight-margin \
         text. This bench swaps in an **exact brute-force cosine** index (distance, then stable \
         insertion order on ties). HNSW is by construction an approximation of this exact search, \
         so it is the deterministic reference HNSW tracks; a production HNSW deployment sees values \
         within its approximate-NN noise floor. Every other lane (BM25, graph, the RRF fusion) is \
         mnemo's default.\n\
         2. **Recency neutralised.** The corpus is seeded in one batch, so every memory is equally \
         recent — a wall-clock recency signal carries no information here and only injects \
         run-to-run noise. The recency half-life is set to ~ages so `recency_score ≡ 1.0` for all \
         records (a constant lane), not dropped.\n\n\
         ## Observed (mnemo, reproducible offline)\n\n\
         | metric | value |\n|---|---:|\n\
         | **recall@1** | **{r1:.1}%** [Wilson 95% {lo1:.1}%, {hi1:.1}%] |\n\
         | recall@3 | {r3:.1}% |\n\
         | recall@5 | {r5:.1}% |\n\
         | MRR | {mrr:.3} |\n\n\
         > **Disclosure:** {errored}/{n} queries errored in the default `auto` BM25 lane \
         (Tantivy's query parser rejects some natural-language punctuation, e.g. the apostrophe in \
         *\"patient's\"*). The query is **not** sanitised — that would measure a non-default path — \
         so an errored recall surfaces no gold and is counted as an honest **miss**. The observed \
         rates above already include those misses. (Same handling as `semantic_recall_bench`; the \
         parser gap in default recall is a real, disclosed limitation, not hidden.)\n\n\
         ## Claimed (vendors' published LoCoMo figures — cited, NOT re-run here)\n\n\
         | system | claimed | note | source |\n|---|---|---|---|\n\
         {claim_rows}\n\
         **How to read this.** The *observed* row is mnemo's own number on a small bundled \
         retrieval slice, reproducible offline with a fixed seed and a Wilson-95 you can re-run. \
         The *claimed* rows are each vendor's **own published** figure at their own (often \
         LLM-judged, full-dataset) protocol — reproduced here **only as citations**, not re-run \
         in mnemo's harness. They are therefore **not a ranking against** the observed number: \
         different task (retrieval vs end-to-end QA), different dataset scale, different judge. \
         The corrected columns (Zep 84→58.44, MemPalace 100→60.3) are exactly why mnemo publishes \
         a re-runnable number instead of a headline. Reproduce: \
         `cargo run --release -p mnemo-locomo-bench --bin reproduction_bench`.\n",
        date = date,
        n = n,
        sha = sha,
        embed_backend = embed_backend,
        seed = cli.seed,
        r1 = rate1 * 100.0,
        lo1 = lo1 * 100.0,
        hi1 = hi1 * 100.0,
        r3 = rate3 * 100.0,
        r5 = rate5 * 100.0,
        mrr = mrr,
        errored = errored,
        claim_rows = claim_rows,
    );

    let json = serde_json::json!({
        "bench": "reproduction_bench",
        "subtask": "LoCoMo single-hop retrieval",
        "harness": "mnemo default auto/RRF recall, deterministic offline",
        "date": date,
        "dataset_sha256": sha,
        "n": n,
        "embedder": embed_backend,
        "seed": cli.seed,
        "reproducibility_method": {
            "vector_index": "exact brute-force cosine (deterministic reference; mnemo default is approximate USearch HNSW)",
            "recency": "neutralised (half-life -> infinity); batch-seeded corpus has no recency signal",
            "fusion": "mnemo default auto/RRF over vector + BM25 + graph unchanged",
        },
        "observed": {
            "recall@1": rate1,
            "recall@1_ci95": [lo1, hi1],
            "recall@3": rate3,
            "recall@5": rate5,
            "mrr": mrr,
            "errored_as_miss": errored,
            "errored_note": "queries that erred in the default auto BM25 lane (unsanitized natural-language punctuation); counted as misses, already reflected in the rates above",
        },
        "claimed_not_rerun_here": claims.iter().map(|c| serde_json::json!({
            "system": c.system, "claimed": c.claimed, "note": c.note, "source": c.source,
        })).collect::<Vec<_>>(),
        "honesty": "claimed figures are vendors' own published numbers, cited not re-run; only the observed row is reproducible in this harness; not a ranking (retrieval vs end-to-end QA, different scale/judge)",
    });

    std::fs::create_dir_all(&cli.out_dir)?;
    std::fs::write(cli.out_dir.join(format!("reproduction_{date}.md")), md)?;
    std::fs::write(
        cli.out_dir.join(format!("reproduction_{date}.json")),
        serde_json::to_string_pretty(&json)?,
    )?;
    println!(
        "wrote {}\nwrote {}",
        cli.out_dir
            .join(format!("reproduction_{date}.md"))
            .display(),
        cli.out_dir
            .join(format!("reproduction_{date}.json"))
            .display()
    );
    Ok(())
}
