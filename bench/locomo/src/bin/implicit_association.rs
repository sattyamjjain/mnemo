//! `implicit_association` — mnemo's own **indirect-query (implicit-association)**
//! retrieval probe, with an **orientation-cache** arm.
//!
//! # The question
//!
//! Retrieval surfaces a stored fact when a query *resembles* it. But agents ask
//! **indirect** questions whose answer depends on a stored fact they share no
//! wording with — you have to bridge world knowledge to connect them. Example:
//! stored "My anniversary falls on Bastille Day"; asked "which mid-July fireworks
//! holiday should I plan a party around?". Pure similarity retrieval has a blind
//! spot here.
//!
//! mnemo already ships the mechanism the InMind paper (arXiv:2607.24368) argues
//! recovers most of this gap: an opt-in, namespace-scoped, constant-token
//! **orientation cache** (`crates/mnemo-core/src/query/orientation_cache.rs`,
//! PEEK-anchored) that keeps distilled decisive knowledge (capitalized entities,
//! `UPPER_SNAKE = value` constants, fenced schemas) visible alongside recall.
//! This bin measures whether that arm surfaces a decisive record an indirect
//! query does not resemble.
//!
//! # Arms (per row, real embedder, fresh engine each trial)
//!
//! - `direct`               — the answer-blind control `direct_query`. Separates
//!   "never stored / unretrievable" from "stored but not surfaced by the indirect
//!   query". If `direct` misses, the record is simply not retrievable and the row
//!   is uninformative about the blind spot.
//! - `indirect`             — the `indirect_query`, no orientation cache. The blind spot.
//! - `indirect+orientation` — the `indirect_query` with the orientation cache,
//!   **warmed by the row's own prior (direct-query) recalls** — modeling a session
//!   where the fact was accessed before, then asked about indirectly. A hit is
//!   counted two ways, reported SEPARATELY (never merged): (A) `target_substring`
//!   in the top-k recalled memories; (B) `target_substring` in the returned bounded
//!   orientation map.
//!
//! # Metric
//!
//! Per arm: recall@1 / recall@5 with a Wilson-95 interval (`stats::wilson_95`).
//! Plus the `direct − indirect` gap (the blind spot) and the
//! `indirect+orientation − indirect` delta (what the cache recovers), each with
//! its CI. Refuses to emit a score under `NoopEmbedding` (`guard_real_embedder`).
//!
//! # What this is NOT
//!
//! **NOT a reproduction of InMind, and its numbers are NOT comparable to InMind's
//! 84.0% / 14.4%.** InMind is a 125-task, expert-verified benchmark (113 tasks
//! grounded in citable public sources) that scores an **LLM backbone's answers**
//! with an in-context arm. This bin is mnemo's own **retrieval-side** probe over a
//! 30-row hand-built corpus, **no LLM and no in-context arm** — it measures
//! whether the decisive record is *surfaced*, not whether a model *answers*. Small
//! n ⇒ wide Wilson CIs; treat sub-0.1 gaps as ties. arXiv:2607.24368 is the
//! **framing**, not a baseline.
//!
//! # Usage
//!
//! ```text
//! ollama pull nomic-embed-text
//! cargo run --release -p mnemo-locomo-bench --bin implicit_association
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Parser;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::orientation_cache::{OrientationCacheConfig, OrientationCacheStore};
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

use mnemo_locomo_bench::OllamaEmbedding;
use mnemo_locomo_bench::real_embedder::guard_real_embedder;
use mnemo_locomo_bench::stats::wilson_95;

#[derive(Debug, Deserialize, Clone)]
struct Row {
    id: String,
    #[allow(dead_code)]
    domain: String,
    stored_fact: String,
    indirect_query: String,
    direct_query: String,
    #[allow(dead_code)]
    bridge: String,
    target_substring: String,
    #[allow(dead_code)]
    source_url: String,
    distractors: Vec<String>,
}

#[derive(Parser, Debug)]
#[command(name = "implicit_association")]
struct Cli {
    /// In-process repeats per row (absorbs UUID-v7 + approximate-HNSW variance).
    #[arg(long, default_value_t = 5)]
    repeats: usize,
    /// Top-k retrieved per query (must be >= 5 for recall@5).
    #[arg(long, default_value_t = 10)]
    limit: usize,
    /// Orientation-cache token budget for the orientation arm.
    #[arg(long, default_value_t = 512)]
    token_budget: u32,
    #[arg(long, default_value = "http://localhost:11434/api/embeddings")]
    ollama_url: String,
    #[arg(long, default_value = "nomic-embed-text")]
    model: String,
    #[arg(long, default_value = "bench/locomo/results")]
    out_dir: PathBuf,
    #[arg(long)]
    dataset: Option<PathBuf>,
}

fn default_dataset_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("implicit_association.jsonl")
}

fn load_corpus(path: &Path) -> Vec<Row> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read corpus at {path:?}: {e}"));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Row>(l).expect("invalid corpus row"))
        .collect()
}

fn dataset_sha(path: &Path) -> String {
    let mut h = Sha256::new();
    h.update(std::fs::read(path).unwrap_or_default());
    hex::encode(h.finalize())
}

fn build_engine(embedding: Arc<dyn EmbeddingProvider>, dim: usize) -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(dim).unwrap());
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
    let store = OrientationCacheStore::new();
    MnemoEngine::new(
        storage,
        index,
        embedding,
        "implicit-assoc-bench".into(),
        None,
    )
    .with_full_text(ft)
    .with_orientation_cache_store(store)
}

/// Seed the decisive `stored_fact` plus its 6 distractors.
async fn seed_row(engine: &MnemoEngine, row: &Row) {
    let mut fact = RememberRequest::new(row.stored_fact.clone());
    fact.tags = Some(vec!["ia".into(), row.id.clone()]);
    engine.remember(fact).await.expect("seed stored_fact");
    for d in &row.distractors {
        let mut req = RememberRequest::new(d.clone());
        req.tags = Some(vec!["ia".into(), row.id.clone(), "distractor".into()]);
        engine.remember(req).await.expect("seed distractor");
    }
}

fn recall_req(
    query: &str,
    limit: usize,
    orientation: Option<OrientationCacheConfig>,
) -> RecallRequest {
    let mut req = RecallRequest::new(query.to_string());
    req.limit = Some(limit);
    // Default hybrid ("auto") — strategy left at its default.
    req.orientation_cache = orientation;
    req
}

/// Rank (1-based) of the first recalled memory whose content contains
/// `target`, or `None` if absent from the top-k.
fn target_rank(
    memories: &[mnemo_core::query::recall::ScoredMemory],
    target: &str,
) -> Option<usize> {
    memories
        .iter()
        .position(|m| m.content.contains(target))
        .map(|i| i + 1)
}

/// True if `target` appears anywhere in the rendered orientation map
/// (entities / constants / schemas — key or value).
fn map_contains(
    rendered: Option<&mnemo_core::query::orientation_cache::RenderedContextMap>,
    target: &str,
) -> bool {
    let Some(m) = rendered else { return false };
    m.entities
        .iter()
        .chain(m.constants.iter())
        .chain(m.schemas.iter())
        .any(|e| e.key.contains(target) || e.value.contains(target))
}

/// Per-arm tallies over all rows × repeats.
#[derive(Default, Clone)]
struct ArmTally {
    n: usize,
    r1: usize,
    r5: usize,
}
impl ArmTally {
    fn record(&mut self, rank: Option<usize>) {
        self.n += 1;
        if let Some(rk) = rank {
            if rk <= 1 {
                self.r1 += 1;
            }
            if rk <= 5 {
                self.r5 += 1;
            }
        }
    }
    fn recall1(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.r1 as f64 / self.n as f64
        }
    }
    fn recall5(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.r5 as f64 / self.n as f64
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    assert!(cli.limit >= 5, "--limit must be >= 5 for recall@5");
    let dataset_path = cli.dataset.clone().unwrap_or_else(default_dataset_path);
    let corpus = load_corpus(&dataset_path);
    assert_eq!(corpus.len(), 30, "expected the committed 30-row corpus");
    let sha = dataset_sha(&dataset_path);

    let embedder = OllamaEmbedding::connect(cli.ollama_url.clone(), cli.model.clone()).await?;
    let dim = embedder.dimensions();
    let embedding: Arc<dyn EmbeddingProvider> = Arc::new(embedder);
    // Refuse to emit a score under a degenerate (Noop) embedder.
    guard_real_embedder(embedding.as_ref(), "ollama")?;
    tracing::info!(model = %cli.model, dim, rows = corpus.len(), repeats = cli.repeats, "connected");

    let mut direct = ArmTally::default();
    let mut indirect = ArmTally::default();
    // indirect+orientation: (A) top-k memories, (B) orientation-map surfaced.
    let mut orient_mem = ArmTally::default();
    let mut orient_map_hits = 0usize; // sub-count B (binary "surfaced")
    let mut orient_combined = ArmTally::default(); // A OR B (recall@5 semantics)
    let mut orient_n = 0usize;

    for row in &corpus {
        for _ in 0..cli.repeats.max(1) {
            // ARM direct
            {
                let engine = build_engine(embedding.clone(), dim);
                seed_row(&engine, row).await;
                let resp = engine
                    .recall(recall_req(&row.direct_query, cli.limit, None))
                    .await?;
                direct.record(target_rank(&resp.memories, &row.target_substring));
            }
            // ARM indirect (no orientation)
            {
                let engine = build_engine(embedding.clone(), dim);
                seed_row(&engine, row).await;
                let resp = engine
                    .recall(recall_req(&row.indirect_query, cli.limit, None))
                    .await?;
                indirect.record(target_rank(&resp.memories, &row.target_substring));
            }
            // ARM indirect+orientation: warm with the row's own prior direct
            // recalls (distilling the decisive entity into the map), then score
            // the indirect query with the orientation cache on.
            {
                let engine = build_engine(embedding.clone(), dim);
                seed_row(&engine, row).await;
                for _ in 0..2 {
                    let _ = engine
                        .recall(recall_req(
                            &row.direct_query,
                            cli.limit,
                            Some(OrientationCacheConfig::new().with_token_budget(cli.token_budget)),
                        ))
                        .await?;
                }
                let resp = engine
                    .recall(recall_req(
                        &row.indirect_query,
                        cli.limit,
                        Some(OrientationCacheConfig::new().with_token_budget(cli.token_budget)),
                    ))
                    .await?;
                let mem_rank = target_rank(&resp.memories, &row.target_substring);
                let in_map = map_contains(resp.orientation_cache.as_ref(), &row.target_substring);
                orient_mem.record(mem_rank); // sub-count A
                if in_map {
                    orient_map_hits += 1; // sub-count B
                }
                orient_n += 1;
                // Combined recall@5 semantics: surfaced in top-5 memories OR in the map.
                let combined = match mem_rank {
                    Some(r) if r <= 5 => Some(1),
                    _ if in_map => Some(1),
                    _ => None,
                };
                orient_combined.record(combined);
            }
        }
    }

    // ---- report ----
    let map_rate = if orient_n == 0 {
        0.0
    } else {
        orient_map_hits as f64 / orient_n as f64
    };
    let gap = direct.recall5() - indirect.recall5(); // the blind spot (recall@5)
    let recovery = orient_combined.recall5() - indirect.recall5();

    let ci = |num: usize, n: usize| wilson_95(num, n);
    let (d5l, d5h) = ci(direct.r5, direct.n);
    let (i5l, i5h) = ci(indirect.r5, indirect.n);
    let (om5l, om5h) = ci(orient_mem.r5, orient_mem.n); // sub-count A CI
    let (o5l, o5h) = ci(orient_combined.r5, orient_combined.n); // combined A||B CI
    let (m_l, m_h) = ci(orient_map_hits, orient_n); // sub-count B CI

    println!(
        "\n=== implicit_association ({} {}-dim) — {} rows × {} repeats ===",
        cli.model,
        dim,
        corpus.len(),
        cli.repeats
    );
    println!(
        "{:<26} {:>9} {:>9} {:>22}",
        "arm", "recall@1", "recall@5", "recall@5 Wilson-95"
    );
    println!(
        "{:<26} {:>9.3} {:>9.3} {:>10.3},{:.3}",
        "direct (control)",
        direct.recall1(),
        direct.recall5(),
        d5l,
        d5h
    );
    println!(
        "{:<26} {:>9.3} {:>9.3} {:>10.3},{:.3}",
        "indirect (blind spot)",
        indirect.recall1(),
        indirect.recall5(),
        i5l,
        i5h
    );
    println!(
        "{:<26} {:>9.3} {:>9.3} {:>10.3},{:.3}",
        "indirect+orient (mem, A)",
        orient_mem.recall1(),
        orient_mem.recall5(),
        om5l,
        om5h
    );
    println!(
        "{:<26} {:>9} {:>9.3} {:>10.3},{:.3}",
        "  \\_ orient-map surfaced (B)", "-", map_rate, m_l, m_h
    );
    println!(
        "{:<26} {:>9} {:>9.3} {:>10.3},{:.3}",
        "  \\_ combined A||B (@5)",
        "-",
        orient_combined.recall5(),
        o5l,
        o5h
    );
    println!(
        "\ndirect - indirect gap (recall@5): {gap:+.3}   (the implicit-association blind spot)"
    );
    println!(
        "indirect+orientation - indirect (recall@5): {recovery:+.3}   (what the orientation cache recovers)"
    );

    // ---- write json + md ----
    std::fs::create_dir_all(&cli.out_dir)?;
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let json = serde_json::json!({
        "bench": "implicit_association",
        "framing": "InMind arXiv:2607.24368 (framing, NOT a baseline)",
        "embedder": { "backend": "ollama", "model": cli.model, "dim": dim },
        "corpus": { "path": "bench/locomo/data/implicit_association.jsonl", "rows": corpus.len(), "sha256": sha },
        "protocol": { "repeats": cli.repeats, "limit": cli.limit, "token_budget": cli.token_budget,
                      "orientation_warm": "2 prior recalls of the row's direct_query" },
        "arms": {
            "direct":   { "recall@1": direct.recall1(),   "recall@5": direct.recall5(),   "recall@5_wilson95": [d5l, d5h], "n": direct.n },
            "indirect": { "recall@1": indirect.recall1(), "recall@5": indirect.recall5(), "recall@5_wilson95": [i5l, i5h], "n": indirect.n },
            "indirect_orientation": {
                "top_k_memories_A": { "recall@1": orient_mem.recall1(), "recall@5": orient_mem.recall5() },
                "orientation_map_surfaced_B": { "rate": map_rate, "wilson95": [m_l, m_h] },
                "combined_A_or_B": { "recall@5": orient_combined.recall5(), "recall@5_wilson95": [o5l, o5h] },
                "n": orient_n
            }
        },
        "deltas": {
            "direct_minus_indirect_recall@5": gap,
            "indirect_orientation_minus_indirect_recall@5": recovery
        }
    });
    let json_path = cli
        .out_dir
        .join(format!("implicit_association_{date}.json"));
    std::fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;

    let md = render_md(
        &date,
        &cli.model,
        dim,
        &corpus,
        &sha,
        cli.repeats,
        cli.limit,
        &direct,
        &indirect,
        &orient_mem,
        map_rate,
        &orient_combined,
        orient_n,
        (d5l, d5h),
        (i5l, i5h),
        (om5l, om5h),
        (o5l, o5h),
        (m_l, m_h),
        gap,
        recovery,
    );
    let md_path = cli.out_dir.join(format!("implicit_association_{date}.md"));
    std::fs::write(&md_path, &md)?;
    println!(
        "\nwrote {}\nwrote {}",
        md_path.display(),
        json_path.display()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_md(
    date: &str,
    model: &str,
    dim: usize,
    corpus: &[Row],
    sha: &str,
    repeats: usize,
    limit: usize,
    direct: &ArmTally,
    indirect: &ArmTally,
    orient_mem: &ArmTally,
    map_rate: f64,
    orient_combined: &ArmTally,
    orient_n: usize,
    d5: (f64, f64),
    i5: (f64, f64),
    omci: (f64, f64),
    o5: (f64, f64),
    mci: (f64, f64),
    gap: f64,
    recovery: f64,
) -> String {
    let domains = {
        let mut d: Vec<&str> = corpus.iter().map(|r| r.domain.as_str()).collect();
        d.sort();
        d.dedup();
        d.len()
    };
    format!(
        "# implicit_association — {date}\n\n\
> mnemo's own **indirect-query (implicit-association)** retrieval probe, with an \
**orientation-cache** arm. Framing: InMind ([arXiv:2607.24368](https://arxiv.org/abs/2607.24368)) \
— **framing only, NOT a baseline** (see \"What this is NOT\").\n\n\
## Setup\n\n\
- Embedder: Ollama `{model}` ({dim}-dim), cosine HNSW; refuses to score under NoopEmbedding.\n\
- Engine: in-memory DuckDB + USearch HNSW + Tantivy BM25 + `OrientationCacheStore`, RRF (`auto`) recall.\n\
- Corpus: `bench/locomo/data/implicit_association.jsonl` ({rows} rows, {domains} domains, 6 distractors/row).\n\
- Corpus SHA-256: `{sha}`\n\
- Protocol: top-k={limit}, {repeats} in-process repeats/row; orientation arm warmed by 2 prior recalls of the row's `direct_query`.\n\n\
## Results\n\n\
| arm | recall@1 | recall@5 | recall@5 Wilson-95 |\n\
|---|---:|---:|---|\n\
| `direct` (answer-blind control) | {d1:.3} | {d5:.3} | [{d5l:.3}, {d5h:.3}] |\n\
| `indirect` (blind spot) | {i1:.3} | {i5:.3} | [{i5l:.3}, {i5h:.3}] |\n\
| `indirect+orientation` — top-k memories (A) | {om1:.3} | {om5:.3} | [{om5l:.3}, {om5h:.3}] |\n\
| &nbsp;&nbsp;└ orientation-map surfaced (B) | — | {mr:.3} | [{ml:.3}, {mh:.3}] |\n\
| &nbsp;&nbsp;└ combined A‖B (@5) | — | {oc5:.3} | [{o5l:.3}, {o5h:.3}] |\n\n\
Sub-counts A (top-k memories) and B (orientation map) are reported **separately, not merged**; \
the combined row is their OR at k=5. n(direct)={dn}, n(indirect)={in_}, n(orientation)={on}.\n\n\
- **Implicit-association blind spot** = `direct − indirect` recall@5 = **{gap:+.3}**.\n\
- **Orientation-cache recovery** = `indirect+orientation(A‖B) − indirect` recall@5 = **{recovery:+.3}**.\n\n\
## Reading this honestly\n\n\
`direct` measures whether the decisive record is retrievable at all; `indirect` measures whether \
a similarity query that shares no wording with it surfaces it (the blind spot). The orientation arm \
tests whether a constant-token context map, warmed by the fact's own earlier access, keeps the decisive \
entity visible for a later indirect question. Sub-count B (the map) is a binary *surfaced* signal, not \
a ranked hit — that is why it is reported apart from the ranked top-k sub-count A. On a 30-row corpus \
the Wilson intervals are wide; treat sub-0.1 gaps as ties and the numbers as directional.\n\n\
## What this is NOT\n\n\
- **NOT a reproduction of InMind, and NOT comparable to InMind's 84.0% / 14.4%.** InMind is a \
125-task, expert-verified benchmark (113 tasks grounded in citable public sources) that scores an \
**LLM backbone's answers** with an in-context arm. This bin is mnemo's **retrieval-side** probe on a \
30-row hand-built corpus with **no LLM and no in-context arm**: it measures whether the decisive \
record is *surfaced*, not whether a model *answers correctly*. arXiv:2607.24368 is the framing, not a baseline.\n\
- **NOT a leaderboard claim.** Small n ⇒ wide CIs.\n\
- **Reproducible + local**: fixed corpus (SHA above), local Ollama, no API key, no network beyond localhost.\n\n\
## Reproducing\n\n\
```text\n\
ollama pull nomic-embed-text\n\
cargo run --release -p mnemo-locomo-bench --bin implicit_association\n\
```\n",
        d1 = direct.recall1(),
        d5 = direct.recall5(),
        d5l = d5.0,
        d5h = d5.1,
        i1 = indirect.recall1(),
        i5 = indirect.recall5(),
        i5l = i5.0,
        i5h = i5.1,
        om1 = orient_mem.recall1(),
        om5 = orient_mem.recall5(),
        om5l = omci.0,
        om5h = omci.1,
        mr = map_rate,
        ml = mci.0,
        mh = mci.1,
        oc5 = orient_combined.recall5(),
        o5l = o5.0,
        o5h = o5.1,
        dn = direct.n,
        in_ = indirect.n,
        on = orient_n,
        rows = corpus.len(),
    )
}
