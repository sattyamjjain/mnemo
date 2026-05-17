//! v0.4.4 (2026-05-17 — Suggestion 1 scaffold) — LongMemEval-shaped
//! recall replay across three [`mnemo_core::query::recall`] strategies
//! (`"lexical"` / `"semantic"` / `"auto"` → BM25-only / vector-only /
//! RRF-hybrid).
//!
//! # What this is
//!
//! Reproduces the Sen et al. arXiv:2605.15184 experiment design
//! ("grep vs vector retrieval inside agent harnesses") against mnemo's
//! own `mnemo.recall` tool. The bin is operator-runnable today against
//! the bundled 45-record synthesized LongMemEval_M slice; the full
//! 116-question gated LongMemEval run + GPT-judge-scored metric is
//! gated behind the same secrets ledger as
//! [issue #44](https://github.com/sattyamjjain/mnemo/issues/44).
//!
//! # What this is NOT
//!
//! - **Not** the official LongMemEval metric. The smoke run uses a
//!   deterministic exact-substring match against the `expected`
//!   field; the official GPT-judge scoring requires `OPENAI_API_KEY`
//!   or `ANTHROPIC_API_KEY` and is gated behind #44.
//! - **Not** a perf comparison against the paper's published numbers.
//!   The bundled dataset is synthesized (45 medical-dialogue records)
//!   so the absolute accuracy numbers are not directly comparable to
//!   the paper. The bin's purpose is the *wiring*: confirming each
//!   mode routes through mnemo's recall path end-to-end.
//! - **Not** running with a real embedder. The scaffold uses
//!   [`NoopEmbedding`] (zero vectors), so the vector-only mode will
//!   report degenerate accuracy. This is *by design* — it primes the
//!   operator to swap in `OnnxEmbedding` or `OpenAiEmbedding` for the
//!   gated run, and demonstrates that the wiring works while the
//!   reported number is meaningless until a real embedder lands.
//!
//! # Modes covered
//!
//! - `vector_only` → `RecallRequest.strategy = Some("semantic")`
//! - `bm25_only`   → `RecallRequest.strategy = Some("lexical")`
//! - `rrf_hybrid`  → `RecallRequest.strategy = Some("auto")` (mnemo's
//!   default RRF fusion across vector + BM25 + recency + decay)
//!
//! The fourth existing strategy `"graph"` is intentionally omitted —
//! it requires a relation graph the bundled dataset does not carry.
//! Adding a graph slice is a follow-up.
//!
//! # Output
//!
//! Writes a single Markdown table to
//! `bench/locomo/results/grep_vs_vector_<YYYY-MM-DD>.md` with:
//!
//! - Per-mode accuracy (smoke metric: % of queries where `expected`
//!   appears as a substring in any of the top-K hit contents).
//! - Per-mode p50 / p95 latency (deterministic; no judge call).
//! - The dataset SHA so the row is reproducible.
//! - The honest-disclaimer block naming what the smoke run does and
//!   does NOT measure (re-stated for the reader who skipped this
//!   module doc).
//!
//! # Usage
//!
//! ```text
//! cargo run --release --bin grep_vs_vector_replay -p mnemo-locomo-bench
//!
//! # Override the dataset (gated full-116 set):
//! MNEMO_LONGMEMEVAL_PATH=/path/to/longmemeval_s.jsonl \
//!     cargo run --release --bin grep_vs_vector_replay -p mnemo-locomo-bench
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

/// LongMemEval-shaped record. Mirrors the existing
/// `crates/mnemo-core/benches/longmemeval_bench.rs::LongMemRecord`
/// shape so the bundled dataset works without re-serialisation.
#[derive(Debug, Deserialize)]
struct LongMemRecord {
    #[allow(dead_code)]
    id: String,
    conversation_id: String,
    turn: u32,
    content: String,
    tags: Vec<String>,
    query: String,
    expected: String,
}

#[derive(Parser, Debug)]
#[command(name = "grep_vs_vector_replay")]
struct Cli {
    /// Top-K retrieved per query.
    #[arg(long, default_value_t = 5)]
    limit: usize,
    /// Output directory.
    #[arg(long, default_value = "bench/locomo/results")]
    out_dir: PathBuf,
    /// Override dataset path. Defaults to the bundled
    /// `crates/mnemo-core/benches/data/longmemeval_m.jsonl` shipped
    /// inside mnemo-core (see `MNEMO_LONGMEMEVAL_PATH` env override).
    #[arg(long, env = "MNEMO_LONGMEMEVAL_PATH")]
    dataset: Option<PathBuf>,
}

/// One of the three modes we route through `mnemo.recall`. The strings
/// are exactly the values the current `RecallRequest.strategy:
/// Option<String>` API accepts (see `crates/mnemo-core/src/query/recall.rs`).
#[derive(Debug, Clone, Copy)]
struct Mode {
    name: &'static str,
    strategy: &'static str,
}

const MODES: [Mode; 3] = [
    Mode {
        name: "vector_only",
        strategy: "semantic",
    },
    Mode {
        name: "bm25_only",
        strategy: "lexical",
    },
    Mode {
        name: "rrf_hybrid",
        strategy: "auto",
    },
];

#[derive(Debug)]
struct ModeResult {
    name: &'static str,
    hits: usize,
    /// Queries where `mnemo.recall` returned an error (e.g. Tantivy
    /// query-parser syntax error on punctuation-bearing questions).
    /// Failures count against the denominator so the accuracy number
    /// does not silently look better than reality.
    failures: usize,
    total: usize,
    latencies_ms: Vec<f64>,
}

impl ModeResult {
    /// Accuracy is `hits / total`. Failures are NOT subtracted from
    /// the denominator — they count as misses so the headline number
    /// stays comparable across modes even when one mode's query
    /// parser is stricter than another's.
    fn accuracy(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.hits as f64 / self.total as f64
        }
    }
    fn p50_ms(&self) -> f64 {
        percentile(&self.latencies_ms, 0.50)
    }
    fn p95_ms(&self) -> f64 {
        percentile(&self.latencies_ms, 0.95)
    }
}

fn percentile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = (q * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn default_dataset_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("mnemo-core")
        .join("benches")
        .join("data")
        .join("longmemeval_m.jsonl")
}

fn load_dataset(path: &Path) -> Vec<LongMemRecord> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read LongMemEval dataset at {path:?}: {e}"));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str::<LongMemRecord>(l)
                .unwrap_or_else(|e| panic!("invalid LongMem record: {e}; line: {l}"))
        })
        .collect()
}

fn dataset_sha(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(&bytes);
    hex::encode(h.finalize())
}

fn build_engine() -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(3).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
    MnemoEngine::new(
        storage,
        index,
        embedding,
        "grep-vs-vector-bench-agent".to_string(),
        None,
    )
    .with_full_text(ft)
}

async fn seed(engine: &MnemoEngine, dataset: &[LongMemRecord]) {
    for r in dataset {
        let req = RememberRequest {
            content: r.content.clone(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.5),
            tags: Some(r.tags.clone()),
            metadata: Some(serde_json::json!({
                "conversation_id": r.conversation_id,
                "turn": r.turn,
                "lme_id": r.id,
            })),
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: Some(r.conversation_id.clone()),
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        };
        engine.remember(req).await.unwrap();
    }
}

fn build_recall(query: &str, strategy: &str, limit: usize) -> RecallRequest {
    RecallRequest {
        query: query.to_string(),
        agent_id: None,
        limit: Some(limit),
        memory_type: None,
        memory_types: None,
        scope: None,
        min_importance: None,
        tags: None,
        org_id: None,
        strategy: Some(strategy.to_string()),
        temporal_range: None,
        recency_half_life_hours: None,
        hybrid_weights: None,
        rrf_k: None,
        as_of: None,
        explain: None,
        with_provenance: None,
    }
}

async fn run_mode(
    engine: &MnemoEngine,
    mode: Mode,
    dataset: &[LongMemRecord],
    limit: usize,
) -> ModeResult {
    let mut hits = 0_usize;
    let mut failures = 0_usize;
    let mut latencies_ms = Vec::with_capacity(dataset.len());
    for r in dataset {
        let req = build_recall(&r.query, mode.strategy, limit);
        let started = Instant::now();
        let result = engine.recall(req).await;
        let dt = started.elapsed();
        latencies_ms.push(dt.as_secs_f64() * 1000.0);
        match result {
            Ok(response) => {
                let expected_lc = r.expected.to_lowercase();
                let any_match = response
                    .memories
                    .iter()
                    .any(|m| m.content.to_lowercase().contains(&expected_lc));
                if any_match {
                    hits += 1;
                }
            }
            Err(e) => {
                failures += 1;
                tracing::debug!(
                    mode = mode.name,
                    error = %e,
                    "recall failed (counted as miss)"
                );
            }
        }
    }
    ModeResult {
        name: mode.name,
        hits,
        failures,
        total: dataset.len(),
        latencies_ms,
    }
}

fn render_markdown(
    results: &[ModeResult],
    dataset_path: &Path,
    dataset_sha_hex: &str,
    limit: usize,
) -> String {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut md = String::new();
    md.push_str(&format!("# grep_vs_vector_replay — {date}\n\n"));
    md.push_str(
        "> **Scaffold run** reproducing the Sen et al. arXiv:2605.15184 \
         experiment design (grep vs vector retrieval inside an agent \
         harness) against mnemo's `mnemo.recall` tool. Smoke metric \
         only — see disclaimer below.\n\n",
    );
    md.push_str("## Setup\n\n");
    md.push_str(&format!("- Dataset: `{}`\n", dataset_path.display()));
    md.push_str(&format!("- Dataset SHA-256: `{dataset_sha_hex}`\n"));
    md.push_str(&format!(
        "- Records: {}\n",
        results.first().map(|r| r.total).unwrap_or(0)
    ));
    md.push_str(&format!("- Top-K per query: {limit}\n"));
    md.push_str(
        "- Engine: in-memory DuckDB storage + USearch HNSW (dim=3) + \
         NoopEmbedding (zero vectors) + Tantivy BM25 full-text\n\n",
    );
    md.push_str("## Results\n\n");
    md.push_str("| Mode (CLI) | mnemo strategy | Accuracy (smoke) | Query failures | p50 latency (ms) | p95 latency (ms) |\n");
    md.push_str("|---|---|---:|---:|---:|---:|\n");
    for r in results {
        let strategy = MODES
            .iter()
            .find(|m| m.name == r.name)
            .map(|m| m.strategy)
            .unwrap_or("");
        md.push_str(&format!(
            "| `{}` | `\"{}\"` | {:.1}% ({}/{}) | {}/{} | {:.2} | {:.2} |\n",
            r.name,
            strategy,
            r.accuracy() * 100.0,
            r.hits,
            r.total,
            r.failures,
            r.total,
            r.p50_ms(),
            r.p95_ms(),
        ));
    }
    md.push_str(
        "\n*Query failures* count as misses in the accuracy column. The \
         common cause is Tantivy's BM25 query parser rejecting queries \
         with un-escaped punctuation (apostrophes, question marks in \
         certain positions). The failure column lets a reader see when \
         a mode's accuracy is dragged down by parser strictness vs by \
         the substrate's actual recall behaviour.\n",
    );
    md.push_str(
        "\n## Honest-disclaimer block\n\n\
         - **Not the official LongMemEval metric.** This bin uses a \
         deterministic exact-substring match (`expected ⊆ any hit's \
         content`) so a smoke run is reproducible without an API key. \
         The official GPT-judge-scored metric requires \
         `OPENAI_API_KEY` or `ANTHROPIC_API_KEY` and is gated behind \
         [#44](https://github.com/sattyamjjain/mnemo/issues/44).\n\
         - **NoopEmbedding makes `vector_only` degenerate.** The \
         scaffold ships zero-vector embeddings so the wiring is \
         self-contained. Swap to `OnnxEmbedding` or `OpenAiEmbedding` \
         for the gated run; the absolute `vector_only` accuracy here \
         is meaningless until a real embedder lands.\n\
         - **Bundled dataset is synthesized, not the published LongMemEval.** \
         45 medical-dialogue records under \
         `crates/mnemo-core/benches/data/longmemeval_m.jsonl`. \
         Override via `MNEMO_LONGMEMEVAL_PATH` for the real 116-question \
         LongMemEval slice (gated dataset access).\n\
         - **Comparison shape, not magnitudes.** The intent of this bin \
         is to confirm each mode routes through mnemo's recall path \
         end-to-end and to give operators a runnable scaffold for the \
         gated comparison; absolute numbers from the smoke run are \
         not comparable to the paper's published numbers.\n\n\
         ## Reproducing\n\n\
         ```text\n\
         cargo run --release --bin grep_vs_vector_replay -p mnemo-locomo-bench\n\
         ```\n",
    );
    md
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let dataset_path = cli.dataset.unwrap_or_else(default_dataset_path);
    let dataset = load_dataset(&dataset_path);
    assert!(
        !dataset.is_empty(),
        "LongMemEval dataset is empty — check MNEMO_LONGMEMEVAL_PATH or the bundled file"
    );
    let dataset_sha_hex = dataset_sha(&dataset_path);

    tracing::info!(
        records = dataset.len(),
        dataset = %dataset_path.display(),
        sha = %dataset_sha_hex,
        "starting grep_vs_vector_replay scaffold"
    );

    let mut results = Vec::with_capacity(MODES.len());
    for mode in MODES {
        let engine = build_engine();
        seed(&engine, &dataset).await;
        let r = run_mode(&engine, mode, &dataset, cli.limit).await;
        tracing::info!(
            mode = mode.name,
            strategy = mode.strategy,
            hits = r.hits,
            total = r.total,
            accuracy_pct = format!("{:.1}", r.accuracy() * 100.0).as_str(),
            p50_ms = format!("{:.2}", r.p50_ms()).as_str(),
            p95_ms = format!("{:.2}", r.p95_ms()).as_str(),
            "mode complete"
        );
        results.push(r);
    }

    let md = render_markdown(&results, &dataset_path, &dataset_sha_hex, cli.limit);
    std::fs::create_dir_all(&cli.out_dir)?;
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let out_path = cli.out_dir.join(format!("grep_vs_vector_{date}.md"));
    std::fs::write(&out_path, md)?;
    tracing::info!(out = %out_path.display(), "report written");
    println!("wrote {}", out_path.display());
    Ok(())
}
