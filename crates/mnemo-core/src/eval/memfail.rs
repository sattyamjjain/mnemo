//! MemFail-style per-operation fault-isolation harness.
//!
//! # Anchor
//!
//! MemFail frames a long-running agent's recall pipeline as a chain of
//! distinct operations — *summarize* → *store* → *retrieve* — and
//! makes the per-operation behaviour the testable unit. A failure
//! observed at the recall surface is decomposed into the single stage
//! responsible for it.
//!
//! mnemo's operation seams are the three primitives exposed on
//! [`MnemoEngine`](crate::query::MnemoEngine):
//!
//! - **store** = [`MnemoEngine::remember`] — write a [`MemoryRecord`]
//!   into the [`StorageBackend`](crate::storage::StorageBackend)
//!   *and* the vector + full-text indices, emit a `MemoryWrite`
//!   event, link the hash chain.
//! - **summarize** = [`MnemoEngine::run_consolidation`] — cluster
//!   episodic records by tag overlap and replace each cluster with a
//!   structured `[Consolidated from N memories] …` semantic bundle
//!   (`SourceType::Consolidation`) plus `consolidated_from` relations.
//!   Closest mnemo analogue to MemFail's "summarize" stage.
//! - **retrieve** = [`MnemoEngine::recall`] — score the active bank
//!   under the active retrieval mode (hybrid RRF by default), return
//!   the top-k.
//!
//! # What this harness is
//!
//! A set of *adversarial probes* — one per operation — each
//! engineered so a failed assertion is **attributable to exactly one
//! stage**. The probes are run in order (store → summarize →
//! retrieve); a downstream probe trusts its upstream peers because
//! their probes already passed in the same run.
//!
//! The canonical
//! [`run_stale_context_fixture`] case demonstrates the attribution
//! shape: write the same fact twice (older write at high importance,
//! newer write at low importance), then recall. The default hybrid
//! ranker returns the older / stale record on top. Store + summarize
//! probes succeed (both records are in storage with correct content
//! hashes; no consolidation has run), so the harness attributes the
//! stale recall to the retrieve stage — exactly the MemFail
//! "isolate the operation" output.
//!
//! # What this harness is NOT
//!
//! - **Not a recall-quality benchmark.** The probes target seams, not
//!   retrieval-mode quality. Use [`bench/locomo`] for quality numbers.
//! - **Not a faithful MemFail reproduction.** The arXiv reference is
//!   prior-art-only; mnemo's harness exercises the three primitives
//!   that the public MCP surface actually exposes.
//! - **Not a write-side guard.** A failed probe surfaces an
//!   attribution; gating production writes on it is the caller's
//!   choice.
//!
//! [`bench/locomo`]: https://github.com/sattyamjjain/mnemo/tree/main/bench/locomo

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::memory::{ConsolidationState, MemoryType, SourceType};
use crate::query::MnemoEngine;
use crate::query::recall::RecallRequest;
use crate::query::remember::RememberRequest;
use crate::storage::MemoryFilter;

/// Identifier for the three operation stages MemFail decomposes a
/// recall into. The harness reports findings keyed on this enum so
/// callers can pivot dashboards by stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Store,
    Summarize,
    Retrieve,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Store => "store",
            Stage::Summarize => "summarize",
            Stage::Retrieve => "retrieve",
        }
    }
}

/// Outcome of a single adversarial probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeOutcome {
    pub name: String,
    pub passed: bool,
    /// Operator-readable diagnostic. Empty string on pass.
    pub detail: String,
}

impl ProbeOutcome {
    fn pass(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            detail: String::new(),
        }
    }

    fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            detail: detail.into(),
        }
    }
}

/// Result of running every probe in a single stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageReport {
    pub stage: Stage,
    pub probes: Vec<ProbeOutcome>,
}

impl StageReport {
    pub fn passed(&self) -> bool {
        self.probes.iter().all(|p| p.passed)
    }

    pub fn failing_probes(&self) -> Vec<&ProbeOutcome> {
        self.probes.iter().filter(|p| !p.passed).collect()
    }
}

/// Output of [`run_stale_context_fixture`]: the canonical MemFail
/// case. `attributed_stage` is the single stage the harness blames
/// for the observed failure based on which upstream probes still
/// pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionReport {
    /// Human-readable description of the failure the fixture
    /// observed at the recall surface.
    pub observed_failure: String,
    /// `true` when every upstream probe (store, summarize) passed so
    /// the harness was able to isolate one stage.
    pub isolated: bool,
    /// The stage the harness assigns responsibility to.
    pub attributed_stage: Stage,
    /// Per-stage assertions consulted to compute the attribution.
    pub evidence: Vec<String>,
    /// Per-stage reports leading up to the attribution.
    pub store_report: StageReport,
    pub summarize_report: StageReport,
}

// ---------------------------------------------------------------------------
// Store probes
// ---------------------------------------------------------------------------

/// Run the **store** adversarial probe set against `engine`.
///
/// Every probe touches storage directly (no recall ranking, no
/// consolidation), so a failure is attributable to
/// [`MnemoEngine::remember`] or the underlying
/// [`StorageBackend`](crate::storage::StorageBackend).
pub async fn run_store_probes(engine: &MnemoEngine, agent_id: &str) -> Result<StageReport> {
    let mut probes = Vec::new();

    // (1) Content + hash round-trip via direct storage fetch.
    {
        let needle = format!("STORE-NEEDLE-{}", Uuid::now_v7());
        let mut req = RememberRequest::new(needle.clone());
        req.agent_id = Some(agent_id.to_string());
        let resp = engine.remember(req).await?;
        match engine.storage.get_memory(resp.id).await? {
            Some(record) => {
                if record.content != needle {
                    probes.push(ProbeOutcome::fail(
                        "store.content_roundtrip",
                        format!("stored content '{}' != input '{}'", record.content, needle),
                    ));
                } else if record.content_hash.is_empty() {
                    probes.push(ProbeOutcome::fail(
                        "store.content_roundtrip",
                        "stored record carries empty content_hash",
                    ));
                } else {
                    probes.push(ProbeOutcome::pass("store.content_roundtrip"));
                }
            }
            None => probes.push(ProbeOutcome::fail(
                "store.content_roundtrip",
                format!("get_memory({}) returned None after remember", resp.id),
            )),
        }
    }

    // (2) Distinct ids + bank-size growth.
    {
        let pre = list_active(engine, agent_id).await?.len();
        let n = 5;
        for i in 0..n {
            let mut req = RememberRequest::new(format!("STORE-ATOM-{}-{i}", Uuid::now_v7()));
            req.agent_id = Some(agent_id.to_string());
            engine.remember(req).await?;
        }
        let post = list_active(engine, agent_id).await?;
        let added = post.len().saturating_sub(pre);
        if added != n {
            probes.push(ProbeOutcome::fail(
                "store.bank_size_growth",
                format!("expected +{n} active records, got +{added}"),
            ));
        } else {
            let mut ids: Vec<Uuid> = post.iter().map(|r| r.id).collect();
            ids.sort();
            ids.dedup();
            if ids.len() != post.len() {
                probes.push(ProbeOutcome::fail(
                    "store.bank_size_growth",
                    "duplicate ids after batch remember",
                ));
            } else {
                probes.push(ProbeOutcome::pass("store.bank_size_growth"));
            }
        }
    }

    // (3) Tag round-trip.
    {
        let mut req = RememberRequest::new(format!("STORE-TAGGED-{}", Uuid::now_v7()));
        req.agent_id = Some(agent_id.to_string());
        req.tags = Some(vec!["memfail.alpha".into(), "memfail.beta".into()]);
        let resp = engine.remember(req).await?;
        let rec = engine
            .storage
            .get_memory(resp.id)
            .await?
            .ok_or_else(|| Error::Validation("tagged record missing".into()))?;
        let has_alpha = rec.tags.iter().any(|t| t == "memfail.alpha");
        let has_beta = rec.tags.iter().any(|t| t == "memfail.beta");
        if has_alpha && has_beta {
            probes.push(ProbeOutcome::pass("store.tag_roundtrip"));
        } else {
            probes.push(ProbeOutcome::fail(
                "store.tag_roundtrip",
                format!(
                    "tags lost on round-trip: alpha={has_alpha}, beta={has_beta}, observed={:?}",
                    rec.tags
                ),
            ));
        }
    }

    Ok(StageReport {
        stage: Stage::Store,
        probes,
    })
}

// ---------------------------------------------------------------------------
// Summarize probes
// ---------------------------------------------------------------------------

/// Run the **summarize** adversarial probe set against `engine`.
///
/// Each probe inspects post-consolidation state via direct storage
/// reads (no recall ranking), so a failure is attributable to
/// [`MnemoEngine::run_consolidation`] / the lifecycle module.
pub async fn run_summarize_probes(engine: &MnemoEngine, agent_id: &str) -> Result<StageReport> {
    let mut probes = Vec::new();

    // Build an isolated cluster so we do not collide with whatever
    // store probes left behind.
    let topic = format!("memfail-cluster-{}", Uuid::now_v7());
    let needle = format!("SUMMARIZE-NEEDLE-{}", Uuid::now_v7());

    let mut needle_req = RememberRequest::new(needle.clone());
    needle_req.agent_id = Some(agent_id.to_string());
    needle_req.tags = Some(vec![topic.clone()]);
    let needle_resp = engine.remember(needle_req).await?;

    let mut companion_ids = Vec::with_capacity(2);
    for i in 0..2 {
        let mut req = RememberRequest::new(format!("companion-{i}-{}", Uuid::now_v7()));
        req.agent_id = Some(agent_id.to_string());
        req.tags = Some(vec![topic.clone()]);
        let resp = engine.remember(req).await?;
        companion_ids.push(resp.id);
    }

    let result = engine
        .run_consolidation(Some(agent_id.to_string()), 3)
        .await?;

    // (1) At least one cluster consolidated.
    if result.clusters_found == 0 || result.new_memories_created == 0 {
        probes.push(ProbeOutcome::fail(
            "summarize.cluster_emitted",
            format!(
                "run_consolidation reported clusters_found={} new={}",
                result.clusters_found, result.new_memories_created
            ),
        ));
    } else {
        probes.push(ProbeOutcome::pass("summarize.cluster_emitted"));
    }

    let active = list_active(engine, agent_id).await?;
    let bundles: Vec<_> = active
        .iter()
        .filter(|r| {
            r.source_type == SourceType::Consolidation && r.memory_type == MemoryType::Semantic
        })
        .collect();

    // (2) Needle survives the bundle verbatim — the canonical
    // summarize fault is content loss.
    let bundle_with_needle = bundles.iter().find(|b| b.content.contains(&needle));
    match bundle_with_needle {
        Some(b) => {
            if b.tags.iter().any(|t| t == &topic) {
                probes.push(ProbeOutcome::pass("summarize.needle_preservation"));
            } else {
                probes.push(ProbeOutcome::fail(
                    "summarize.needle_preservation",
                    format!("bundle missing cluster topic tag: {:?}", b.tags),
                ));
            }
        }
        None => {
            probes.push(ProbeOutcome::fail(
                "summarize.needle_preservation",
                format!(
                    "needle string '{needle}' not found in any of {} Consolidation bundle(s)",
                    bundles.len()
                ),
            ));
        }
    }

    // (3) Originals are flipped to Consolidated state (audit chain stays alive).
    let needle_after = engine.storage.get_memory(needle_resp.id).await?;
    let state = needle_after
        .as_ref()
        .map(|r| r.consolidation_state)
        .unwrap_or(ConsolidationState::Raw);
    if state == ConsolidationState::Consolidated {
        probes.push(ProbeOutcome::pass("summarize.original_marked_consolidated"));
    } else {
        probes.push(ProbeOutcome::fail(
            "summarize.original_marked_consolidated",
            format!(
                "expected needle original ({}) in state Consolidated, observed {:?}",
                needle_resp.id, state
            ),
        ));
    }

    Ok(StageReport {
        stage: Stage::Summarize,
        probes,
    })
}

// ---------------------------------------------------------------------------
// Retrieve probes
// ---------------------------------------------------------------------------

/// Run the **retrieve** adversarial probe set against `engine`.
///
/// Each probe assumes [`run_store_probes`] passed: it remembers a
/// record, then asserts something about the ranked recall result.
/// Because store has already been verified in the same run, a
/// failure here points at the recall path.
pub async fn run_retrieve_probes(engine: &MnemoEngine, agent_id: &str) -> Result<StageReport> {
    let mut probes = Vec::new();

    // (1) Direct hit: the unique needle text must appear in the
    // top-k of a query that contains the needle verbatim.
    {
        let needle = format!("RETRIEVE-NEEDLE-{}", Uuid::now_v7());
        let mut req = RememberRequest::new(needle.clone());
        req.agent_id = Some(agent_id.to_string());
        req.tags = Some(vec!["memfail.retrieve.direct".into()]);
        engine.remember(req).await?;

        let mut rec = RecallRequest::new(needle.clone());
        rec.agent_id = Some(agent_id.to_string());
        rec.limit = Some(10);
        rec.strategy = Some("auto".into());
        let resp = engine.recall(rec).await?;
        if resp.memories.iter().any(|m| m.content.contains(&needle)) {
            probes.push(ProbeOutcome::pass("retrieve.direct_hit"));
        } else {
            probes.push(ProbeOutcome::fail(
                "retrieve.direct_hit",
                format!(
                    "needle '{needle}' missing from top-{} recall (got {} hits)",
                    10,
                    resp.memories.len()
                ),
            ));
        }
    }

    // (2) Tag filter: a recall scoped by tag must return a memory
    // carrying that tag.
    {
        let tag = format!("memfail.retrieve.tag.{}", Uuid::now_v7());
        let mut req = RememberRequest::new(format!("retrieve-by-tag-{}", Uuid::now_v7()));
        req.agent_id = Some(agent_id.to_string());
        req.tags = Some(vec![tag.clone()]);
        engine.remember(req).await?;

        let mut rec = RecallRequest::new("retrieve-by-tag".into());
        rec.agent_id = Some(agent_id.to_string());
        rec.tags = Some(vec![tag.clone()]);
        rec.limit = Some(10);
        rec.strategy = Some("auto".into());
        let resp = engine.recall(rec).await?;
        let any_tagged = resp.memories.iter().any(|m| m.tags.contains(&tag));
        if any_tagged {
            probes.push(ProbeOutcome::pass("retrieve.tag_filter"));
        } else {
            probes.push(ProbeOutcome::fail(
                "retrieve.tag_filter",
                format!(
                    "no recall result carried tag '{tag}' ({} hits)",
                    resp.memories.len()
                ),
            ));
        }
    }

    Ok(StageReport {
        stage: Stage::Retrieve,
        probes,
    })
}

// ---------------------------------------------------------------------------
// Stale-context fixture (canonical MemFail case)
// ---------------------------------------------------------------------------

/// Canonical MemFail attribution fixture.
///
/// Writes the same fact twice — an older write at `high` importance
/// and a newer write at `low` importance — then asks the retrieve
/// stage for the fact. The default hybrid ranker returns the older
/// (stale) record on top. Store + summarize probes succeed (both
/// records are in storage with correct content hashes; no
/// consolidation has run), so the harness attributes the failure to
/// retrieve.
///
/// The fixture asserts the attribution shape, not the retriever's
/// quality on this scenario — the v0.4.7 current-fact-resolver
/// (`fact_key` post-processor on `RecallRequest`) is the documented
/// opt-in mitigation. The point of the fixture is that the harness
/// can *isolate the operation responsible* when the failure is
/// observed.
pub async fn run_stale_context_fixture(
    engine: &MnemoEngine,
    agent_id: &str,
) -> Result<AttributionReport> {
    let mut evidence = Vec::new();

    // Older write (stale): high importance so the default ranker
    // prefers it.
    let stale_content = format!("USER-COLOR-{}: blue (older write)", Uuid::now_v7());
    let mut stale_req = RememberRequest::new(stale_content.clone());
    stale_req.agent_id = Some(agent_id.to_string());
    stale_req.tags = Some(vec!["memfail.stale.user-color".into()]);
    stale_req.importance = Some(0.95);
    let stale_resp = engine.remember(stale_req).await?;

    // Yield so the second write lands at a strictly later timestamp.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    // Newer write (current): low importance, deliberately
    // de-prioritised by the default ranker.
    let current_content = format!("USER-COLOR-{}: red (current write)", Uuid::now_v7());
    let mut current_req = RememberRequest::new(current_content.clone());
    current_req.agent_id = Some(agent_id.to_string());
    current_req.tags = Some(vec!["memfail.stale.user-color".into()]);
    current_req.importance = Some(0.05);
    let current_resp = engine.remember(current_req).await?;

    // ---- Store stage: directly verify both records exist verbatim.
    let mut store_probes = Vec::new();
    let stale_after = engine.storage.get_memory(stale_resp.id).await?;
    let current_after = engine.storage.get_memory(current_resp.id).await?;
    let stale_ok = stale_after.as_ref().map(|r| r.content.as_str()) == Some(stale_content.as_str());
    let current_ok =
        current_after.as_ref().map(|r| r.content.as_str()) == Some(current_content.as_str());
    store_probes.push(if stale_ok {
        ProbeOutcome::pass("stale.store.older_write")
    } else {
        ProbeOutcome::fail(
            "stale.store.older_write",
            format!("older record content drifted: {stale_after:?}"),
        )
    });
    store_probes.push(if current_ok {
        ProbeOutcome::pass("stale.store.newer_write")
    } else {
        ProbeOutcome::fail(
            "stale.store.newer_write",
            format!("newer record content drifted: {current_after:?}"),
        )
    });
    let store_report = StageReport {
        stage: Stage::Store,
        probes: store_probes,
    };

    // ---- Summarize stage: no consolidation should have fired in
    // this fixture, so the active bank still contains both records
    // as raw episodic writes (consolidation_state ∈ {Raw, Active}).
    // Any Consolidation-source bundle covering either id would shift
    // the blame upstream.
    let mut summarize_probes = Vec::new();
    let active = list_active(engine, agent_id).await?;
    let consolidation_bundles_touching_fact = active
        .iter()
        .filter(|r| r.source_type == SourceType::Consolidation)
        .filter(|r| r.content.contains(&stale_content) || r.content.contains(&current_content))
        .count();
    let stale_unconsolidated = stale_after
        .as_ref()
        .map(|r| r.consolidation_state != ConsolidationState::Consolidated)
        .unwrap_or(false);
    let current_unconsolidated = current_after
        .as_ref()
        .map(|r| r.consolidation_state != ConsolidationState::Consolidated)
        .unwrap_or(false);
    summarize_probes.push(if consolidation_bundles_touching_fact == 0 {
        ProbeOutcome::pass("stale.summarize.no_bundle_touches_fact")
    } else {
        ProbeOutcome::fail(
            "stale.summarize.no_bundle_touches_fact",
            format!("{consolidation_bundles_touching_fact} Consolidation bundle(s) cover the fact"),
        )
    });
    summarize_probes.push(if stale_unconsolidated && current_unconsolidated {
        ProbeOutcome::pass("stale.summarize.both_records_unconsolidated")
    } else {
        ProbeOutcome::fail(
            "stale.summarize.both_records_unconsolidated",
            format!(
                "older.state={:?} newer.state={:?}",
                stale_after.as_ref().map(|r| r.consolidation_state),
                current_after.as_ref().map(|r| r.consolidation_state),
            ),
        )
    });
    let summarize_report = StageReport {
        stage: Stage::Summarize,
        probes: summarize_probes,
    };

    // ---- Retrieve stage: ask the recall surface.
    let mut rec = RecallRequest::new("USER-COLOR".to_string());
    rec.agent_id = Some(agent_id.to_string());
    rec.tags = Some(vec!["memfail.stale.user-color".into()]);
    rec.limit = Some(5);
    rec.strategy = Some("auto".into());
    let resp = engine.recall(rec).await?;

    let top_id = resp.memories.first().map(|m| m.id);
    let returned_stale_on_top = top_id == Some(stale_resp.id);
    let observed_failure = if returned_stale_on_top {
        format!(
            "default ranker returned older write ({}) above newer write ({}) for the same fact",
            stale_resp.id, current_resp.id
        )
    } else {
        format!(
            "recall surfaced current record first ({:?}); the fixture's stale-bias setup did not reproduce",
            top_id
        )
    };
    evidence.push(format!("recall.top_id = {top_id:?}"));
    evidence.push(format!(
        "store.older_write_intact = {}, store.newer_write_intact = {}",
        stale_ok, current_ok
    ));
    evidence.push(format!(
        "summarize.bundles_touching_fact = {consolidation_bundles_touching_fact}"
    ));
    evidence.push(format!(
        "summarize.both_records_unconsolidated = {}",
        stale_unconsolidated && current_unconsolidated
    ));

    let upstream_ok = store_report.passed() && summarize_report.passed();
    let attributed_stage = if !store_report.passed() {
        Stage::Store
    } else if !summarize_report.passed() {
        Stage::Summarize
    } else {
        Stage::Retrieve
    };

    Ok(AttributionReport {
        observed_failure,
        isolated: upstream_ok,
        attributed_stage,
        evidence,
        store_report,
        summarize_report,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn list_active(
    engine: &MnemoEngine,
    agent_id: &str,
) -> Result<Vec<crate::model::memory::MemoryRecord>> {
    let filter = MemoryFilter {
        agent_id: Some(agent_id.to_string()),
        include_deleted: false,
        ..Default::default()
    };
    engine
        .storage
        .list_memories(&filter, super::super::query::MAX_BATCH_QUERY_LIMIT, 0)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::NoopEmbedding;
    use crate::index::usearch::UsearchIndex;
    use crate::search::tantivy_index::TantivyFullTextIndex;
    use crate::storage::duckdb::DuckDbStorage;
    use std::sync::Arc;

    fn build_engine() -> MnemoEngine {
        let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
        let index = Arc::new(UsearchIndex::new(3).unwrap());
        let embedding = Arc::new(NoopEmbedding::new(3));
        let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
        MnemoEngine::new(storage, index, embedding, "memfail-agent".into(), None).with_full_text(ft)
    }

    #[tokio::test]
    async fn store_probes_pass_on_a_well_formed_engine() {
        let engine = build_engine();
        let report = run_store_probes(&engine, "memfail-agent").await.unwrap();
        assert!(
            report.passed(),
            "store probes must pass on default engine: {:?}",
            report.failing_probes()
        );
        assert_eq!(report.stage, Stage::Store);
        assert_eq!(report.probes.len(), 3);
    }

    #[tokio::test]
    async fn summarize_probes_pass_on_a_well_formed_engine() {
        let engine = build_engine();
        let report = run_summarize_probes(&engine, "memfail-agent")
            .await
            .unwrap();
        assert!(
            report.passed(),
            "summarize probes must pass on default engine: {:?}",
            report.failing_probes()
        );
        assert_eq!(report.stage, Stage::Summarize);
        assert_eq!(report.probes.len(), 3);
    }

    #[tokio::test]
    async fn retrieve_probes_pass_on_a_well_formed_engine() {
        let engine = build_engine();
        let report = run_retrieve_probes(&engine, "memfail-agent").await.unwrap();
        assert!(
            report.passed(),
            "retrieve probes must pass on default engine: {:?}",
            report.failing_probes()
        );
        assert_eq!(report.stage, Stage::Retrieve);
        assert_eq!(report.probes.len(), 2);
    }
}
