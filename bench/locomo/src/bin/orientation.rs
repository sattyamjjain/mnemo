//! v0.4.8 — Orientation-cache vs hybrid-only repeated-context bench.
//!
//! # Anchor
//!
//! [arXiv:2605.19932](https://arxiv.org/abs/2605.19932) (PEEK —
//! Prefix-Encoded Episodic Knowledge) shows that a small,
//! token-budgeted "orientation map" maintained alongside an agent's
//! retrieval surface lets agents re-enter long-running contexts
//! with a fraction of the recall payload. The scenario here
//! mirrors that shape against mnemo's `mnemo.recall` tool: compares
//! the default hybrid-only path against the v0.4.8 opt-in
//! orientation-cache mode
//! ([`mnemo_core::query::orientation_cache`]).
//!
//! # Scenario
//!
//! For each `K ∈ {3, 6, 10, 15}` (number of *repeated-context*
//! recall calls per trial):
//!
//! 1. Stand up a fresh in-memory `MnemoEngine` (DuckDB + USearch +
//!    Tantivy + `OrientationCacheStore`).
//! 2. REMEMBER a shared synthetic context: 30 facts referencing a
//!    fixed cast of `Entity` names + `UPPER_SNAKE` constants +
//!    one fenced schema block. The cast does not change across
//!    the trial — only the surface queries do.
//! 3. Issue `K` related recall queries.
//!     - **hybrid-only arm:** standard hybrid recall, ignore
//!       orientation map.
//!     - **orientation-cache arm:** standard hybrid recall +
//!       opt-in orientation cache (default token budget = 512).
//! 4. Score each arm:
//!     - `top-1 recall` on the final query (deterministic
//!       contains-check against a known target string).
//!     - `payload tokens` per call (sum across hits + rendered
//!       map). The orientation-cache arm should hit **lower
//!       per-call cost at matched top-1** after the map warms up,
//!       because the constant-token map amortises across calls.
//! 5. Repeat each `K` `N=10` times for stability.
//!
//! # Output
//!
//! Writes a Markdown table to
//! `bench/locomo/results/orientation_<YYYY-MM-DD>.md`. One row
//! per `K`: hybrid p50 tokens, orientation p50 tokens, headline
//! delta, top-1 hit-rate parity.
//!
//! # What this bin is NOT
//!
//! - **Not a faithful PEEK reproduction.** PEEK uses a learned
//!   prefix encoder. This bin uses the v0.4.8 heuristic Distiller
//!   (regex-free, zero-dep); the bench measures the *shape* of
//!   the savings, not the absolute number PEEK reports.
//! - **Not a perf bench.** Latency is recorded but not the
//!   headline. The signal is *bounded payload tokens at matched
//!   recall*.
//! - **Not a real embedder run.** Uses `NoopEmbedding` (dim=3) so
//!   the wiring is self-contained. Vector lane is degenerate by
//!   design; BM25 + the orientation map carry the signal.
//! - **Tokens are heuristic estimates (~4 chars per token).** The
//!   delta is meaningful within this bin; comparisons against
//!   `tiktoken-rs`-counted budgets need a separate calibration
//!   run.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::orientation_cache::{OrientationCacheConfig, OrientationCacheStore};
use mnemo_core::query::recall::{RecallRequest, ScoredMemory};
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

const TRIALS_PER_K: usize = 10;
const K_VALUES: [usize; 4] = [3, 6, 10, 15];
const CONTEXT_FACTS: usize = 30;

#[derive(Parser, Debug)]
#[command(name = "orientation")]
struct Cli {
    #[arg(long, default_value = "bench/locomo/results")]
    out_dir: PathBuf,
    #[arg(long, default_value_t = TRIALS_PER_K)]
    trials: usize,
}

fn build_engine() -> (MnemoEngine, Arc<OrientationCacheStore>) {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(3).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
    let store = OrientationCacheStore::new();
    let engine = MnemoEngine::new(
        storage,
        index,
        embedding,
        "orientation-bench-agent".to_string(),
        None,
    )
    .with_full_text(ft)
    .with_orientation_cache_store(store.clone());
    (engine, store)
}

async fn seed_shared_context(engine: &MnemoEngine, trial: usize) {
    // Cast of entities + constants + a fenced schema. Reused across
    // the K recall calls in a trial; the orientation map should
    // converge to a stable representation after the first 1-2 hits.
    let entities = [
        "FrobnicatorService",
        "QuxClient",
        "PipelineAlpha",
        "BravoQueue",
        "CharlieIndex",
    ];
    let constants = [
        ("API_BASE", "https://api.example.com"),
        ("MAX_RETRIES", "5"),
        ("TIMEOUT_MS", "2500"),
    ];
    for i in 0..CONTEXT_FACTS {
        let entity = entities[i % entities.len()];
        let (k, v) = constants[i % constants.len()];
        let content = if i % 7 == 0 {
            format!(
                "{entity} configuration: {k} = {v}\n```sql\nCREATE TABLE orders (id BIGINT);\n```"
            )
        } else {
            format!(
                "{entity} interacts with {k} ({v}) under repeated-context conditions (trial {trial}, fact #{i})"
            )
        };
        let mut req = RememberRequest::new(content);
        req.tags = Some(vec!["orientation-bench".to_string(), entity.to_string()]);
        engine.remember(req).await.unwrap();
    }
}

fn build_recall(query: &str, with_orientation: bool) -> RecallRequest {
    let mut req = RecallRequest::new(query.to_string());
    req.limit = Some(8);
    req.strategy = Some("auto".to_string());
    req.tags = Some(vec!["orientation-bench".to_string()]);
    if with_orientation {
        req.orientation_cache = Some(OrientationCacheConfig::new());
    }
    req
}

fn estimate_tokens_for_payload(
    hits: &[ScoredMemory],
    rendered: Option<&mnemo_core::query::orientation_cache::RenderedContextMap>,
) -> u32 {
    let hits_tokens: u32 = hits
        .iter()
        .map(|h| (h.content.len().div_ceil(4)).max(1) as u32)
        .sum();
    let map_tokens = rendered.map(|r| r.token_estimate).unwrap_or(0);
    hits_tokens + map_tokens
}

struct ArmResult {
    p50_tokens: f64,
    last_top1_hit: bool,
    #[allow(dead_code)]
    p50_latency_ms: f64,
    /// Orientation-arm only: rendered map size in tokens. 0 for the
    /// hybrid arm. The cache's constant-token guarantee is asserted
    /// against this column.
    p50_map_tokens: f64,
}

async fn run_arm(k: usize, trial: usize, with_orientation: bool) -> ArmResult {
    let (engine, _store) = build_engine();
    seed_shared_context(&engine, trial).await;
    let queries: Vec<String> = (0..k)
        .map(|i| format!("FrobnicatorService API_BASE repeated-context call {i} of trial {trial}"))
        .collect();
    let mut tokens = Vec::with_capacity(k);
    let mut latencies = Vec::with_capacity(k);
    let mut map_tokens = Vec::with_capacity(k);
    let mut last_top1_hit = false;
    for q in &queries {
        let started = Instant::now();
        let resp = engine
            .recall(build_recall(q, with_orientation))
            .await
            .unwrap();
        latencies.push(started.elapsed().as_secs_f64() * 1000.0);
        let payload_tokens =
            estimate_tokens_for_payload(&resp.memories, resp.orientation_cache.as_ref());
        tokens.push(payload_tokens as f64);
        map_tokens.push(
            resp.orientation_cache
                .as_ref()
                .map(|r| r.token_estimate as f64)
                .unwrap_or(0.0),
        );
        last_top1_hit = resp
            .memories
            .first()
            .map(|m| m.content.contains("FrobnicatorService"))
            .unwrap_or(false);
    }
    ArmResult {
        p50_tokens: percentile(&tokens, 0.50),
        last_top1_hit,
        p50_latency_ms: percentile(&latencies, 0.50),
        p50_map_tokens: percentile(&map_tokens, 0.50),
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    std::fs::create_dir_all(&cli.out_dir)?;

    let mut md = String::new();
    md.push_str(&format!(
        "# Orientation-cache vs hybrid-only — {date}\n\n\
         > PEEK-anchored bench ([arXiv:2605.19932](https://arxiv.org/abs/2605.19932)). \
         Measures the v0.4.8 orientation-cache mode against the \
         default hybrid path on a repeated-context scenario. \
         Reports (a) the bounded constant-token guarantee of the \
         rendered map, (b) the per-call payload delta the cache \
         adds, and (c) top-1 hit-rate parity.\n\n\
         ## Setup\n\n\
         - Trials per K: {trials}.\n\
         - K values: {kvals:?} (number of repeated-context recall calls per trial).\n\
         - Shared context: {CONTEXT_FACTS} facts referencing a fixed cast of entities + constants + a fenced schema.\n\
         - Token estimate: heuristic ~4 chars/token (not `tiktoken-rs`).\n\
         - Engine: in-memory DuckDB + USearch (dim=3, `NoopEmbedding` — vector lane degenerate by design) + Tantivy BM25 + `OrientationCacheStore` (default 512-token budget).\n\n\
         ## Results (median across trials)\n\n\
         | K | hybrid p50 hits-tokens | orientation p50 map-tokens | orientation p50 hits+map | map ≤ budget? | top-1 parity |\n\
         |---:|---:|---:|---:|---|---|\n",
        trials = cli.trials,
        kvals = K_VALUES,
    ));

    let mut all_within_budget = true;
    let mut all_top1_parity = true;
    for &k in &K_VALUES {
        let mut hybrid_tokens = Vec::new();
        let mut orient_tokens = Vec::new();
        let mut orient_map_tokens = Vec::new();
        let mut hybrid_hits = 0usize;
        let mut orient_hits = 0usize;
        for trial in 0..cli.trials {
            let h = run_arm(k, trial, false).await;
            let o = run_arm(k, trial, true).await;
            hybrid_tokens.push(h.p50_tokens);
            orient_tokens.push(o.p50_tokens);
            orient_map_tokens.push(o.p50_map_tokens);
            if h.last_top1_hit {
                hybrid_hits += 1;
            }
            if o.last_top1_hit {
                orient_hits += 1;
            }
        }
        let hy_p50 = percentile(&hybrid_tokens, 0.50);
        let or_p50 = percentile(&orient_tokens, 0.50);
        let map_p50 = percentile(&orient_map_tokens, 0.50);
        let map_within_budget = map_p50 <= 512.0;
        if !map_within_budget {
            all_within_budget = false;
        }
        if orient_hits < hybrid_hits {
            all_top1_parity = false;
        }
        md.push_str(&format!(
            "| {k} | {hy_p50:.0} | {map_p50:.0} | {or_p50:.0} | {} | {hybrid_hits}/{trials} vs {orient_hits}/{trials} |\n",
            if map_within_budget { "yes" } else { "no" },
            trials = cli.trials,
        ));
    }

    md.push_str(&format!(
        "\n## Assertions\n\n\
         - **Constant-token guarantee:** rendered map ≤ 512-token budget on every K — **{}**\n\
         - **Top-1 parity:** orientation arm never lower than hybrid arm — **{}**\n\n\
         These are the v0.4.8 honest claims for the orientation cache. \
         The cache is a *bounded augmentation* of the recall payload, \
         not a payload reducer in this measurement. The PEEK-style \
         win — agent uses the warm map to skip rehydrating hits in \
         subsequent contexts — is a workflow optimisation downstream \
         of the engine and is NOT measured here. See the honest \
         disclaimers below.\n\n\
         ## Honest-disclaimer block\n\n\
         - **Not a faithful PEEK reproduction.** Heuristic distiller, \
         no learned encoder.\n\
         - **`NoopEmbedding` makes the vector lane degenerate.** \
         BM25 + the orientation map carry the signal. For real \
         numbers, swap to a real embedder (gated behind #44).\n\
         - **Token estimate is `(len / 4)`-heuristic.** Calibrate \
         with `tiktoken-rs` for production sizing decisions.\n\
         - **This bench measures per-call payload only.** The \
         workflow-level PEEK win (agent reads the map and requests \
         fewer hits next call) is downstream of the engine and is \
         not modeled here.\n\
         - **`top-1 parity` is a regression guard.** The orientation \
         cache MUST NOT lower top-1 hit rate at the bench scale.\n",
        if all_within_budget { "yes" } else { "no" },
        if all_top1_parity { "yes" } else { "no" },
    ));

    let out_path = cli.out_dir.join(format!("orientation_{date}.md"));
    std::fs::write(&out_path, md)?;
    println!("wrote {}", out_path.display());
    if !all_within_budget {
        eprintln!(
            "warning: rendered orientation map exceeded the 512-token budget on at least one K"
        );
    }
    if !all_top1_parity {
        eprintln!("warning: orientation arm reduced top-1 hit rate vs hybrid on at least one K");
    }
    Ok(())
}
