//! v0.5.1 — active-reconstruction vs. default-RRF A/B bench.
//!
//! # Anchor
//!
//! [arXiv:2606.06036](https://arxiv.org/abs/2606.06036) (MRAgent) argues
//! that memory is better *reconstructed* from a cue plus its linked/causal
//! context than *retrieved* as isolated top-k snippets, and reports up to a
//! ~23% gain on multi-hop questions. This bin A/Bs mnemo's default hybrid
//! RRF recall against the v0.5.1 `reconstruct` strategy
//! ([`mnemo_core::retrieval::RetrievalMode::Reconstruct`]) so the claim can
//! be *checked on mnemo's own data*.
//!
//! # Scenario (deterministic, self-contained)
//!
//! `N` topic clusters. Each cluster has:
//!   * a **head** fact that matches the cluster's query lexically
//!     (`"Topic07: subsystem 07 is owned by OP07."`), and
//!   * a **detail** fact carrying the GOLD answer
//!     (`"OP07 stores the artifact in vault V07."`), which is linked to the
//!     head via a `related_to` graph edge but shares NO token with the
//!     query — so plain retrieval at the same `k` cannot surface it.
//!
//! For each cluster we issue `query = "TopicNN"` and measure
//! **gold-coverage@k**:
//!   * **auto arm:** gold token present in the top-`k` hits.
//!   * **reconstruct arm:** gold token present in the top-`k` hits OR in the
//!     reconstructed belief node (its `linked_context_ids` / summary).
//!
//! The delta is the fraction of multi-hop golds that reconstruction
//! surfaces and flat retrieval misses.
//!
//! # What this bin is NOT
//!
//! - **Not a faithful MRAgent reproduction.** MRAgent reconstructs with an
//!   LLM over a learned memory graph; mnemo's `reconstruct` walks explicit
//!   `related_to` edges and renders a deterministic, rule-based belief
//!   node. This measures the *mechanism* (does graph-linked context recover
//!   multi-hop answers flat top-k drops?), not MRAgent's absolute number.
//! - **The fixture is adversarially multi-hop by construction**, so the
//!   delta here is an upper-bound illustration, not a number to expect on
//!   arbitrary corpora. It is a harness to test reconstruction-vs-retrieval
//!   on *your* data — not a claim that retrieval is wrong.
//! - **Not a real embedder run.** `NoopEmbedding` (degenerate vector lane);
//!   Tantivy BM25 carries the lexical signal, the graph carries the rest.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

const AGENT: &str = "reconstruct-ab-agent";

#[derive(Parser, Debug)]
#[command(name = "reconstruct_ab")]
struct Cli {
    /// Output directory for the Markdown report.
    #[arg(long, default_value = "bench/locomo/results")]
    out_dir: PathBuf,
    /// Number of topic clusters (each = one head + one linked gold detail).
    #[arg(long, default_value_t = 24)]
    clusters: usize,
    /// Top-k cutoff for coverage scoring.
    #[arg(long, default_value_t = 5)]
    k: usize,
}

fn build_engine() -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(8).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(8));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft)
}

struct Cluster {
    query: String,
    gold_token: String,
    gold_id: uuid::Uuid,
}

async fn seed(engine: &MnemoEngine, n: usize) -> Vec<Cluster> {
    let mut clusters = Vec::with_capacity(n);
    for i in 0..n {
        let head = format!("Topic{i:02}: subsystem {i:02} is owned by operator OP{i:02}.");
        let head_id = engine
            .remember(RememberRequest::new(head))
            .await
            .unwrap()
            .id;

        // GOLD detail: carries the answer token VNN, linked to the head but
        // sharing no token with the query "TopicNN".
        let gold_token = format!("V{i:02}");
        let detail = format!("OP{i:02} stores the artifact in vault {gold_token}.");
        let mut detail_req = RememberRequest::new(detail);
        detail_req.related_to = Some(vec![head_id.to_string()]);
        let gold_id = engine.remember(detail_req).await.unwrap().id;

        clusters.push(Cluster {
            query: format!("Topic{i:02}"),
            gold_token,
            gold_id,
        });
    }
    clusters
}

/// Coverage under the default hybrid (`auto`) recall: gold token present in
/// the top-k hit contents.
async fn auto_covers(engine: &MnemoEngine, c: &Cluster, k: usize) -> bool {
    let mut req = RecallRequest::new(c.query.clone());
    req.limit = Some(k);
    let resp = engine.recall(req).await.unwrap();
    resp.memories
        .iter()
        .any(|m| m.content.contains(&c.gold_token))
}

/// Coverage under `reconstruct`: gold token in the top-k hits OR the gold
/// id pulled into the belief node's linked context (equivalently, present in
/// the reconstructed summary).
async fn reconstruct_covers(engine: &MnemoEngine, c: &Cluster, k: usize) -> bool {
    let mut req = RecallRequest::new(c.query.clone());
    req.limit = Some(k);
    req.strategy = Some("reconstruct".to_string());
    let resp = engine.recall(req).await.unwrap();
    let in_hits = resp
        .memories
        .iter()
        .any(|m| m.content.contains(&c.gold_token));
    let in_belief = resp
        .reconstruction
        .as_ref()
        .map(|b| b.linked_context_ids.contains(&c.gold_id) || b.summary.contains(&c.gold_token))
        .unwrap_or(false);
    in_hits || in_belief
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let engine = build_engine();
    let clusters = seed(&engine, cli.clusters).await;

    let mut auto_hits = 0usize;
    let mut recon_hits = 0usize;
    for c in &clusters {
        if auto_covers(&engine, c, cli.k).await {
            auto_hits += 1;
        }
        if reconstruct_covers(&engine, c, cli.k).await {
            recon_hits += 1;
        }
    }

    let n = clusters.len().max(1) as f64;
    let auto_cov = auto_hits as f64 / n;
    let recon_cov = recon_hits as f64 / n;
    let delta = recon_cov - auto_cov;

    // Sanity: the two arms must touch the same id-set; gold ids are unique.
    let unique_golds: HashSet<_> = clusters.iter().map(|c| c.gold_id).collect();
    assert_eq!(
        unique_golds.len(),
        clusters.len(),
        "gold ids must be unique"
    );

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let report = format!(
        "# Reconstruct vs. RRF A/B — gold-coverage@{k}\n\n\
         > {date} — active-reconstruction recall (MRAgent, arXiv:2606.06036) vs. default hybrid RRF.\n\
         > Fixture: {n} multi-hop clusters (head matches the query; the gold answer lives in a\n\
         > graph-linked detail that shares no token with the query). See the module doc for the\n\
         > honesty caveats — this is a mechanism check, not an absolute-number claim.\n\n\
         | strategy | gold-coverage@{k} |\n\
         |----------|------------------:|\n\
         | `auto` (RRF) | {auto_cov:.3} |\n\
         | `reconstruct` | {recon_cov:.3} |\n\
         | **delta** | **{delta:+.3}** |\n\n\
         Of {n_int} multi-hop golds, flat RRF surfaced {auto_hits} at k={k}; reconstruction\n\
         surfaced {recon_hits} by walking the memory graph for linked/causal context.\n",
        k = cli.k,
        date = date,
        n = n as usize,
        n_int = n as usize,
        auto_cov = auto_cov,
        recon_cov = recon_cov,
        delta = delta,
        auto_hits = auto_hits,
        recon_hits = recon_hits,
    );

    println!("{report}");

    if let Err(e) = std::fs::create_dir_all(&cli.out_dir) {
        eprintln!("warning: could not create {}: {e}", cli.out_dir.display());
        return;
    }
    let path = cli.out_dir.join(format!("reconstruct_ab_{date}.md"));
    match std::fs::write(&path, &report) {
        Ok(()) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("warning: could not write {}: {e}", path.display()),
    }
}
