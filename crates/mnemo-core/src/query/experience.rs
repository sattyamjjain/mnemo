//! Experience-memory tier (DocTrace, arXiv:2606.10921).
//!
//! DocTrace's two-tier idea: tier 1 is the raw memory store (everything
//! mnemo already does); tier 2 is an **experience memory** that caches a
//! *successful* retrieval/reasoning **plan** — the query signature, the
//! steps taken, the chunks that led to a confirmed-good outcome, and an
//! outcome score — and **replays** that plan when a structurally-similar
//! query recurs, instead of re-running full retrieval from scratch.
//!
//! This is implemented as a **mode, not a new store**: plans are
//! persisted as ordinary [`MemoryRecord`]s carrying the reserved
//! [`EXPERIENCE_PLAN_TAG`] with the plan payload in `metadata`. That
//! buys backend-agnosticism (DuckDB + PostgreSQL, unchanged schema) and
//! RBAC/consent (scope + ACL) for free, and lets the existing
//! `remember` write-path handle hashing, embedding, and audit events.
//!
//! # Two new ops (gated)
//!
//! - **`REMEMBER_PLAN`** ([`execute_remember_plan`]): persist a plan,
//!   but only when its `outcome_score` clears [`DEFAULT_SUCCESS_THRESHOLD`]
//!   — failures are never cached.
//! - **`RECALL_PLAN`** ([`execute_recall_plan`]): on a new query, return
//!   the best stored plan whose query signature matches above a
//!   similarity threshold, or a miss.
//!
//! Both are gated behind [`MnemoEngine::with_experience_memory`]; with
//! the mode **off** (the default) `REMEMBER_PLAN` is a validation error
//! and `RECALL_PLAN` always misses, so default behaviour is unchanged.
//! Plan records are also excluded from ordinary `recall` (they are
//! replayed only via `RECALL_PLAN`).
//!
//! # Signature & similarity
//!
//! A query *signature* is its normalized significant-token set
//! ([`signature_tokens`]); structural similarity is the
//! [`jaccard`] overlap of two signatures. This is deterministic and
//! backend-/embedder-agnostic (works under `NoopEmbedding`), which is
//! what the v0 replay gate needs; a learned signature is a later knob.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::memory::{MemoryType, Scope, SourceType};
use crate::query::MnemoEngine;
use crate::query::remember::RememberRequest;
use crate::storage::MemoryFilter;

/// Reserved tag stamped on every experience-tier plan record. Plans are
/// excluded from ordinary `recall` and read back only via `RECALL_PLAN`.
pub const EXPERIENCE_PLAN_TAG: &str = "__experience_plan__";
/// Metadata key under which the [`PlanPayload`] is serialized.
pub const PLAN_METADATA_KEY: &str = "experience_plan";
/// Default Jaccard threshold above which a stored plan is replayed.
pub const DEFAULT_SIMILARITY_THRESHOLD: f32 = 0.7;
/// Plans scoring below this outcome are treated as failures and never
/// cached.
pub const DEFAULT_SUCCESS_THRESHOLD: f32 = 0.5;
/// Cap on plan records scanned per `RECALL_PLAN`.
const PLAN_SCAN_LIMIT: usize = 1000;

/// The cached plan payload, serialized into the record `metadata`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanPayload {
    /// The original query the plan was confirmed good for.
    pub query: String,
    /// Normalized signature tokens (sorted, deduped).
    pub signature_tokens: Vec<String>,
    /// Ordered retrieval/reasoning steps taken.
    pub steps: Vec<String>,
    /// Ids of the chunks that led to the confirmed-good outcome.
    pub chunk_ids: Vec<String>,
    /// Confirmed outcome score in [0.0, 1.0].
    pub outcome_score: f32,
}

/// `REMEMBER_PLAN` input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberPlanRequest {
    /// The query the plan succeeded for.
    pub query: String,
    /// Ordered retrieval/reasoning steps to replay.
    pub steps: Vec<String>,
    /// Chunk ids that produced the confirmed-good outcome.
    pub chunk_ids: Vec<String>,
    /// Confirmed outcome score in [0.0, 1.0].
    pub outcome_score: f32,
    /// Owning agent (defaults to the engine default agent).
    pub agent_id: Option<String>,
    /// Visibility scope (defaults to `Private` — an agent replays its own
    /// plans). `Shared` plans honour the ACL on read.
    pub scope: Option<Scope>,
    /// Organization id for multi-tenant scoping.
    pub org_id: Option<String>,
}

/// `REMEMBER_PLAN` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberPlanResponse {
    /// Id of the stored plan record, if it was persisted.
    pub id: Option<Uuid>,
    /// The computed signature (sorted token list, space-joined).
    pub signature: String,
    /// `true` if the plan cleared the success threshold and was stored;
    /// `false` if it was rejected as a non-success (not cached).
    pub stored: bool,
}

/// `RECALL_PLAN` input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallPlanRequest {
    /// The new query to look up a replayable plan for.
    pub query: String,
    /// Requesting agent (defaults to the engine default agent).
    pub agent_id: Option<String>,
    /// Organization id for multi-tenant scoping.
    pub org_id: Option<String>,
    /// Override the replay similarity threshold
    /// ([`DEFAULT_SIMILARITY_THRESHOLD`] when `None`).
    pub similarity_threshold: Option<f32>,
}

/// A replayable plan plus the similarity that matched it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPlan {
    /// Plan record id.
    pub id: Uuid,
    /// The query the plan was originally confirmed good for.
    pub query: String,
    /// Ordered steps to replay.
    pub steps: Vec<String>,
    /// Chunk ids to return instead of re-running retrieval.
    pub chunk_ids: Vec<String>,
    /// Original outcome score.
    pub outcome_score: f32,
    /// Jaccard similarity between the incoming query and this plan.
    pub similarity: f32,
}

/// `RECALL_PLAN` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallPlanResponse {
    /// The best replayable plan above threshold, or `None` on a miss.
    pub plan: Option<CachedPlan>,
    /// How many visible plan records were considered.
    pub candidates_considered: usize,
}

/// Normalize a query into its signature token set: lowercase, split on
/// non-alphanumerics, drop tokens shorter than 3 chars, dedup, and sort
/// so the signature is order-independent.
pub fn signature_tokens(query: &str) -> Vec<String> {
    let mut tokens: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 3)
        .map(|t| t.to_lowercase())
        .collect();
    tokens.sort();
    tokens.dedup();
    tokens
}

/// Jaccard overlap `|A∩B| / |A∪B|` of two signature token sets. Returns
/// `0.0` when both are empty.
pub fn jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let mut intersection = 0usize;
    for t in a {
        if b.contains(t) {
            intersection += 1;
        }
    }
    let union = a.len() + b.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}

/// Whether `agent_id` may read `record` under mnemo's scope/ACL rules —
/// the same consent gate ordinary recall enforces.
async fn plan_visible_to(
    engine: &MnemoEngine,
    record: &crate::model::memory::MemoryRecord,
    agent_id: &str,
) -> bool {
    match record.scope {
        Scope::Public | Scope::Global => true,
        Scope::Private => record.agent_id == agent_id,
        Scope::Shared => {
            record.agent_id == agent_id
                || engine
                    .storage
                    .check_permission(record.id, agent_id, crate::model::acl::Permission::Read)
                    .await
                    .unwrap_or(false)
        }
    }
}

/// `REMEMBER_PLAN` — persist a successful plan into the experience tier.
pub async fn execute_remember_plan(
    engine: &MnemoEngine,
    request: RememberPlanRequest,
) -> Result<RememberPlanResponse> {
    if !engine.experience_memory_enabled {
        return Err(Error::Validation(
            "experience memory mode is disabled; enable it with MnemoEngine::with_experience_memory()".to_string(),
        ));
    }
    let tokens = signature_tokens(&request.query);
    let signature = tokens.join(" ");

    // Only confirmed-good outcomes are cached — failures must not be
    // replayed. Binding the comparison first keeps NaN rejecting (NaN is
    // never a success) without a negated partial-ord comparison.
    let is_success = request.outcome_score >= DEFAULT_SUCCESS_THRESHOLD;
    if !is_success {
        return Ok(RememberPlanResponse {
            id: None,
            signature,
            stored: false,
        });
    }

    let payload = PlanPayload {
        query: request.query.clone(),
        signature_tokens: tokens,
        steps: request.steps,
        chunk_ids: request.chunk_ids,
        outcome_score: request.outcome_score.clamp(0.0, 1.0),
    };
    let metadata = serde_json::json!({ PLAN_METADATA_KEY: payload });

    // Persist via the ordinary write-path so hashing, embedding, audit
    // events, encryption and RBAC scope all apply uniformly.
    let mut rr = RememberRequest::new(request.query);
    rr.agent_id = request.agent_id;
    rr.org_id = request.org_id;
    rr.scope = Some(request.scope.unwrap_or(Scope::Private));
    rr.memory_type = Some(MemoryType::Procedural);
    rr.importance = Some(request.outcome_score.clamp(0.0, 1.0));
    rr.tags = Some(vec![EXPERIENCE_PLAN_TAG.to_string()]);
    rr.metadata = Some(metadata);
    rr.source_type = Some(SourceType::System);

    let resp = engine.remember(rr).await?;
    Ok(RememberPlanResponse {
        id: Some(resp.id),
        signature,
        stored: true,
    })
}

/// `RECALL_PLAN` — return the best replayable plan for a query, or a miss.
pub async fn execute_recall_plan(
    engine: &MnemoEngine,
    request: RecallPlanRequest,
) -> Result<RecallPlanResponse> {
    if !engine.experience_memory_enabled {
        return Ok(RecallPlanResponse {
            plan: None,
            candidates_considered: 0,
        });
    }
    let agent_id = request
        .agent_id
        .clone()
        .unwrap_or_else(|| engine.default_agent_id.clone());
    let threshold = request
        .similarity_threshold
        .unwrap_or(DEFAULT_SIMILARITY_THRESHOLD);
    let query_sig = signature_tokens(&request.query);

    // List plan records by reserved tag (no agent filter — RBAC is
    // applied per-record below so shared/public plans are visible too).
    let filter = MemoryFilter {
        agent_id: None,
        memory_type: None,
        scope: None,
        tags: Some(vec![EXPERIENCE_PLAN_TAG.to_string()]),
        min_importance: None,
        org_id: request.org_id.clone(),
        thread_id: None,
        include_deleted: false,
    };
    let records = engine
        .storage
        .list_memories(&filter, PLAN_SCAN_LIMIT, 0)
        .await?;

    let mut considered = 0usize;
    let mut best: Option<CachedPlan> = None;
    for record in &records {
        if record.is_deleted() || record.quarantined {
            continue;
        }
        if !plan_visible_to(engine, record, &agent_id).await {
            continue;
        }
        let Some(raw) = record.metadata.get(PLAN_METADATA_KEY) else {
            continue;
        };
        let Ok(payload) = serde_json::from_value::<PlanPayload>(raw.clone()) else {
            continue;
        };
        considered += 1;
        let sim = jaccard(&query_sig, &payload.signature_tokens);
        if sim >= threshold && best.as_ref().map(|b| sim > b.similarity).unwrap_or(true) {
            best = Some(CachedPlan {
                id: record.id,
                query: payload.query,
                steps: payload.steps,
                chunk_ids: payload.chunk_ids,
                outcome_score: payload.outcome_score,
                similarity: sim,
            });
        }
    }

    Ok(RecallPlanResponse {
        plan: best,
        candidates_considered: considered,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_order_independent_and_normalized() {
        let a = signature_tokens("How do I Reset my PASSWORD?");
        let b = signature_tokens("password reset — how to do it");
        // Shared significant tokens regardless of order/case/punctuation.
        assert!(a.contains(&"reset".to_string()));
        assert!(a.contains(&"password".to_string()));
        assert!(jaccard(&a, &b) > 0.0);
        // Sorted + deduped.
        let mut sorted = a.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(a, sorted);
    }

    #[test]
    fn jaccard_bounds() {
        let a = signature_tokens("alpha bravo charlie");
        assert_eq!(jaccard(&a, &a), 1.0);
        let disjoint = signature_tokens("xray yankee zulu");
        assert_eq!(jaccard(&a, &disjoint), 0.0);
        assert_eq!(jaccard(&[], &[]), 0.0);
    }
}
