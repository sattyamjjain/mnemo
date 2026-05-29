//! v0.4.10 — Fixed-schedule vs metric-driven consolidation bench.
//!
//! Compares the v0.4.x default [`ConsolidationPolicy::FixedSize`]
//! against the new feedback-driven [`ConsolidationPolicy::MaturityDriven`]
//! (FluxMem prior art, arXiv:2605.28773; structural cousin only).
//!
//! # Scenario
//!
//! A LoCoMo-style synthetic trace with a deliberate mix of "mature"
//! and "fresh" clusters:
//!
//! - **Mature clusters**: backdated `created_at`, non-zero
//!   `access_count`, full pairwise relations between members. These
//!   are the clusters the operator wants consolidated.
//! - **Fresh clusters**: created now, zero `access_count`, no
//!   relations. Premature consolidation here is "overreach"; the
//!   maturity gate should reject them.
//!
//! Each cluster holds `FACTS_PER_CLUSTER` records sharing one topic
//! tag plus a unique `NEEDLE-<trial>-<cluster>-<uuid>` payload used to
//! score recall before and after consolidation.
//!
//! # Arms
//!
//! - **Arm A — fixed**: `ConsolidationPolicy::FixedSize` with
//!   `min_cluster_size = 3`. Consolidates every cluster that clears
//!   the size gate, including fresh ones.
//! - **Arm B — maturity**: `ConsolidationPolicy::MaturityDriven`
//!   with `threshold = 0.50`, balanced weights. The maturity gate
//!   skips fresh clusters whose recency × hit-success × edge-degree
//!   product cannot clear the threshold. The threshold is tuned for
//!   the bench's degenerate `NoopEmbedding` redundancy axis; the
//!   production `MaturityPolicy::balanced()` default stays at `0.55`.
//!
//! # Output
//!
//! - Markdown: `bench/locomo/results/maturity_<YYYY-MM-DD>.md`
//! - JSON summary: `bench/locomo/results/maturity_<YYYY-MM-DD>.json`
//!
//! The Markdown table compares the two arms on
//! `active_bank_ratio`, `recall_pre`, `recall_post`,
//! `clusters_consolidated`, and `overreach` (fresh clusters
//! consolidated — Arm B should report `0`).
//!
//! # What this bin is NOT
//!
//! - **Not a FluxMem reproduction.** The arXiv:2605.28773 reference
//!   is prior-art-only — mnemo's policy is a structural cousin
//!   (same four-axis intuition), not the FluxMem control loop.
//! - **Not a criterion-crate bench.** "Criterion-style" here means
//!   the same structured-report pattern the other `bench/locomo`
//!   bins follow. The `criterion` target lives at
//!   `crates/mnemo-core/benches/longmemeval_bench.rs`.
//! - **Not a real-embedder run.** `NoopEmbedding` (dim=3) makes the
//!   vector lane and the maturity redundancy axis degenerate by
//!   design. The bench rides on tag clustering + BM25 + the
//!   recency / hit-success / edge-degree axes; recall_post tends to
//!   equal `1.0` in both arms because the needle string survives the
//!   bundle. The headline number is `active_bank_ratio` and
//!   `overreach`. Swap to a real embedder via [#44] for absolute
//!   numbers.
//! - **Not a multi-agent / cross-thread bench.** Single agent,
//!   single private scope.
//!
//! [#44]: https://github.com/sattyamjjain/mnemo/issues/44

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::memory::ConsolidationState;
use mnemo_core::model::relation::Relation;
use mnemo_core::query::lifecycle::{ConsolidationResult, run_consolidation};
use mnemo_core::query::maturity::{
    ConsolidationPolicy, MaturityPolicy, MaturitySaturation, MaturityWeights,
};
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::query::{MAX_BATCH_QUERY_LIMIT, MnemoEngine};
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::MemoryFilter;
use mnemo_core::storage::duckdb::DuckDbStorage;

const MATURE_CLUSTERS: usize = 3;
const FRESH_CLUSTERS: usize = 3;
const FACTS_PER_CLUSTER: usize = 4;
const TRIALS: usize = 5;
const MATURE_AGE_HOURS: i64 = 168; // 1 week.
const MATURE_ACCESS_COUNT: u64 = 12;
// Tuned for this synthetic trace: with `NoopEmbedding`, the redundancy
// component evaluates to 0 (zero-vector cosine), so the 0.55 production
// default `MaturityPolicy::balanced()` lands at the boundary even for
// the "mature" class. 0.50 cleanly separates mature (0.55) from fresh
// (~0.375) here. The MnemoEngine-side default is unchanged at 0.55.
const MATURITY_THRESHOLD: f32 = 0.50;
const MIN_CLUSTER_SIZE: usize = 3;
const AGENT: &str = "maturity-bench-agent";

#[derive(Parser, Debug)]
#[command(name = "maturity_consolidation")]
struct Cli {
    /// Output directory for the Markdown + JSON artifacts.
    #[arg(long, default_value = "bench/locomo/results")]
    out_dir: PathBuf,
    /// Number of independent trials (medians reported).
    #[arg(long, default_value_t = TRIALS)]
    trials: usize,
    /// Number of "mature" clusters per trial.
    #[arg(long, default_value_t = MATURE_CLUSTERS)]
    mature_clusters: usize,
    /// Number of "fresh" clusters per trial.
    #[arg(long, default_value_t = FRESH_CLUSTERS)]
    fresh_clusters: usize,
    /// Facts per cluster (one is the needle).
    #[arg(long, default_value_t = FACTS_PER_CLUSTER)]
    facts_per_cluster: usize,
    /// Maturity-arm threshold.
    #[arg(long, default_value_t = MATURITY_THRESHOLD)]
    maturity_threshold: f32,
    /// Min cluster size (applied to both arms).
    #[arg(long, default_value_t = MIN_CLUSTER_SIZE)]
    min_cluster_size: usize,
}

fn build_engine(policy: ConsolidationPolicy) -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("duckdb open"));
    let index = Arc::new(UsearchIndex::new(3).expect("usearch new"));
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy open"));
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None)
        .with_full_text(ft)
        .with_consolidation_policy(policy)
}

#[derive(Clone, Copy)]
enum ClusterFlavour {
    Mature,
    Fresh,
}

struct Cluster {
    topic_tag: String,
    needle_payload: String,
    flavour: ClusterFlavour,
}

fn build_clusters(trial: usize, mature: usize, fresh: usize) -> Vec<Cluster> {
    let mut out = Vec::with_capacity(mature + fresh);
    for i in 0..mature {
        out.push(Cluster {
            topic_tag: format!("mature-topic-{trial}-{i}"),
            needle_payload: format!("NEEDLE-MATURE-{trial}-{i}-{}", uuid::Uuid::now_v7()),
            flavour: ClusterFlavour::Mature,
        });
    }
    for i in 0..fresh {
        out.push(Cluster {
            topic_tag: format!("fresh-topic-{trial}-{i}"),
            needle_payload: format!("NEEDLE-FRESH-{trial}-{i}-{}", uuid::Uuid::now_v7()),
            flavour: ClusterFlavour::Fresh,
        });
    }
    out
}

async fn seed_cluster(
    engine: &MnemoEngine,
    trial: usize,
    idx: usize,
    cluster: &Cluster,
    facts: usize,
) {
    let backdate_str = match cluster.flavour {
        ClusterFlavour::Mature => {
            (chrono::Utc::now() - chrono::Duration::hours(MATURE_AGE_HOURS)).to_rfc3339()
        }
        ClusterFlavour::Fresh => chrono::Utc::now().to_rfc3339(),
    };
    let access = match cluster.flavour {
        ClusterFlavour::Mature => MATURE_ACCESS_COUNT,
        ClusterFlavour::Fresh => 0,
    };

    let mut member_ids = Vec::with_capacity(facts);

    // Needle fact. Tag is ONLY the per-cluster topic so the
    // tag-overlap clusterer keeps clusters disjoint; the engine is
    // bench-scoped (fresh per arm), so we do not need a shared
    // BENCH_TAG on the memories themselves — the recall path no
    // longer filters by it either.
    let needle_content = format!(
        "{} | cluster #{idx} of trial {trial}",
        cluster.needle_payload
    );
    let mut req = RememberRequest::new(needle_content);
    req.tags = Some(vec![cluster.topic_tag.clone()]);
    req.importance = Some(0.6);
    let resp = engine.remember(req).await.expect("remember needle");
    apply_synthetic_state(engine, resp.id, &backdate_str, access).await;
    member_ids.push(resp.id);

    // Supporting facts share the topic tag for clustering.
    for f in 1..facts {
        let content = format!(
            "cluster #{idx} fact #{f} of trial {trial}: topic {} payload-{f}",
            cluster.topic_tag
        );
        let mut req = RememberRequest::new(content);
        req.tags = Some(vec![cluster.topic_tag.clone()]);
        req.importance = Some(0.6);
        let resp = engine.remember(req).await.expect("remember fact");
        apply_synthetic_state(engine, resp.id, &backdate_str, access).await;
        member_ids.push(resp.id);
    }

    // Mature clusters get full pairwise relations to lift the
    // edge-degree component of the maturity score. Fresh clusters
    // stay isolated.
    if matches!(cluster.flavour, ClusterFlavour::Mature) {
        for i in 0..member_ids.len() {
            for j in 0..member_ids.len() {
                if i == j {
                    continue;
                }
                let relation = Relation {
                    id: uuid::Uuid::now_v7(),
                    source_id: member_ids[i],
                    target_id: member_ids[j],
                    relation_type: "cluster_co_occurrence".to_string(),
                    weight: 1.0,
                    metadata: serde_json::Value::Object(serde_json::Map::new()),
                    created_at: backdate_str.clone(),
                };
                engine
                    .storage
                    .insert_relation(&relation)
                    .await
                    .expect("insert_relation");
            }
        }
    }
}

async fn apply_synthetic_state(
    engine: &MnemoEngine,
    id: uuid::Uuid,
    created_at_rfc3339: &str,
    access_count: u64,
) {
    let mut record = engine
        .storage
        .get_memory(id)
        .await
        .expect("get_memory")
        .expect("inserted record should exist");
    record.created_at = created_at_rfc3339.to_string();
    record.last_accessed_at = Some(created_at_rfc3339.to_string());
    record.access_count = access_count;
    engine
        .storage
        .update_memory(&record)
        .await
        .expect("update_memory");
}

/// Re-apply the per-flavour synthetic state (`created_at`,
/// `last_accessed_at`, `access_count`) to every cluster member.
/// `recall(...)` mutates these fields via `touch_memory`, which would
/// equalise the mature / fresh distinction before the maturity gate
/// sees it. Running this between `score_recall_pre` and the
/// consolidation pass restores the intended pre-state without changing
/// what consolidation actually consumes.
async fn restore_synthetic_state(engine: &MnemoEngine, clusters: &[Cluster]) {
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
    for mut record in records {
        // Identify the cluster by topic tag — every member carries
        // exactly one cluster-specific tag.
        let Some(cluster) = clusters
            .iter()
            .find(|c| record.tags.iter().any(|t| t == &c.topic_tag))
        else {
            continue;
        };
        let backdate_str = match cluster.flavour {
            ClusterFlavour::Mature => {
                (chrono::Utc::now() - chrono::Duration::hours(MATURE_AGE_HOURS)).to_rfc3339()
            }
            ClusterFlavour::Fresh => chrono::Utc::now().to_rfc3339(),
        };
        let access = match cluster.flavour {
            ClusterFlavour::Mature => MATURE_ACCESS_COUNT,
            ClusterFlavour::Fresh => 0,
        };
        record.created_at = backdate_str.clone();
        record.last_accessed_at = Some(backdate_str);
        record.access_count = access;
        engine
            .storage
            .update_memory(&record)
            .await
            .expect("update_memory");
    }
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

async fn score_recall(engine: &MnemoEngine, clusters: &[Cluster]) -> f64 {
    let mut hits = 0usize;
    for c in clusters {
        let mut req = RecallRequest::new(c.needle_payload.clone());
        req.limit = Some(10);
        req.strategy = Some("auto".to_string());
        let resp = engine.recall(req).await.expect("recall");
        if resp
            .memories
            .iter()
            .any(|m| m.content.contains(&c.needle_payload))
        {
            hits += 1;
        }
    }
    if clusters.is_empty() {
        0.0
    } else {
        hits as f64 / clusters.len() as f64
    }
}

/// Count consolidated bundles created from the *fresh* clusters by
/// scanning bundle content for the fresh-needle payloads. Arm B
/// (maturity) should report 0 here.
async fn count_overreach(engine: &MnemoEngine, clusters: &[Cluster]) -> usize {
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
    let fresh_needles: Vec<&str> = clusters
        .iter()
        .filter(|c| matches!(c.flavour, ClusterFlavour::Fresh))
        .map(|c| c.needle_payload.as_str())
        .collect();
    records
        .iter()
        .filter(|r| {
            r.content.starts_with("[Consolidated from")
                && fresh_needles.iter().any(|n| r.content.contains(n))
        })
        .count()
}

struct ArmResult {
    active_bank_pre: usize,
    active_bank_post: usize,
    recall_pre: f64,
    recall_post: f64,
    consolidation: ConsolidationResult,
    overreach: usize,
    elapsed_ms: f64,
}

async fn run_arm(cli: &Cli, trial: usize, policy: ConsolidationPolicy) -> ArmResult {
    let engine = build_engine(policy);
    let clusters = build_clusters(trial, cli.mature_clusters, cli.fresh_clusters);
    for (i, c) in clusters.iter().enumerate() {
        seed_cluster(&engine, trial, i, c, cli.facts_per_cluster).await;
    }

    let active_bank_pre = count_active_bank(&engine).await;
    let recall_pre = score_recall(&engine, &clusters).await;
    // recall mutates access_count + last_accessed_at on every touched
    // record; restore the intended pre-consolidation state so the
    // maturity gate sees the mature/fresh distinction it was given.
    restore_synthetic_state(&engine, &clusters).await;

    let started = Instant::now();
    let consolidation = run_consolidation(&engine, AGENT, cli.min_cluster_size)
        .await
        .expect("run_consolidation");
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

    let active_bank_post = count_active_bank(&engine).await;
    let recall_post = score_recall(&engine, &clusters).await;
    let overreach = count_overreach(&engine, &clusters).await;

    ArmResult {
        active_bank_pre,
        active_bank_post,
        recall_pre,
        recall_post,
        consolidation,
        overreach,
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

struct ArmMedians {
    name: &'static str,
    active_bank_pre: f64,
    active_bank_post: f64,
    ratio: f64,
    recall_pre: f64,
    recall_post: f64,
    overreach: f64,
    clusters_consolidated: f64,
    elapsed_ms: f64,
}

fn medians(name: &'static str, arm: &[ArmResult]) -> ArmMedians {
    ArmMedians {
        name,
        active_bank_pre: median(arm.iter().map(|t| t.active_bank_pre as f64).collect()),
        active_bank_post: median(arm.iter().map(|t| t.active_bank_post as f64).collect()),
        ratio: median(
            arm.iter()
                .map(|t| ratio(t.active_bank_pre, t.active_bank_post))
                .collect(),
        ),
        recall_pre: median(arm.iter().map(|t| t.recall_pre).collect()),
        recall_post: median(arm.iter().map(|t| t.recall_post).collect()),
        overreach: median(arm.iter().map(|t| t.overreach as f64).collect()),
        clusters_consolidated: median(
            arm.iter()
                .map(|t| t.consolidation.new_memories_created as f64)
                .collect(),
        ),
        elapsed_ms: median(arm.iter().map(|t| t.elapsed_ms).collect()),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    std::fs::create_dir_all(&cli.out_dir)?;

    let mut arm_fixed: Vec<ArmResult> = Vec::with_capacity(cli.trials);
    let mut arm_maturity: Vec<ArmResult> = Vec::with_capacity(cli.trials);
    let maturity_policy = ConsolidationPolicy::MaturityDriven(MaturityPolicy {
        weights: MaturityWeights::balanced(),
        saturation: MaturitySaturation::default(),
        threshold: cli.maturity_threshold,
        min_cluster_size_floor: cli.min_cluster_size,
        trigger_on_forget: false,
        trigger_on_checkpoint: false,
    });
    for t in 0..cli.trials {
        arm_fixed.push(run_arm(&cli, t, ConsolidationPolicy::FixedSize).await);
        arm_maturity.push(run_arm(&cli, t, maturity_policy.clone()).await);
    }

    let med_a = medians("fixed", &arm_fixed);
    let med_b = medians("maturity", &arm_maturity);

    // The user-specified Pareto axes are "recall retained" and
    // "store-size reduction". A Pareto win for the metric-driven arm
    // requires equal-or-better recall AND equal-or-better store
    // reduction (i.e. equal-or-lower active_bank_ratio). Tolerance is
    // 1e-9 on both axes so the equal case still counts.
    let recall_not_worse = med_b.recall_post + 1e-9 >= med_a.recall_post;
    let reduction_not_worse = med_b.ratio <= med_a.ratio + 1e-9;
    let pareto_win = recall_not_worse && reduction_not_worse;
    // Separate, supplementary check: did the gate avoid over-reaching
    // into "fresh" clusters? This is the metric-driven arm's design
    // intent — qualitative win for selectivity, not the headline.
    let no_overreach = med_b.overreach <= med_a.overreach;

    // ---- Markdown report ----
    let mut md = String::new();
    md.push_str(&format!(
        "# Fixed vs metric-driven consolidation — {date}\n\n\
         > Compares `ConsolidationPolicy::FixedSize` (v0.4.x default) \
         against `ConsolidationPolicy::MaturityDriven` on a LoCoMo-style \
         synthetic trace mixing mature (backdated, hit, edge-rich) and \
         fresh (zero-access, no-edge) clusters. The maturity gate \
         should consolidate the mature clusters and skip the fresh \
         ones; the fixed gate consolidates everything.\n\n\
         ## Setup\n\n\
         - Mature clusters per trial: {mature}.\n\
         - Fresh clusters per trial: {fresh}.\n\
         - Facts per cluster: {facts}.\n\
         - Trials: {trials} (medians reported).\n\
         - Mature-cluster backdate: {age}h.\n\
         - Mature-cluster `access_count`: {access}.\n\
         - Maturity arm: threshold = {thresh:.2}, balanced weights, default saturations.\n\
         - Fixed arm: `min_cluster_size = {minc}`, unconditional.\n\
         - Engine: in-memory DuckDB + USearch (dim=3, `NoopEmbedding` \
         — vector + redundancy axes degenerate by design) + Tantivy BM25.\n\n\
         ## Results (median across trials)\n\n\
         | arm | active_pre | active_post | ratio | recall_pre | recall_post | clusters_consolidated | overreach | elapsed (ms) |\n\
         |---|---:|---:|---:|---:|---:|---:|---:|---:|\n\
         | {a_name} | {a_pre:.1} | {a_post:.1} | {a_ratio:.3} | {a_rp:.3} | {a_rpost:.3} | {a_cons:.1} | {a_over:.1} | {a_el:.1} |\n\
         | {b_name} | {b_pre:.1} | {b_post:.1} | {b_ratio:.3} | {b_rp:.3} | {b_rpost:.3} | {b_cons:.1} | {b_over:.1} | {b_el:.1} |\n\n\
         ## Per-trial detail (fixed arm)\n\n\
         | trial | active_pre | active_post | ratio | recall_post | overreach | elapsed (ms) |\n\
         |---:|---:|---:|---:|---:|---:|---:|\n",
        mature = cli.mature_clusters,
        fresh = cli.fresh_clusters,
        facts = cli.facts_per_cluster,
        trials = cli.trials,
        age = MATURE_AGE_HOURS,
        access = MATURE_ACCESS_COUNT,
        thresh = cli.maturity_threshold,
        minc = cli.min_cluster_size,
        a_name = med_a.name,
        a_pre = med_a.active_bank_pre,
        a_post = med_a.active_bank_post,
        a_ratio = med_a.ratio,
        a_rp = med_a.recall_pre,
        a_rpost = med_a.recall_post,
        a_cons = med_a.clusters_consolidated,
        a_over = med_a.overreach,
        a_el = med_a.elapsed_ms,
        b_name = med_b.name,
        b_pre = med_b.active_bank_pre,
        b_post = med_b.active_bank_post,
        b_ratio = med_b.ratio,
        b_rp = med_b.recall_pre,
        b_rpost = med_b.recall_post,
        b_cons = med_b.clusters_consolidated,
        b_over = med_b.overreach,
        b_el = med_b.elapsed_ms,
    ));
    for (i, t) in arm_fixed.iter().enumerate() {
        md.push_str(&format!(
            "| {i} | {pre} | {post} | {r:.3} | {rpost:.3} | {ov} | {el:.1} |\n",
            pre = t.active_bank_pre,
            post = t.active_bank_post,
            r = ratio(t.active_bank_pre, t.active_bank_post),
            rpost = t.recall_post,
            ov = t.overreach,
            el = t.elapsed_ms,
        ));
    }
    md.push_str("\n## Per-trial detail (maturity arm)\n\n| trial | active_pre | active_post | ratio | recall_post | overreach | elapsed (ms) |\n|---:|---:|---:|---:|---:|---:|---:|\n");
    for (i, t) in arm_maturity.iter().enumerate() {
        md.push_str(&format!(
            "| {i} | {pre} | {post} | {r:.3} | {rpost:.3} | {ov} | {el:.1} |\n",
            pre = t.active_bank_pre,
            post = t.active_bank_post,
            r = ratio(t.active_bank_pre, t.active_bank_post),
            rpost = t.recall_post,
            ov = t.overreach,
            el = t.elapsed_ms,
        ));
    }

    md.push_str(&format!(
        "\n## Verdict\n\n\
         Pareto axes follow the user-specified framing of *recall \
         retained* and *store-size reduction*. Selectivity is reported \
         alongside as a supplementary axis (the design intent of the \
         gate) but does not gate the Pareto verdict.\n\n\
         - **`recall_post (maturity) >= recall_post (fixed)`:** **{rnw}**.\n\
         - **`active_bank_ratio (maturity) <= active_bank_ratio (fixed)`:** **{rdnw}**.\n\
         - **Pareto win on (recall, store-reduction):** **{pw}**.\n\
         - Supplementary — **`overreach (maturity) <= overreach (fixed)`:** **{noo}**.\n\n\
         ## What this bench is NOT\n\n\
         - **Not a FluxMem reproduction.** [arXiv:2605.28773] is \
         prior-art-only; mnemo's policy is a structural cousin (same \
         four-axis intuition), not the FluxMem control loop.\n\
         - **`NoopEmbedding` makes the vector + redundancy axes \
         degenerate.** Recall signal rides on BM25 + tag clustering; \
         the needle string survives the consolidation bundle so BM25 \
         still finds it. `recall_post` consequently tends to `1.0` in \
         both arms. The headline numbers are `active_bank_ratio` and \
         `overreach`.\n\
         - **Synthetic only.** A real-trace replay is a follow-up; \
         this bench is a controlled comparison of the two gates on a \
         repeatable trace.\n\
         - **Backdated `created_at` + explicit `access_count` drive a \
         deterministic maturity outcome.** That is the bench's lever \
         for reproducible mature / fresh classes; production traces \
         arrive with whatever recency / hit-counts they arrive with.\n\
         - **`overreach` is measured by string-match on the fresh \
         needle inside `[Consolidated from N memories] …` bundles.** \
         Sufficient for this scenario, but not a general-purpose \
         detector.\n",
        rnw = if recall_not_worse { "yes" } else { "no" },
        rdnw = if reduction_not_worse { "yes" } else { "no" },
        noo = if no_overreach { "yes" } else { "no" },
        pw = if pareto_win { "yes" } else { "no" },
    ));

    let md_path = cli.out_dir.join(format!("maturity_{date}.md"));
    std::fs::write(&md_path, md)?;

    // ---- JSON summary ----
    let summary = serde_json::json!({
        "scenario": "maturity_consolidation",
        "anchor": "FluxMem (arXiv:2605.28773) — prior art only; mnemo policy is a structural cousin",
        "date": date,
        "config": {
            "mature_clusters": cli.mature_clusters,
            "fresh_clusters": cli.fresh_clusters,
            "facts_per_cluster": cli.facts_per_cluster,
            "trials": cli.trials,
            "min_cluster_size": cli.min_cluster_size,
            "maturity_threshold": cli.maturity_threshold,
            "mature_age_hours": MATURE_AGE_HOURS,
            "mature_access_count": MATURE_ACCESS_COUNT,
        },
        "summary_median": {
            "fixed": {
                "active_bank_pre": med_a.active_bank_pre,
                "active_bank_post": med_a.active_bank_post,
                "active_bank_ratio": med_a.ratio,
                "recall_pre": med_a.recall_pre,
                "recall_post": med_a.recall_post,
                "clusters_consolidated": med_a.clusters_consolidated,
                "overreach": med_a.overreach,
                "elapsed_ms": med_a.elapsed_ms,
            },
            "maturity": {
                "active_bank_pre": med_b.active_bank_pre,
                "active_bank_post": med_b.active_bank_post,
                "active_bank_ratio": med_b.ratio,
                "recall_pre": med_b.recall_pre,
                "recall_post": med_b.recall_post,
                "clusters_consolidated": med_b.clusters_consolidated,
                "overreach": med_b.overreach,
                "elapsed_ms": med_b.elapsed_ms,
            }
        },
        "verdict": {
            "recall_not_worse": recall_not_worse,
            "reduction_not_worse": reduction_not_worse,
            "no_overreach": no_overreach,
            "pareto_win": pareto_win,
        }
    });
    let json_path = cli.out_dir.join(format!("maturity_{date}.json"));
    std::fs::write(&json_path, serde_json::to_string_pretty(&summary)?)?;

    println!("wrote {}", md_path.display());
    println!("wrote {}", json_path.display());

    if !pareto_win {
        eprintln!("note: metric-driven arm did not establish a Pareto win — see report for axes");
    }
    Ok(())
}
