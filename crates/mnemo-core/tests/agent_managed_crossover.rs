//! AutoMEM crossover eval (arXiv:2606.04315): agent-managed flat store
//! vs the fixed ingestion+retrieval pipeline.
//!
//! AutoMEM's framing is a **crossover**, not a winner: a fixed retrieval
//! pipeline wins **single-shot** queries (it ingested everything, so the
//! fact is in the index), while an agent that **controls its own writes**
//! over a simple flat store wins **long-horizon** workloads, because it
//! revises stale facts in place instead of letting every version pile up
//! and pollute retrieval.
//!
//! This eval reproduces both directions on one multi-session fixture:
//!
//! - **Tracked facts** are each revised across 3 sessions (current value
//!   = the 3rd).
//!   - *Fixed pipeline* ingests all 3 versions (append-only) — a
//!     "current value of X" query retrieves all three, so 2/3 of the
//!     retrieved evidence is stale (low precision / F1).
//!   - *Agent-managed* revises in place (soft-forget old + write new),
//!     so only the current value survives (precision 1.0).
//! - **Incidental details** the agent judged not worth keeping.
//!   - *Fixed pipeline* ingested them → single-shot recall 1.0.
//!   - *Agent-managed* never wrote them → recall 0.0.
//!
//! The test asserts the crossover in **both** directions, so the knob's
//! value (and its cost) is measurable.

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy};
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;

const AGENT: &str = "automem-eval-agent";
/// Mirrors `mnemo_mcp::tools::agent_managed::AGENT_MANAGED_TAG`. Kept as a
/// literal here so `mnemo-core` tests carry no dependency on the MCP crate.
const AGENT_MANAGED_TAG: &str = "agent-managed";
const N_TRACKED: usize = 12;
const N_VERSIONS: usize = 3;
const N_INCIDENTAL: usize = 12;

fn build_engine() -> MnemoEngine {
    let storage =
        Arc::new(mnemo_core::storage::duckdb::DuckDbStorage::open_in_memory().expect("duckdb"));
    let index = Arc::new(UsearchIndex::new(8).expect("usearch"));
    let embedding = Arc::new(NoopEmbedding::new(8));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy"));
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft)
}

/// Content of version `ver` (1-based) of tracked fact `i`. The unique
/// `TOPIC{i}` token makes BM25 select exactly this fact's versions; the
/// `VAL{i}v{ver}` token identifies which version it is.
fn tracked_content(i: usize, ver: usize) -> String {
    format!("TOPIC{i:02} status update: the recorded value is VAL{i:02}v{ver}")
}

fn incidental_content(j: usize) -> String {
    format!("DETAIL{j:02} note: incidental session info MISC{j:02}")
}

async fn write_plain(engine: &MnemoEngine, content: String, tags: Vec<String>) -> uuid::Uuid {
    let mut req = RememberRequest::new(content);
    if !tags.is_empty() {
        req.tags = Some(tags);
    }
    engine.remember(req).await.expect("remember").id
}

/// Seed the **fixed-pipeline** corpus: ingest every version of every
/// tracked fact plus every incidental (append-only ingestion heuristic).
async fn seed_pipeline(engine: &MnemoEngine) {
    for i in 0..N_TRACKED {
        for ver in 1..=N_VERSIONS {
            write_plain(engine, tracked_content(i, ver), vec![]).await;
        }
    }
    for j in 0..N_INCIDENTAL {
        write_plain(engine, incidental_content(j), vec![]).await;
    }
}

/// Seed the **agent-managed** flat store: the agent writes v1 then
/// revises in place to vN (soft-forget prior + write new, tagged), and
/// deliberately does NOT persist the incidentals (write-control).
async fn seed_agent_managed(engine: &MnemoEngine) {
    let tag = vec![AGENT_MANAGED_TAG.to_string()];
    for i in 0..N_TRACKED {
        let mut prev = write_plain(engine, tracked_content(i, 1), tag.clone()).await;
        for ver in 2..=N_VERSIONS {
            // mem_revise == soft-forget prior + write corrected entry.
            let mut fr = ForgetRequest::new(vec![prev]);
            fr.strategy = Some(ForgetStrategy::SoftDelete);
            engine.forget(fr).await.expect("forget");
            prev = write_plain(engine, tracked_content(i, ver), tag.clone()).await;
        }
    }
}

/// Recall top-`k` for `query`, optionally tag-scoped to the agent-managed
/// flat store (mirrors `mem_read`).
///
/// Retrieval is held to lexical BM25 for **both** arms on purpose: under
/// `NoopEmbedding` the vector lane is degenerate, so fixing the
/// retrieval mechanism isolates the measured variable to **write-control**
/// (which records exist in the store), which is exactly AutoMEM's claim.
/// With a real embedder the pipeline's single-shot lead would only widen.
async fn recall(engine: &MnemoEngine, query: &str, k: usize, agent_scoped: bool) -> Vec<String> {
    let mut req = RecallRequest::new(query.to_string());
    req.limit = Some(k);
    req.strategy = Some("lexical".to_string());
    if agent_scoped {
        req.tags = Some(vec![AGENT_MANAGED_TAG.to_string()]);
    }
    engine
        .recall(req)
        .await
        .expect("recall")
        .memories
        .into_iter()
        .map(|m| m.content)
        .collect()
}

#[derive(Default)]
struct Metrics {
    recall_sum: f64,
    precision_sum: f64,
    n: usize,
}

impl Metrics {
    fn add(&mut self, recall: f64, precision: f64) {
        self.recall_sum += recall;
        self.precision_sum += precision;
        self.n += 1;
    }
    fn recall(&self) -> f64 {
        self.recall_sum / self.n.max(1) as f64
    }
    fn precision(&self) -> f64 {
        self.precision_sum / self.n.max(1) as f64
    }
    fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }
}

/// Current-fact arm: for each tracked fact, query its unique topic and
/// score against the *current* value. Retrieving a stale version counts
/// against precision.
async fn current_fact_metrics(engine: &MnemoEngine, agent_scoped: bool) -> Metrics {
    let mut m = Metrics::default();
    for i in 0..N_TRACKED {
        let gold = format!("VAL{i:02}v{N_VERSIONS}");
        // Query the unique topic token only — BM25 returns just the docs
        // that contain it (all 3 versions for the pipeline; the single
        // revised survivor for the agent store), with no shared-word
        // padding to dilute the comparison.
        let hits = recall(engine, &format!("TOPIC{i:02}"), N_VERSIONS, agent_scoped).await;
        let retrieved = hits.len().max(1);
        let correct = hits.iter().filter(|c| c.contains(&gold)).count();
        let recall = if hits.iter().any(|c| c.contains(&gold)) {
            1.0
        } else {
            0.0
        };
        let precision = correct as f64 / retrieved as f64;
        m.add(recall, precision);
    }
    m
}

/// Incidental single-shot arm: recall@1 of details the agent did not
/// curate.
async fn incidental_recall(engine: &MnemoEngine, agent_scoped: bool) -> Metrics {
    let mut m = Metrics::default();
    for j in 0..N_INCIDENTAL {
        let gold = format!("MISC{j:02}");
        let hits = recall(engine, &format!("DETAIL{j:02}"), 1, agent_scoped).await;
        let recall = if hits.iter().any(|c| c.contains(&gold)) {
            1.0
        } else {
            0.0
        };
        m.add(recall, recall);
    }
    m
}

#[tokio::test]
async fn agent_managed_vs_pipeline_crossover() {
    // Mode (b): fixed ingestion + retrieval pipeline.
    let pipeline = build_engine();
    seed_pipeline(&pipeline).await;
    let pipe_current = current_fact_metrics(&pipeline, false).await;
    let pipe_incidental = incidental_recall(&pipeline, false).await;

    // Mode (a): agent-managed flat store (revise-in-place, selective).
    let agent = build_engine();
    seed_agent_managed(&agent).await;
    let agent_current = current_fact_metrics(&agent, true).await;
    let agent_incidental = incidental_recall(&agent, true).await;

    println!("\n=== AutoMEM crossover eval (arXiv:2606.04315) ===");
    println!(
        "fixture: {N_TRACKED} tracked facts × {N_VERSIONS} revisions, {N_INCIDENTAL} incidental details"
    );
    println!("| query family                | mode            | recall@k | precision | F1    |");
    println!("|-----------------------------|-----------------|---------:|----------:|------:|");
    println!(
        "| long-horizon current-fact   | fixed-pipeline  | {:>8.3} | {:>9.3} | {:>5.3} |",
        pipe_current.recall(),
        pipe_current.precision(),
        pipe_current.f1()
    );
    println!(
        "| long-horizon current-fact   | agent-managed   | {:>8.3} | {:>9.3} | {:>5.3} |",
        agent_current.recall(),
        agent_current.precision(),
        agent_current.f1()
    );
    println!(
        "| single-shot incidental      | fixed-pipeline  | {:>8.3} | {:>9.3} | {:>5.3} |",
        pipe_incidental.recall(),
        pipe_incidental.precision(),
        pipe_incidental.f1()
    );
    println!(
        "| single-shot incidental      | agent-managed   | {:>8.3} | {:>9.3} | {:>5.3} |",
        agent_incidental.recall(),
        agent_incidental.precision(),
        agent_incidental.f1()
    );

    // Direction 1 (long-horizon): agent-managed write-control wins F1 on
    // current-fact queries — it carries no stale versions.
    assert!(
        agent_current.f1() > pipe_current.f1(),
        "agent-managed current-fact F1 ({:.3}) must beat fixed-pipeline ({:.3})",
        agent_current.f1(),
        pipe_current.f1()
    );
    // Direction 2 (single-shot): the fixed pipeline wins incidental
    // recall — it ingested everything the agent chose to skip.
    assert!(
        pipe_incidental.recall() > agent_incidental.recall(),
        "fixed-pipeline incidental recall ({:.3}) must beat agent-managed ({:.3})",
        pipe_incidental.recall(),
        agent_incidental.recall()
    );
}
