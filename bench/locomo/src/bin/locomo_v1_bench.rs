//! `locomo_v1_bench` — mnemo's first **real-embedder** retrieval benchmark.
//!
//! Measures gold-document retrieval quality on the bundled LoCoMo-/LongMemEval-
//! style slice through a real [`MnemoEngine`] (in-memory DuckDB + USearch HNSW +
//! Tantivy BM25), with a **real semantic embedder** — never `NoopEmbedding`.
//!
//! # Embedder (real, no paid gate)
//!
//! Resolved by `--embedder` (default **`onnx`**):
//! - **`onnx`** (default; reproducible with no API key) — local ONNX
//!   sentence-transformer via `MNEMO_ONNX_MODEL_PATH` (e.g. `all-MiniLM-L6-v2`).
//!   Requires building with `--features onnx`.
//! - **`openai`** — `OpenAiEmbedding` behind `OPENAI_API_KEY` (`text-embedding-3-small`).
//! - **`ollama`** — local Ollama HTTP (`nomic-embed-text`), no key.
//!
//! # Hard guard
//!
//! Before scoring, the resolved embedder is routed through
//! [`guard_real_embedder`](mnemo_locomo_bench::real_embedder::guard_real_embedder):
//! if it is not semantic-capable (i.e. resolved to the zero-vector no-op) the
//! run **refuses to emit a score** and names the embedder. A silently-noop
//! benchmark is worse than no benchmark.
//!
//! # Metrics
//!
//! Per strategy (`lexical`, `semantic`, `auto`): **recall@{1,5,10}** with a
//! **Wilson 95%** interval over `n` queries, **MRR**, **p50/p95** query latency
//! (includes the embed round-trip for vector/hybrid), and one-time **index build
//! time**. Reported over `--repeats` seeds to absorb UUID-v7 + approximate-HNSW
//! run-to-run variance; the CI is on the mean recall over `n`. Small `n`
//! (< 100) is labelled `preliminary`.
//!
//! # Reproduce (ONNX default, no credentials)
//!
//! ```text
//! # one-time: fetch a public sentence-transformer ONNX model + tokenizer
//! #   model.onnx + tokenizer.json  (e.g. sentence-transformers/all-MiniLM-L6-v2)
//! MNEMO_ONNX_MODEL_PATH=/path/to/all-MiniLM-L6-v2/model.onnx \
//!   cargo run --release --features onnx -p mnemo-locomo-bench --bin locomo_v1_bench
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;

use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::{Error, Result as MnemoResult};
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

use mnemo_locomo_bench::dataset::{LongMemRecord, dataset_sha, default_dataset_path, load_dataset};
use mnemo_locomo_bench::real_embedder::guard_real_embedder;
use mnemo_locomo_bench::stats::wilson_95;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// ---------------------------------------------------------------------------
// Ollama HTTP embedder (local, no key) — for reproduction on a box without ONNX
// ---------------------------------------------------------------------------
struct OllamaEmbedding {
    client: reqwest::Client,
    url: String,
    model: String,
    dimensions: usize,
}

impl OllamaEmbedding {
    async fn connect(url: String, model: String) -> MnemoResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Embedding(format!("http client: {e}")))?;
        let probe = Self {
            client,
            url,
            model,
            dimensions: 0,
        };
        let v = probe.embed_raw("dimensionality probe").await.map_err(|e| {
            Error::Embedding(format!(
                "{e} — is Ollama running and the model pulled? Try: `ollama pull {}`",
                probe.model
            ))
        })?;
        let dimensions = v.len();
        if dimensions == 0 {
            return Err(Error::Embedding(
                "embedder returned a 0-length vector".into(),
            ));
        }
        Ok(Self {
            dimensions,
            ..probe
        })
    }

    async fn embed_raw(&self, text: &str) -> MnemoResult<Vec<f32>> {
        let resp = self
            .client
            .post(&self.url)
            .json(&serde_json::json!({ "model": self.model, "prompt": text }))
            .send()
            .await
            .map_err(|e| Error::Embedding(format!("ollama request failed: {e}")))?;
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
        let arr = body
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| Error::Embedding("response missing 'embedding' array".into()))?;
        Ok(arr
            .iter()
            .map(|x| x.as_f64().unwrap_or(0.0) as f32)
            .collect())
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OllamaEmbedding {
    async fn embed(&self, text: &str) -> MnemoResult<Vec<f32>> {
        self.embed_raw(text).await
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnemoResult<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed_raw(t).await?);
        }
        Ok(out)
    }
    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------
#[derive(Parser, Debug)]
#[command(
    name = "locomo_v1_bench",
    about = "mnemo's first real-embedder LoCoMo retrieval numbers"
)]
struct Cli {
    /// Embedder backend: `onnx` (default, no key), `openai`, or `ollama`.
    #[arg(long, default_value = "onnx")]
    embedder: String,
    /// ONNX model path (tokenizer.json must sit beside it).
    #[arg(long, env = "MNEMO_ONNX_MODEL_PATH")]
    onnx_model: Option<PathBuf>,
    /// ONNX embedding dimension (all-MiniLM-L6-v2 = 384).
    #[arg(long, default_value_t = 384)]
    onnx_dim: usize,
    /// OpenAI embedding model.
    #[arg(long, default_value = "text-embedding-3-small")]
    openai_model: String,
    /// OpenAI embedding dimension.
    #[arg(long, default_value_t = 1536)]
    openai_dim: usize,
    #[arg(long, default_value = "http://localhost:11434/api/embeddings")]
    ollama_url: String,
    #[arg(long, default_value = "nomic-embed-text")]
    ollama_model: String,
    /// Top-K per query (must be >= 10 for recall@10).
    #[arg(long, default_value_t = 10)]
    limit: usize,
    /// Seeds averaged over to absorb UUID/HNSW run-to-run variance.
    #[arg(long, default_value_t = 3)]
    repeats: usize,
    /// Hardware label recorded in the result (defaults to `<arch>/<os>`).
    #[arg(long)]
    hardware: Option<String>,
    #[arg(long, default_value = "bench/results/locomo_v1.json")]
    out: PathBuf,
    #[arg(long, env = "MNEMO_LONGMEMEVAL_PATH")]
    dataset: Option<PathBuf>,
}

struct EmbedderMeta {
    backend: String,
    model: String,
    dim: usize,
}

async fn resolve_embedder(cli: &Cli) -> Result<(Arc<dyn EmbeddingProvider>, EmbedderMeta), BoxErr> {
    match cli.embedder.as_str() {
        "onnx" => {
            #[cfg(feature = "onnx")]
            {
                let path = cli.onnx_model.clone().ok_or_else(|| {
                    "onnx embedder needs --onnx-model (or MNEMO_ONNX_MODEL_PATH)".to_string()
                })?;
                let model_str = path.to_string_lossy().to_string();
                let e = mnemo_core::embedding::onnx::OnnxEmbedding::new(&model_str, cli.onnx_dim)?;
                let model_name = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "onnx".to_string());
                Ok((
                    Arc::new(e),
                    EmbedderMeta {
                        backend: "onnx".into(),
                        model: model_name,
                        dim: cli.onnx_dim,
                    },
                ))
            }
            #[cfg(not(feature = "onnx"))]
            {
                Err(
                    "this binary was built WITHOUT the `onnx` feature; rebuild with \
                     `--features onnx` (or use `--embedder ollama`)"
                        .into(),
                )
            }
        }
        "openai" => {
            let key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| "openai embedder needs OPENAI_API_KEY".to_string())?;
            let e = mnemo_core::embedding::openai::OpenAiEmbedding::new(
                key,
                cli.openai_model.clone(),
                cli.openai_dim,
            );
            Ok((
                Arc::new(e),
                EmbedderMeta {
                    backend: "openai".into(),
                    model: cli.openai_model.clone(),
                    dim: cli.openai_dim,
                },
            ))
        }
        "ollama" => {
            let e =
                OllamaEmbedding::connect(cli.ollama_url.clone(), cli.ollama_model.clone()).await?;
            let dim = e.dimensions();
            Ok((
                Arc::new(e),
                EmbedderMeta {
                    backend: "ollama".into(),
                    model: cli.ollama_model.clone(),
                    dim,
                },
            ))
        }
        other => Err(format!("unknown --embedder '{other}' (expected onnx|openai|ollama)").into()),
    }
}

// ---------------------------------------------------------------------------
// Bench core
// ---------------------------------------------------------------------------
fn build_engine(embedding: Arc<dyn EmbeddingProvider>, dim: usize) -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(dim).unwrap());
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
    MnemoEngine::new(
        storage,
        index,
        embedding,
        "locomo-v1-bench".to_string(),
        None,
    )
    .with_full_text(ft)
}

async fn seed(engine: &MnemoEngine, dataset: &[LongMemRecord]) {
    for r in dataset {
        let mut req = RememberRequest::new(r.content.clone());
        req.importance = Some(0.5);
        req.tags = Some(r.tags.clone());
        req.thread_id = Some(r.conversation_id.clone());
        req.metadata = Some(serde_json::json!({ "lme_id": r.id }));
        engine.remember(req).await.expect("seed remember failed");
    }
}

fn recall_req(query: &str, strategy: &str, limit: usize) -> RecallRequest {
    let mut r = RecallRequest::new(query.to_string());
    r.limit = Some(limit);
    r.strategy = Some(strategy.to_string());
    r
}

fn percentile(values: &mut [f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = (q * (values.len() - 1) as f64).round() as usize;
    values[idx.min(values.len() - 1)]
}

struct StrategyResult {
    strategy: String,
    // mean recall over repeats
    recall1: f64,
    recall5: f64,
    recall10: f64,
    // wilson-95 on round(mean_recall * n) over n
    ci1: (f64, f64),
    ci5: (f64, f64),
    ci10: (f64, f64),
    mrr: f64,
    p50_ms: f64,
    p95_ms: f64,
    index_build_ms: f64,
}

async fn run_strategy(
    embedding: Arc<dyn EmbeddingProvider>,
    dim: usize,
    strategy: &str,
    dataset: &[LongMemRecord],
    limit: usize,
    repeats: usize,
) -> StrategyResult {
    let n = dataset.len();
    let mut r1s = Vec::new();
    let mut r5s = Vec::new();
    let mut r10s = Vec::new();
    let mut mrrs = Vec::new();
    let mut all_lat = Vec::new();
    let mut index_build_ms = 0.0;

    for rep in 0..repeats.max(1) {
        let engine = build_engine(embedding.clone(), dim);
        let t0 = Instant::now();
        seed(&engine, dataset).await;
        if rep == 0 {
            index_build_ms = t0.elapsed().as_secs_f64() * 1000.0;
        }
        let (mut h1, mut h5, mut h10) = (0usize, 0usize, 0usize);
        let mut rr = 0.0f64;
        for r in dataset {
            let started = Instant::now();
            let resp = engine.recall(recall_req(&r.query, strategy, limit)).await;
            all_lat.push(started.elapsed().as_secs_f64() * 1000.0);
            if let Ok(resp) = resp
                && let Some(rank) = resp
                    .memories
                    .iter()
                    .position(|m| {
                        m.metadata.get("lme_id").and_then(|v| v.as_str()) == Some(r.id.as_str())
                    })
                    .map(|i| i + 1)
            {
                if rank <= 1 {
                    h1 += 1;
                }
                if rank <= 5 {
                    h5 += 1;
                }
                if rank <= 10 {
                    h10 += 1;
                }
                rr += 1.0 / rank as f64;
            }
        }
        r1s.push(h1 as f64 / n as f64);
        r5s.push(h5 as f64 / n as f64);
        r10s.push(h10 as f64 / n as f64);
        mrrs.push(rr / n as f64);
    }

    let mean = |xs: &[f64]| xs.iter().sum::<f64>() / xs.len().max(1) as f64;
    let recall1 = mean(&r1s);
    let recall5 = mean(&r5s);
    let recall10 = mean(&r10s);
    // Wilson-95 on the mean recall expressed as successes/n (rounded to the
    // nearest integer count over the n queries).
    let ci = |rec: f64| wilson_95((rec * n as f64).round() as usize, n);

    StrategyResult {
        strategy: strategy.to_string(),
        recall1,
        recall5,
        recall10,
        ci1: ci(recall1),
        ci5: ci(recall5),
        ci10: ci(recall10),
        mrr: mean(&mrrs),
        p50_ms: percentile(&mut all_lat.clone(), 0.50),
        p95_ms: percentile(&mut all_lat, 0.95),
        index_build_ms,
    }
}

fn ci_arr((lo, hi): (f64, f64)) -> serde_json::Value {
    serde_json::json!([round3(lo), round3(hi)])
}
fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

#[tokio::main]
async fn main() -> Result<(), BoxErr> {
    let cli = Cli::parse();
    assert!(cli.limit >= 10, "--limit must be >= 10 for recall@10");

    let dataset_path = cli.dataset.clone().unwrap_or_else(default_dataset_path);
    let dataset = load_dataset(&dataset_path);
    assert!(!dataset.is_empty(), "dataset is empty");
    let sha = dataset_sha(&dataset_path);
    let n = dataset.len();
    let preliminary = n < 100;
    let hardware = cli
        .hardware
        .clone()
        .unwrap_or_else(|| format!("{}/{}", std::env::consts::ARCH, std::env::consts::OS));

    // Resolve + GUARD the embedder. Refuse to score under a no-op embedder.
    let (embedding, meta) = resolve_embedder(&cli).await?;
    guard_real_embedder(&*embedding, &meta.backend)?;
    eprintln!(
        "embedder OK: backend={} model={} dim={} (semantic-capable)",
        meta.backend, meta.model, meta.dim
    );

    let strategies = ["lexical", "semantic", "auto"];
    let mut results = Vec::new();
    for s in strategies {
        let r = run_strategy(
            embedding.clone(),
            meta.dim,
            s,
            &dataset,
            cli.limit,
            cli.repeats,
        )
        .await;
        results.push(r);
    }

    // Deterministic JSON: sorted keys (serde_json Map), NO wall-clock/timestamp.
    let mut per_strategy = serde_json::Map::new();
    for r in &results {
        per_strategy.insert(
            r.strategy.clone(),
            serde_json::json!({
                "recall@1": round3(r.recall1),
                "recall@1_ci95": ci_arr(r.ci1),
                "recall@5": round3(r.recall5),
                "recall@5_ci95": ci_arr(r.ci5),
                "recall@10": round3(r.recall10),
                "recall@10_ci95": ci_arr(r.ci10),
                "mrr": round3(r.mrr),
                "p50_ms": round3(r.p50_ms),
                "p95_ms": round3(r.p95_ms),
                "index_build_ms": round3(r.index_build_ms),
            }),
        );
    }
    let payload = serde_json::json!({
        "bench": "locomo_v1",
        "corpus": {
            "dataset": display_rel(&dataset_path),
            "note": "bundled LongMemEval_M slice (the repo's LoCoMo-style stand-in); gated full LoCoMo/LongMemEval are out of scope (#44)",
            "records": dataset.len(),
            "sha256": sha,
        },
        "embedder": { "backend": meta.backend, "dim": meta.dim, "model": meta.model },
        "hardware": hardware,
        "limit": cli.limit,
        "metric": "gold-document recall@k (each query's source record is its gold doc, matched by lme_id)",
        "n": n,
        "preliminary": preliminary,
        "repeats": cli.repeats,
        "strategies": per_strategy,
    });

    if let Some(parent) = cli.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cli.out, serde_json::to_string_pretty(&payload)? + "\n")?;

    // Console summary.
    println!(
        "\n=== locomo_v1_bench — {backend} {model} ({dim}-dim) — n={n} queries, mean of {reps} seeds{prelim} ===",
        backend = meta.backend,
        model = meta.model,
        dim = meta.dim,
        n = n,
        reps = cli.repeats,
        prelim = if preliminary {
            " [PRELIMINARY: n<100]"
        } else {
            ""
        },
    );
    println!(
        "{:<10} {:>9} {:>18} {:>9} {:>10} {:>7} {:>8} {:>8} {:>12}",
        "strategy",
        "recall@1",
        "r@1 95%CI",
        "recall@5",
        "recall@10",
        "MRR",
        "p50ms",
        "p95ms",
        "build_ms"
    );
    for r in &results {
        println!(
            "{:<10} {:>9.3} {:>8.3}-{:<8.3} {:>9.3} {:>10.3} {:>7.3} {:>8.1} {:>8.1} {:>12.1}",
            r.strategy,
            r.recall1,
            r.ci1.0,
            r.ci1.1,
            r.recall5,
            r.recall10,
            r.mrr,
            r.p50_ms,
            r.p95_ms,
            r.index_build_ms
        );
    }
    println!("\nwrote {}", cli.out.display());
    Ok(())
}

fn display_rel(p: &std::path::Path) -> String {
    std::fs::canonicalize(p)
        .ok()
        .and_then(|abs| {
            std::env::current_dir()
                .ok()
                .and_then(|cwd| abs.strip_prefix(&cwd).ok().map(|r| r.display().to_string()))
        })
        .unwrap_or_else(|| p.display().to_string())
}
