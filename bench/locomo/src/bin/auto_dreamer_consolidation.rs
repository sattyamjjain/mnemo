//! v0.4.8 — Auto-Dreamer-style offline consolidation bench.
//!
//! # Anchor (Auto-Dreamer / Auto Dream)
//!
//! Anthropic's "Auto Dream" consolidation runs offline, away from the
//! agent's interactive loop, and produces a smaller *active bank* of
//! semantic summaries that should serve subsequent recall at least as
//! well as the raw episodic trace it replaced. The reflection and
//! consolidation pass mnemo already exposes
//! ([`mnemo_core::query::lifecycle::run_consolidation`] +
//! [`mnemo_core::query::lifecycle::run_decay_pass`], with the
//! Auto-Dream-compatible reflection module at
//! [`mnemo_core::query::reflection`]) is the engine-side equivalent.
//! This bin exercises that path end-to-end on a synthetic
//! multi-session trajectory and reports the two numbers Auto-Dreamer
//! headlines as its axis:
//!
//! - `active_bank_ratio` = `active_bank_post / active_bank_pre`,
//!   where "active" = `consolidation_state ∈ {Raw, Active}` AND not
//!   deleted. Auto-Dreamer expects `< 1.0`.
//! - `recall_post >= recall_pre` on a fixed held-out needle set
//!   (one needle per session). Auto-Dreamer expects equal or better
//!   recall despite the bank shrinking.
//!
//! # Scenario
//!
//! `S` sessions × `F` facts each. Each session has its own topic tag
//! so consolidation clusters cleanly, plus a shared bench tag and one
//! per-session needle (`NEEDLE-{trial}-{session}-{uuid}`). Older
//! sessions have backdated `created_at` and an explicit `decay_rate`
//! chosen so the offline decay pass marks them `Archived` or
//! `Forgotten` deterministically.
//!
//! 1. Build a fresh in-memory `MnemoEngine`.
//! 2. Seed `S` sessions; backdate `created_at` and set `decay_rate`
//!    per session so older sessions decay further.
//! 3. Score held-out recall PRE (one query per needle).
//! 4. Snapshot `active_bank_pre`.
//! 5. Run `run_decay_pass(archive_threshold, forget_threshold)`.
//! 6. Run `run_consolidation(min_cluster_size)`.
//! 7. Snapshot `active_bank_post`.
//! 8. Re-score held-out recall POST.
//! 9. Emit a Markdown report + JSON summary.
//!
//! # Output
//!
//! - Markdown: `bench/locomo/results/auto_dreamer_<YYYY-MM-DD>.md`.
//! - JSON summary:
//!   `bench/locomo/results/auto_dreamer_<YYYY-MM-DD>.json` with
//!   fields `{ active_bank_ratio, recall_pre, recall_post, ... }`
//!   so the headline number is citable in the README.
//!
//! # What this bin is NOT
//!
//! - **Not a faithful Auto-Dreamer reproduction.** Anthropic's
//!   description points at an LLM-driven reflection summarizer;
//!   mnemo's `run_consolidation` clusters by *tag overlap* and
//!   emits a structured `[Consolidated from N memories] …` bundle.
//!   The bench measures the *shape* of the active-bank vs recall
//!   tradeoff, not the absolute scores any LLM-based summarizer
//!   would report.
//! - **Not a criterion-crate bench.** "Criterion-style" here means
//!   the same structured-report pattern the other `bench/locomo`
//!   bins follow (Markdown table + per-trial rows + assertion
//!   block). The `criterion` crate target lives at
//!   `crates/mnemo-core/benches/longmemeval_bench.rs` and is
//!   intentionally separate.
//! - **Not a perf bench.** The offline-pass latency is recorded as
//!   `offline_pass_elapsed_ms` for orientation, but it is never the
//!   headline.
//! - **Not a real embedder run.** Uses `NoopEmbedding` (dim=3) so
//!   the wiring is self-contained; the vector lane is degenerate by
//!   design. The recall signal rides on BM25 + tag clustering, both
//!   of which preserve the needle string through the consolidation
//!   bundle. For real numbers, swap to a real embedder (gated
//!   behind [#44](https://github.com/sattyamjjain/mnemo/issues/44)).
//! - **Not a multi-agent / cross-thread bench.** Single agent,
//!   single private scope.
//! - **Backdated `created_at` + explicit `decay_rate` drive a
//!   deterministic decay outcome.** That is the bench's lever for
//!   producing reproducible Archived / Forgotten counts; production
//!   decay schedules are operator-tuned.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::memory::ConsolidationState;
use mnemo_core::query::lifecycle::{run_consolidation, run_decay_pass};
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::query::{MAX_BATCH_QUERY_LIMIT, MnemoEngine};
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::MemoryFilter;
use mnemo_core::storage::duckdb::DuckDbStorage;

const SESSIONS: usize = 8;
const FACTS_PER_SESSION: usize = 25;
const TRIALS: usize = 5;
const ARCHIVE_THRESHOLD: f32 = 0.40;
const FORGET_THRESHOLD: f32 = 0.10;
const MIN_CLUSTER_SIZE: usize = 3;
const BENCH_TAG: &str = "auto-dreamer-bench";
const AGENT: &str = "auto-dreamer-bench-agent";

#[derive(Parser, Debug)]
#[command(name = "auto_dreamer_consolidation")]
struct Cli {
    /// Output directory for the Markdown + JSON artifacts.
    #[arg(long, default_value = "bench/locomo/results")]
    out_dir: PathBuf,
    /// Number of independent trials (medians reported).
    #[arg(long, default_value_t = TRIALS)]
    trials: usize,
    /// Sessions per trial.
    #[arg(long, default_value_t = SESSIONS)]
    sessions: usize,
    /// Facts per session (one of these is the needle).
    #[arg(long, default_value_t = FACTS_PER_SESSION)]
    facts_per_session: usize,
    /// Effective-importance threshold below which a record is Archived.
    #[arg(long, default_value_t = ARCHIVE_THRESHOLD)]
    archive_threshold: f32,
    /// Effective-importance threshold below which a record is Forgotten.
    #[arg(long, default_value_t = FORGET_THRESHOLD)]
    forget_threshold: f32,
    /// Minimum cluster size for `run_consolidation`.
    #[arg(long, default_value_t = MIN_CLUSTER_SIZE)]
    min_cluster_size: usize,
}

fn build_engine() -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("duckdb open"));
    let index = Arc::new(UsearchIndex::new(3).expect("usearch new"));
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy open"));
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft)
}

/// Per-session schedule. Older sessions (smaller `idx`) get larger
/// `age_hours` and a faster `decay_rate` so the offline decay pass
/// drains them predictably.
struct Session {
    topic_tag: String,
    needle_payload: String,
    age_hours: i64,
    decay_rate: f32,
    importance: f32,
}

fn build_sessions(trial: usize, n: usize) -> Vec<Session> {
    (0..n)
        .map(|i| {
            // Recency in [0, 1]: 0 = oldest, 1 = newest.
            let recency = if n <= 1 {
                1.0
            } else {
                i as f32 / (n as f32 - 1.0)
            };
            Session {
                topic_tag: format!("topic-{trial}-{i}"),
                needle_payload: format!("NEEDLE-{trial}-{i}-{}", uuid::Uuid::now_v7()),
                // Oldest: ~700h (~29 days); newest: 0h.
                age_hours: (700.0 * (1.0 - recency)) as i64,
                // Older = faster decay so old sessions clearly fall
                // below forget_threshold after the backdate.
                decay_rate: 0.005 + (1.0 - recency) * 0.05,
                // Newer = higher base importance.
                importance: 0.45 + recency * 0.30,
            }
        })
        .collect()
}

async fn seed_session(
    engine: &MnemoEngine,
    trial: usize,
    idx: usize,
    session: &Session,
    facts: usize,
) {
    let now = chrono::Utc::now();
    let backdate = now - chrono::Duration::hours(session.age_hours);
    let backdate_str = backdate.to_rfc3339();

    // Needle fact (one per session, deterministic-payload).
    let needle_content = format!(
        "{} | session #{idx} of trial {trial}",
        session.needle_payload
    );
    let mut req = RememberRequest::new(needle_content);
    req.tags = Some(vec![
        BENCH_TAG.to_string(),
        session.topic_tag.clone(),
        format!("trial-{trial}"),
    ]);
    req.importance = Some(session.importance);
    let resp = engine.remember(req).await.expect("remember needle");
    apply_backdate_and_decay(engine, resp.id, &backdate_str, session.decay_rate).await;

    // Supporting facts share the session topic tag so consolidation
    // clusters cleanly.
    for f in 1..facts {
        let content = format!(
            "session #{idx} fact #{f} of trial {trial}: topic {} payload-{f}",
            session.topic_tag
        );
        let mut req = RememberRequest::new(content);
        req.tags = Some(vec![
            BENCH_TAG.to_string(),
            session.topic_tag.clone(),
            format!("trial-{trial}"),
        ]);
        req.importance = Some(session.importance);
        let resp = engine.remember(req).await.expect("remember fact");
        apply_backdate_and_decay(engine, resp.id, &backdate_str, session.decay_rate).await;
    }
}

async fn apply_backdate_and_decay(
    engine: &MnemoEngine,
    id: uuid::Uuid,
    backdate_rfc3339: &str,
    decay_rate: f32,
) {
    let mut record = engine
        .storage
        .get_memory(id)
        .await
        .expect("get_memory")
        .expect("inserted record should exist");
    record.created_at = backdate_rfc3339.to_string();
    record.decay_rate = Some(decay_rate);
    engine
        .storage
        .update_memory(&record)
        .await
        .expect("update_memory");
}

async fn count_active_bank(engine: &MnemoEngine) -> usize {
    let filter = MemoryFilter {
        agent_id: Some(AGENT.to_string()),
        include_deleted: false,
        ..Default::default()
    };
    let records = engine
        .storage
        .list_memories(&filter, MAX_BATCH_QUERY_LIMIT, 0)
        .await
        .expect("list_memories");
    records
        .iter()
        .filter(|r| {
            matches!(
                r.consolidation_state,
                ConsolidationState::Raw | ConsolidationState::Active
            )
        })
        .count()
}

async fn score_recall(engine: &MnemoEngine, sessions: &[Session]) -> f64 {
    let mut hits = 0usize;
    for s in sessions {
        let mut req = RecallRequest::new(s.needle_payload.clone());
        req.limit = Some(10);
        req.strategy = Some("auto".to_string());
        req.tags = Some(vec![BENCH_TAG.to_string()]);
        let resp = engine.recall(req).await.expect("recall");
        if resp
            .memories
            .iter()
            .any(|m| m.content.contains(&s.needle_payload))
        {
            hits += 1;
        }
    }
    if sessions.is_empty() {
        0.0
    } else {
        hits as f64 / sessions.len() as f64
    }
}

struct TrialResult {
    active_bank_pre: usize,
    active_bank_post: usize,
    recall_pre: f64,
    recall_post: f64,
    decay_archived: usize,
    decay_forgotten: usize,
    consolidation_clusters: usize,
    consolidation_new: usize,
    consolidation_orig_consolidated: usize,
    elapsed_ms: f64,
}

async fn run_trial(cli: &Cli, trial: usize) -> TrialResult {
    let engine = build_engine();
    let sessions = build_sessions(trial, cli.sessions);
    for (i, s) in sessions.iter().enumerate() {
        seed_session(&engine, trial, i, s, cli.facts_per_session).await;
    }

    let active_bank_pre = count_active_bank(&engine).await;
    let recall_pre = score_recall(&engine, &sessions).await;

    let started = Instant::now();
    let decay = run_decay_pass(&engine, AGENT, cli.archive_threshold, cli.forget_threshold)
        .await
        .expect("run_decay_pass");
    let cons = run_consolidation(&engine, AGENT, cli.min_cluster_size)
        .await
        .expect("run_consolidation");
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

    let active_bank_post = count_active_bank(&engine).await;
    let recall_post = score_recall(&engine, &sessions).await;

    TrialResult {
        active_bank_pre,
        active_bank_post,
        recall_pre,
        recall_post,
        decay_archived: decay.archived,
        decay_forgotten: decay.forgotten,
        consolidation_clusters: cons.clusters_found,
        consolidation_new: cons.new_memories_created,
        consolidation_orig_consolidated: cons.originals_consolidated,
        elapsed_ms,
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

fn ratio(pre: usize, post: usize) -> f64 {
    if pre == 0 {
        0.0
    } else {
        post as f64 / pre as f64
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    std::fs::create_dir_all(&cli.out_dir)?;

    let mut trials: Vec<TrialResult> = Vec::with_capacity(cli.trials);
    for t in 0..cli.trials {
        trials.push(run_trial(&cli, t).await);
    }

    let pre_sizes: Vec<f64> = trials.iter().map(|t| t.active_bank_pre as f64).collect();
    let post_sizes: Vec<f64> = trials.iter().map(|t| t.active_bank_post as f64).collect();
    let ratios: Vec<f64> = trials
        .iter()
        .map(|t| ratio(t.active_bank_pre, t.active_bank_post))
        .collect();
    let pre_rec: Vec<f64> = trials.iter().map(|t| t.recall_pre).collect();
    let post_rec: Vec<f64> = trials.iter().map(|t| t.recall_post).collect();
    let elapsed: Vec<f64> = trials.iter().map(|t| t.elapsed_ms).collect();

    let pre_med = median(pre_sizes);
    let post_med = median(post_sizes);
    let ratio_med = median(ratios.clone());
    let recall_pre = median(pre_rec.clone());
    let recall_post = median(post_rec.clone());
    let elapsed_med = median(elapsed);

    let smaller_bank = ratio_med < 1.0;
    let equal_or_better_recall = recall_post + 1e-9 >= recall_pre;

    // ---- Markdown report ----
    let mut md = String::new();
    md.push_str(&format!(
        "# Auto-Dreamer offline consolidation — {date}\n\n\
         > Auto-Dreamer-style offline consolidation bench. Exercises \
         the engine's `run_decay_pass` + `run_consolidation` path on \
         a synthetic multi-session trajectory and reports the two \
         axes Auto-Dreamer headlines: **smaller active bank, \
         equal-or-better recall**.\n\n\
         ## Setup\n\n\
         - Sessions per trial: {sessions}.\n\
         - Facts per session: {facts}.\n\
         - Trials: {trials} (medians reported).\n\
         - Decay thresholds: archive={archive:.2}, forget={forget:.2}.\n\
         - Consolidation: tag-overlap clusters, `min_cluster_size = {min_cluster}`.\n\
         - Engine: in-memory DuckDB + USearch (dim=3, `NoopEmbedding` \
         — vector lane degenerate by design) + Tantivy BM25.\n\n\
         ## Results (median across trials)\n\n\
         | metric | value |\n\
         |---|---:|\n\
         | active_bank_pre | {pre_med:.1} |\n\
         | active_bank_post | {post_med:.1} |\n\
         | active_bank_ratio | {ratio_med:.3} |\n\
         | recall_pre | {recall_pre:.3} |\n\
         | recall_post | {recall_post:.3} |\n\
         | offline pass elapsed (ms) | {elapsed_med:.1} |\n\n\
         ## Per-trial detail\n\n\
         | trial | active_pre | active_post | ratio | recall_pre | recall_post | decay (arch/forg) | cons (clusters/new/orig) | elapsed (ms) |\n\
         |---:|---:|---:|---:|---:|---:|---|---|---:|\n",
        sessions = cli.sessions,
        facts = cli.facts_per_session,
        trials = cli.trials,
        archive = cli.archive_threshold,
        forget = cli.forget_threshold,
        min_cluster = cli.min_cluster_size,
    ));
    for (i, t) in trials.iter().enumerate() {
        let r = ratio(t.active_bank_pre, t.active_bank_post);
        md.push_str(&format!(
            "| {i} | {pre} | {post} | {r:.3} | {rp:.3} | {rpost:.3} | {arch}/{forg} | {cl}/{new}/{oc} | {el:.1} |\n",
            pre = t.active_bank_pre,
            post = t.active_bank_post,
            rp = t.recall_pre,
            rpost = t.recall_post,
            arch = t.decay_archived,
            forg = t.decay_forgotten,
            cl = t.consolidation_clusters,
            new = t.consolidation_new,
            oc = t.consolidation_orig_consolidated,
            el = t.elapsed_ms,
        ));
    }

    md.push_str(&format!(
        "\n## Auto-Dreamer assertions\n\n\
         - **Smaller active bank (`ratio < 1.0`):** **{}**.\n\
         - **Equal-or-better recall (`recall_post ≥ recall_pre`):** **{}**.\n\n\
         ## Honest-disclaimer block\n\n\
         - **Not a faithful Auto-Dreamer reproduction.** Anthropic's \
         description points at an LLM-driven reflection summarizer; \
         mnemo's `run_consolidation` clusters by tag overlap and \
         emits a structured `[Consolidated from N memories] …` \
         bundle. The bench measures the *shape* of the active-bank \
         vs recall tradeoff, not the absolute scores any LLM-based \
         summarizer would report.\n\
         - **\"Criterion-style\"** here means the structured-report \
         pattern the other `bench/locomo` bins use, not the \
         `criterion` crate. The `criterion` target lives at \
         `crates/mnemo-core/benches/longmemeval_bench.rs` and is \
         intentionally separate.\n\
         - **`NoopEmbedding` makes the vector lane degenerate.** \
         Recall signal rides on BM25 + tag clustering; the needle \
         string survives the consolidation bundle so BM25 still \
         finds it. For real numbers, swap to a real embedder (gated \
         behind [#44](https://github.com/sattyamjjain/mnemo/issues/44)).\n\
         - **Backdated `created_at` + explicit `decay_rate` drive a \
         deterministic decay outcome.** This is the bench's lever \
         for producing reproducible Archived / Forgotten counts; \
         production decay schedules are operator-tuned.\n\
         - **Single-agent, single-scope.** Multi-agent share / \
         delegation paths are out of scope for this bin.\n",
        if smaller_bank { "yes" } else { "no" },
        if equal_or_better_recall { "yes" } else { "no" },
    ));

    let md_path = cli.out_dir.join(format!("auto_dreamer_{date}.md"));
    std::fs::write(&md_path, md)?;

    // ---- JSON summary ----
    let summary = serde_json::json!({
        "scenario": "auto_dreamer_consolidation",
        "anchor": "Auto-Dreamer (offline consolidation: smaller active bank, equal-or-better recall)",
        "date": date,
        "config": {
            "sessions": cli.sessions,
            "facts_per_session": cli.facts_per_session,
            "trials": cli.trials,
            "archive_threshold": cli.archive_threshold,
            "forget_threshold": cli.forget_threshold,
            "min_cluster_size": cli.min_cluster_size,
        },
        "summary_median": {
            "active_bank_pre": pre_med,
            "active_bank_post": post_med,
            "active_bank_ratio": ratio_med,
            "recall_pre": recall_pre,
            "recall_post": recall_post,
            "offline_pass_elapsed_ms": elapsed_med,
        },
        "auto_dreamer_assertions": {
            "smaller_active_bank": smaller_bank,
            "equal_or_better_recall": equal_or_better_recall,
        },
    });
    let json_path = cli.out_dir.join(format!("auto_dreamer_{date}.json"));
    std::fs::write(&json_path, serde_json::to_string_pretty(&summary)?)?;

    println!("wrote {}", md_path.display());
    println!("wrote {}", json_path.display());

    if !smaller_bank {
        eprintln!("warning: active_bank_ratio >= 1.0 — Auto-Dreamer 'smaller bank' axis violated");
    }
    if !equal_or_better_recall {
        eprintln!(
            "warning: recall_post < recall_pre — Auto-Dreamer 'equal-or-better recall' axis violated"
        );
    }
    Ok(())
}
