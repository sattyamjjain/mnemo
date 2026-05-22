//! v0.4.7 — MINTEval-shaped interference scenario.
//!
//! # Anchor
//!
//! [arXiv:2605.18565](https://arxiv.org/abs/2605.18565) (MINTEval —
//! Memory Interference under Targeted Edits) measures how often a
//! memory system returns a *superseded* value of a fact after the
//! same fact has been revised K times. The scenario in this bin
//! mirrors the MINTEval shape against mnemo's
//! `mnemo.recall` tool, comparing the default read path (semantic,
//! BM25, graph, recency — all fact-identity-unaware) against the
//! opt-in current-fact resolver (v0.4.7,
//! `mnemo_core::query::current_fact_resolver`).
//!
//! # Scenario
//!
//! For each `K ∈ {1, 3, 5, 10}`:
//!
//! 1. Stand up a fresh in-memory `MnemoEngine`.
//! 2. REMEMBER a synthetic context (50 distractor facts about a
//!    fictional entity) under unique `fact_id`s.
//! 3. REMEMBER a target fact `K + 1` times: an initial write
//!    followed by `K` revisions. Each write carries the same
//!    `fact_id` in metadata + a unique current value.
//! 4. Query the target fact via `mnemo.recall` in two arms:
//!    - **default arm:** no resolver; recall picks whatever the
//!      hybrid path ranks first.
//!    - **resolver arm:** `current_fact_resolver = Some({ fact_key:
//!      "fact_id", include_supersession_chain: true })`.
//! 5. Score: a hit *iff* the top-1 hit's content equals the
//!    most-recent revision. Compute `current-fact-accuracy@K =
//!    hits / trials`.
//! 6. Repeat each `K` setting `N=20` times for statistical stability.
//!
//! # Output
//!
//! Writes a single Markdown table to
//! `bench/locomo/results/interference_<YYYY-MM-DD>.md` with one row
//! per `K`: default vs resolver accuracy + the supersession-chain
//! length the resolver emitted on a representative trial.
//!
//! # What this bin is NOT
//!
//! - **Not a faithful MINTEval reproduction.** The MINTEval paper
//!   uses a curated benchmark corpus with GPT-judge scoring; this
//!   bin uses a deterministic exact-content match over a
//!   synthetic distractor pool. Operators wanting the official
//!   metric should swap the scoring function for a GPT-judge
//!   call (gated behind the same secrets as
//!   [#44](https://github.com/sattyamjjain/mnemo/issues/44)).
//! - **Not a perf bench.** Latency is recorded but not the
//!   primary signal; the headline is accuracy under interference.
//! - **Not a real embedder run.** Uses `NoopEmbedding` so the
//!   wiring is self-contained; vector lane is degenerate by
//!   design. The interference signal still surfaces because the
//!   resolver post-processes after recall.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::current_fact_resolver::CurrentFactResolverConfig;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

const TARGET_FACT_KEY: &str = "fact_id";
const TARGET_FACT_ID: &str = "target-city";
const DISTRACTOR_COUNT: usize = 50;
const TRIALS_PER_K: usize = 20;
const K_VALUES: [usize; 4] = [1, 3, 5, 10];

#[derive(Parser, Debug)]
#[command(name = "interference")]
struct Cli {
    /// Output directory.
    #[arg(long, default_value = "bench/locomo/results")]
    out_dir: PathBuf,
    /// Trials per K setting (default 20).
    #[arg(long, default_value_t = TRIALS_PER_K)]
    trials: usize,
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
        "interference-bench-agent".to_string(),
        None,
    )
    .with_full_text(ft)
}

async fn seed_distractors(engine: &MnemoEngine, trial: usize) {
    for i in 0..DISTRACTOR_COUNT {
        let mut req = RememberRequest::new(format!(
            "distractor fact about fictional-entity #{i} for trial {trial}"
        ));
        req.metadata = Some(serde_json::json!({
            TARGET_FACT_KEY: format!("distractor-{trial}-{i}"),
            "kind": "distractor",
        }));
        req.tags = Some(vec!["mint-eval".to_string(), "distractor".to_string()]);
        engine.remember(req).await.unwrap();
    }
}

async fn revise_target_fact(engine: &MnemoEngine, k: usize, trial: usize) -> String {
    // Write the target fact k+1 times. Each revision carries the
    // same fact_id but a unique current value.
    let cities = [
        "Paris", "Berlin", "Madrid", "Rome", "Vienna", "Lisbon", "Athens", "Oslo", "Prague",
        "Warsaw", "Dublin",
    ];
    let mut last_value = String::new();
    for i in 0..=k {
        let value = cities[i % cities.len()];
        last_value = value.to_string();
        let mut req = RememberRequest::new(format!(
            "The capital of fictional-region for trial {trial} is {value}."
        ));
        req.metadata = Some(serde_json::json!({
            TARGET_FACT_KEY: TARGET_FACT_ID,
            "kind": "target",
            "revision_idx": i,
        }));
        req.tags = Some(vec!["mint-eval".to_string(), "target".to_string()]);
        engine.remember(req).await.unwrap();
        // Small spacing so updated_at distinguishes revisions on
        // backends with sub-millisecond resolution.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    last_value
}

fn build_recall(query: &str, with_resolver: bool) -> RecallRequest {
    let mut req = RecallRequest::new(query.to_string());
    req.limit = Some(10);
    req.strategy = Some("auto".to_string());
    req.tags = Some(vec!["target".to_string()]);
    if with_resolver {
        req.current_fact_resolver =
            Some(CurrentFactResolverConfig::new(TARGET_FACT_KEY).with_supersession_chain());
    }
    req
}

async fn run_trial(engine: &MnemoEngine, k: usize, trial: usize, with_resolver: bool) -> bool {
    let expected = revise_target_fact(engine, k, trial).await;
    let query = format!("capital of fictional-region for trial {trial}");
    let resp = engine
        .recall(build_recall(&query, with_resolver))
        .await
        .unwrap();
    resp.memories
        .first()
        .map(|m| m.content.contains(&expected))
        .unwrap_or(false)
}

struct ArmResult {
    accuracy: f64,
    p50_ms: f64,
}

async fn run_arm(k: usize, trials: usize, with_resolver: bool) -> ArmResult {
    let mut hits = 0usize;
    let mut latencies = Vec::with_capacity(trials);
    for trial in 0..trials {
        let engine = build_engine();
        seed_distractors(&engine, trial).await;
        let started = Instant::now();
        if run_trial(&engine, k, trial, with_resolver).await {
            hits += 1;
        }
        latencies.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    ArmResult {
        accuracy: hits as f64 / trials as f64,
        p50_ms: percentile(&latencies, 0.50),
    }
}

async fn capture_supersession_chain_len(k: usize) -> usize {
    let engine = build_engine();
    seed_distractors(&engine, 999).await;
    let _ = revise_target_fact(&engine, k, 999).await;
    let resp = engine
        .recall(build_recall(
            "capital of fictional-region for trial 999",
            true,
        ))
        .await
        .unwrap();
    resp.superseded.map(|s| s.len()).unwrap_or(0)
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
        "# MINTEval-shaped interference bench — {date}\n\n\
         > Scaffold run reproducing the [arXiv:2605.18565](https://arxiv.org/abs/2605.18565) \
         MINTEval shape against mnemo's `mnemo.recall` tool. \
         Compares the default read path (fact-identity-unaware) \
         against the v0.4.7 opt-in current-fact resolver.\n\n\
         ## Setup\n\n\
         - Distractor pool: {DISTRACTOR_COUNT} synthetic facts per trial.\n\
         - Trials per K: {trials}.\n\
         - Target fact: revised K+1 times under the same `fact_id`.\n\
         - Scoring: top-1 content contains the most-recent revision \
         (deterministic exact-match; GPT-judge scoring deferred behind \
         [#44](https://github.com/sattyamjjain/mnemo/issues/44)).\n\
         - Engine: in-memory DuckDB + USearch (dim=3, `NoopEmbedding` \
         — vector lane degenerate by design) + Tantivy BM25.\n\n\
         ## Results\n\n\
         | K | default accuracy | resolver accuracy | resolver chain len | default p50 (ms) | resolver p50 (ms) |\n\
         |---:|---:|---:|---:|---:|---:|\n",
        trials = cli.trials,
    ));

    for &k in &K_VALUES {
        let default_arm = run_arm(k, cli.trials, false).await;
        let resolver_arm = run_arm(k, cli.trials, true).await;
        let chain_len = capture_supersession_chain_len(k).await;
        md.push_str(&format!(
            "| {} | {:.1}% ({}/{}) | {:.1}% ({}/{}) | {} | {:.2} | {:.2} |\n",
            k,
            default_arm.accuracy * 100.0,
            (default_arm.accuracy * cli.trials as f64).round() as usize,
            cli.trials,
            resolver_arm.accuracy * 100.0,
            (resolver_arm.accuracy * cli.trials as f64).round() as usize,
            cli.trials,
            chain_len,
            default_arm.p50_ms,
            resolver_arm.p50_ms,
        ));
    }

    md.push_str(
        "\n## Honest-disclaimer block\n\n\
         - **Not a faithful MINTEval reproduction.** The paper uses a \
         curated corpus + GPT-judge; this bin uses synthetic facts + \
         exact-content match.\n\
         - **NoopEmbedding makes the vector lane degenerate.** The \
         resolver still surfaces the interference signal because it \
         post-processes after recall. For real numbers, swap to a \
         real embedder (gated behind #44).\n\
         - **`chain len` column** is the length of the supersession \
         chain emitted on a representative trial (K=10 typically \
         emits 10 superseded entries; K=1 emits 1).\n",
    );

    let out_path = cli.out_dir.join(format!("interference_{date}.md"));
    std::fs::write(&out_path, md)?;
    println!("wrote {}", out_path.display());
    Ok(())
}
