use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use mnemo_core::error::Error as CoreError;
use mnemo_core::hash::compute_content_hash;
use mnemo_core::model::acl::Permission;
use mnemo_core::model::delegation::{Delegation, DelegationScope};
use mnemo_core::model::event::{AgentEvent, EventType};
use mnemo_core::model::memory::{MemoryType, Scope};
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::branch::{BranchRequest, BranchResponse};
use mnemo_core::query::checkpoint::{CheckpointRequest, CheckpointResponse};
use mnemo_core::query::forget::{
    ForgetRequest, ForgetResponse, ForgetStrategy, ForgetSubjectRequest, ForgetSubjectResponse,
};
use mnemo_core::query::merge::{MergeRequest, MergeResponse};
use mnemo_core::query::recall::{RecallRequest, RecallResponse};
use mnemo_core::query::remember::{RememberRequest, RememberResponse};
use mnemo_core::query::replay::{ReplayRequest, ReplayResponse};
use mnemo_core::query::share::{ShareRequest, ShareResponse};

type AppState = Arc<MnemoEngine>;

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

pub struct AppError(CoreError);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self.0 {
            CoreError::Validation(m) => (StatusCode::BAD_REQUEST, m.clone()),
            CoreError::PermissionDenied(m) => (StatusCode::FORBIDDEN, m.clone()),
            CoreError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            other => {
                tracing::error!("internal error: {other}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };
        (status, Json(serde_json::json!({"error": msg}))).into_response()
    }
}

impl From<CoreError> for AppError {
    fn from(e: CoreError) -> Self {
        AppError(e)
    }
}

// ---------------------------------------------------------------------------
// Query / body helper structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RecallParams {
    pub query: String,
    pub agent_id: Option<String>,
    pub limit: Option<usize>,
    pub memory_type: Option<String>,
    pub scope: Option<String>,
    pub min_importance: Option<f32>,
    pub tags: Option<String>,
    pub org_id: Option<String>,
    pub strategy: Option<String>,
    pub as_of: Option<String>,
    pub memory_types: Option<String>,
    pub hybrid_weights: Option<String>,
    pub rrf_k: Option<f32>,
    pub explain: Option<bool>,
    /// v0.4.7 — opt-in current-fact resolver. Set the metadata key
    /// to scope fact identity by (typical: `fact_id`). When set,
    /// the response returns the most-recent write per fact group.
    pub current_fact_key: Option<String>,
    /// v0.4.7 — include the supersession chain in the response.
    /// Honored only when `current_fact_key` is also set.
    pub current_fact_include_chain: Option<bool>,
    /// v0.4.8 — opt-in orientation cache. When `true` AND the
    /// engine has an `OrientationCacheStore` attached, the recall
    /// updates a per-namespace, constant-token "context map" and
    /// returns a bounded rendering in
    /// `response.orientation_cache`. PEEK-anchored
    /// (arXiv:2605.19932).
    pub orientation_cache: Option<bool>,
    /// v0.4.8 — explicit namespace label. When omitted, the engine
    /// derives one from `(org_id, agent_id)`.
    pub orientation_namespace: Option<String>,
    /// v0.4.8 — token budget for the rendered map. Defaults to
    /// 512 when omitted.
    pub orientation_token_budget: Option<u32>,
    /// v0.4.8 — include the rendered map in the response.
    /// Defaults to `true` when omitted.
    pub orientation_include_in_response: Option<bool>,
    /// v0.4.8 — run the Distiller and update the in-process store.
    /// Defaults to `true` when omitted; set to `false` for warm-up
    /// or inspection calls that should not mutate the map.
    pub orientation_distill: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ForgetParams {
    pub strategy: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShareBody {
    pub target_agent_id: String,
    pub target_agent_ids: Option<Vec<String>>,
    pub permission: Option<String>,
    pub expires_in_hours: Option<f64>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyBody {
    pub agent_id: Option<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TrajectoryAuditBody {
    pub agent_id: Option<String>,
    pub thread_id: Option<String>,
    pub active_bank_ceiling: Option<usize>,
    pub fact_key: Option<String>,
    pub named_forget_strategies: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct DelegateRequest {
    pub delegate_id: String,
    pub permission: String,
    pub memory_ids: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub max_depth: Option<u32>,
    pub expires_in_hours: Option<f64>,
    /// The agent requesting delegation. Required — the server will verify
    /// this agent has `Delegate` permission on the target memories.
    pub agent_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/memories -- store a new memory.
pub async fn remember_handler(
    State(engine): State<AppState>,
    Json(request): Json<RememberRequest>,
) -> Result<Json<RememberResponse>, AppError> {
    let response = engine.remember(request).await?;
    Ok(Json(response))
}

/// GET /v1/memories?query=...&limit=...&memory_type=...&scope=...&strategy=...
pub async fn recall_handler(
    State(engine): State<AppState>,
    Query(params): Query<RecallParams>,
) -> Result<Json<RecallResponse>, AppError> {
    let memory_type = match params.memory_type.as_deref() {
        Some(s) => Some(s.parse::<MemoryType>().map_err(|_| {
            AppError(CoreError::Validation(format!(
                "invalid memory_type '{}': expected one of: episodic, semantic, procedural, working",
                s
            )))
        })?),
        None => None,
    };

    let scope = match params.scope.as_deref() {
        Some(s) => Some(s.parse::<Scope>().map_err(|_| {
            AppError(CoreError::Validation(format!(
                "invalid scope '{}': expected one of: private, shared, public, global",
                s
            )))
        })?),
        None => None,
    };

    let tags = params.tags.as_deref().map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>()
    });

    let memory_types = match params.memory_types.as_deref() {
        Some(s) => {
            let mut parsed = Vec::new();
            for t in s.split(',') {
                let trimmed = t.trim();
                let mt = trimmed.parse::<MemoryType>().map_err(|_| {
                    AppError(CoreError::Validation(format!(
                        "invalid memory_type '{}' in memory_types: expected one of: episodic, semantic, procedural, working",
                        trimmed
                    )))
                })?;
                parsed.push(mt);
            }
            Some(parsed)
        }
        None => None,
    };

    let hybrid_weights = match params.hybrid_weights.as_deref() {
        Some(s) => {
            let mut weights = Vec::new();
            for w in s.split(',') {
                let trimmed = w.trim();
                let val = trimmed.parse::<f32>().map_err(|_| {
                    AppError(CoreError::Validation(format!(
                        "invalid weight '{}' in hybrid_weights: expected a floating-point number",
                        trimmed
                    )))
                })?;
                weights.push(val);
            }
            Some(weights)
        }
        None => None,
    };

    let request = RecallRequest {
        query: params.query,
        agent_id: params.agent_id,
        limit: params.limit,
        memory_type,
        memory_types,
        scope,
        min_importance: params.min_importance,
        tags,
        org_id: params.org_id,
        strategy: params.strategy,
        temporal_range: None,
        recency_half_life_hours: None,
        hybrid_weights,
        rrf_k: params.rrf_k,
        as_of: params.as_of,
        explain: params.explain,
        with_provenance: None,
        mode: None,
        current_fact_resolver: params.current_fact_key.map(|fact_key| {
            mnemo_core::query::current_fact_resolver::CurrentFactResolverConfig {
                fact_key,
                include_supersession_chain: params.current_fact_include_chain.unwrap_or(false),
            }
        }),
        orientation_cache: if params.orientation_cache.unwrap_or(false) {
            Some(
                mnemo_core::query::orientation_cache::OrientationCacheConfig {
                    namespace: params.orientation_namespace.clone(),
                    token_budget: params.orientation_token_budget,
                    include_in_response: params.orientation_include_in_response.unwrap_or(true),
                    distill: params.orientation_distill.unwrap_or(true),
                },
            )
        } else {
            None
        },
    };

    let response = engine.recall(request).await?;
    Ok(Json(response))
}

/// GET /v1/memories/:id -- retrieve a single memory by UUID.
pub async fn get_memory_handler(
    State(engine): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let record = engine
        .storage
        .get_memory(id)
        .await?
        .ok_or_else(|| CoreError::NotFound(format!("memory {id} not found")))?;

    let value = serde_json::json!({
        "id": record.id,
        "agent_id": record.agent_id,
        "content": record.content,
        "memory_type": record.memory_type,
        "scope": record.scope,
        "importance": record.importance,
        "tags": record.tags,
        "metadata": record.metadata,
        "source_type": record.source_type,
        "source_id": record.source_id,
        "consolidation_state": record.consolidation_state,
        "access_count": record.access_count,
        "org_id": record.org_id,
        "thread_id": record.thread_id,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
        "last_accessed_at": record.last_accessed_at,
        "expires_at": record.expires_at,
        "deleted_at": record.deleted_at,
        "decay_rate": record.decay_rate,
        "created_by": record.created_by,
        "version": record.version,
        "prev_version_id": record.prev_version_id,
        "quarantined": record.quarantined,
        "quarantine_reason": record.quarantine_reason,
    });

    Ok(Json(value))
}

/// DELETE /v1/memories/:id?strategy=soft_delete|hard_delete|decay|consolidate|archive
pub async fn forget_handler(
    State(engine): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<ForgetParams>,
) -> Result<Json<ForgetResponse>, AppError> {
    let strategy = match params.strategy.as_deref() {
        Some(s) => Some(match s {
            "soft_delete" => ForgetStrategy::SoftDelete,
            "hard_delete" => ForgetStrategy::HardDelete,
            "decay" => ForgetStrategy::Decay,
            "consolidate" => ForgetStrategy::Consolidate,
            "archive" => ForgetStrategy::Archive,
            "redact" => ForgetStrategy::Redact,
            other => {
                return Err(AppError(CoreError::Validation(format!(
                    "invalid forget strategy '{}': expected one of: soft_delete, hard_delete, decay, consolidate, archive, redact",
                    other
                ))));
            }
        }),
        None => None,
    };

    let request = ForgetRequest {
        memory_ids: vec![id],
        agent_id: params.agent_id,
        strategy,
        criteria: None,
    };

    let response = engine.forget(request).await?;
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct ForgetSubjectBody {
    pub subject_id: String,
    pub strategy: Option<String>,
    pub agent_id: Option<String>,
}

/// POST /v1/forget_subject — GDPR / DPDPA-aligned subject erasure.
pub async fn forget_subject_handler(
    State(engine): State<AppState>,
    Json(body): Json<ForgetSubjectBody>,
) -> Result<Json<ForgetSubjectResponse>, AppError> {
    let strategy = match body.strategy.as_deref().unwrap_or("redact") {
        "redact" => ForgetStrategy::Redact,
        "hard_delete" => ForgetStrategy::HardDelete,
        "soft_delete" => ForgetStrategy::SoftDelete,
        other => {
            return Err(AppError(CoreError::Validation(format!(
                "invalid forget_subject strategy '{}': expected one of: redact, hard_delete, soft_delete",
                other
            ))));
        }
    };

    let request = ForgetSubjectRequest {
        subject_id: body.subject_id,
        agent_id: body.agent_id,
        strategy,
    };

    let response = engine.forget_subject(request).await?;
    Ok(Json(response))
}

/// POST /v1/memories/:id/share
pub async fn share_handler(
    State(engine): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ShareBody>,
) -> Result<Json<ShareResponse>, AppError> {
    let permission = match body.permission.as_deref() {
        Some(s) => Some(s.parse::<Permission>().map_err(|_| {
            AppError(CoreError::Validation(format!(
                "invalid permission '{}': expected one of: read, write, delete, share, delegate, admin",
                s
            )))
        })?),
        None => None,
    };

    let request = ShareRequest {
        memory_id: id,
        agent_id: body.agent_id,
        target_agent_id: body.target_agent_id,
        target_agent_ids: body.target_agent_ids,
        permission,
        expires_in_hours: body.expires_in_hours,
    };

    let response = engine.share(request).await?;
    Ok(Json(response))
}

/// POST /v1/checkpoints
pub async fn checkpoint_handler(
    State(engine): State<AppState>,
    Json(request): Json<CheckpointRequest>,
) -> Result<Json<CheckpointResponse>, AppError> {
    let response = engine.checkpoint(request).await?;
    Ok(Json(response))
}

/// POST /v1/branches
pub async fn branch_handler(
    State(engine): State<AppState>,
    Json(request): Json<BranchRequest>,
) -> Result<Json<BranchResponse>, AppError> {
    let response = engine.branch(request).await?;
    Ok(Json(response))
}

/// POST /v1/merge
pub async fn merge_handler(
    State(engine): State<AppState>,
    Json(request): Json<MergeRequest>,
) -> Result<Json<MergeResponse>, AppError> {
    let response = engine.merge(request).await?;
    Ok(Json(response))
}

/// POST /v1/replay
pub async fn replay_handler(
    State(engine): State<AppState>,
    Json(request): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, AppError> {
    let response = engine.replay(request).await?;
    Ok(Json(response))
}

/// POST /v1/verify -- verify hash chain integrity.
pub async fn verify_handler(
    State(engine): State<AppState>,
    Json(body): Json<VerifyBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = engine
        .verify_integrity(body.agent_id, body.thread_id.as_deref())
        .await?;

    let response = serde_json::json!({
        "valid": result.valid,
        "total_records": result.total_records,
        "verified_records": result.verified_records,
        "first_broken_at": result.first_broken_at.map(|id| id.to_string()),
        "error_message": result.error_message,
        "status": if result.valid { "verified" } else { "integrity_violation" },
    });

    Ok(Json(response))
}

/// POST /v1/compliance/trajectory_audit -- GEM-aligned trajectory audit.
///
/// Anchor: arXiv:2605.26252. Complements `/v1/verify` (per-record chain
/// integrity) on the orthogonal trajectory-correctness axis.
pub async fn trajectory_audit_handler(
    State(engine): State<AppState>,
    Json(body): Json<TrajectoryAuditBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let agent_id = body
        .agent_id
        .clone()
        .unwrap_or_else(|| engine.default_agent_id.clone());

    // Mirror verify_handler's storage fetch shape: list_events returns
    // DESC order; the trajectory audit needs chronological order.
    let mut events = engine
        .storage
        .list_events(&agent_id, mnemo_core::query::MAX_BATCH_QUERY_LIMIT, 0)
        .await?;
    events.reverse();

    let mut req = mnemo_compliance::trajectory::TrajectoryAuditRequest {
        agent_id: Some(agent_id),
        thread_id: body.thread_id.clone(),
        ..Default::default()
    };
    if let Some(c) = body.active_bank_ceiling {
        req.active_bank_ceiling = c;
    }
    if let Some(k) = body.fact_key {
        req.fact_key = k;
    }
    if let Some(s) = body.named_forget_strategies {
        req.named_forget_strategies = s;
    }

    let report = mnemo_compliance::trajectory::trajectory_audit(&events, &req)
        .map_err(|e| AppError(CoreError::Validation(e.to_string())))?;

    let response = serde_json::json!({
        "report": report,
        "all_ok": report.all_ok(),
    });

    Ok(Json(response))
}

/// POST /v1/delegate -- delegate permissions to another agent.
///
/// The caller must provide their `agent_id` and must have `Delegate`
/// permission on the target memories. Without a full auth middleware
/// this is advisory; production deployments should add an auth layer.
pub async fn delegate_handler(
    State(engine): State<AppState>,
    Json(body): Json<DelegateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let permission: Permission = body
        .permission
        .parse()
        .map_err(|e: CoreError| AppError(e))?;

    let caller_agent_id = body
        .agent_id
        .unwrap_or_else(|| engine.default_agent_id.clone());

    let scope = if let Some(ref ids) = body.memory_ids {
        let parsed: std::result::Result<Vec<Uuid>, _> =
            ids.iter().map(|s| Uuid::parse_str(s)).collect();
        match parsed {
            Ok(uuids) => {
                // Verify caller has Delegate permission on each memory
                for mid in &uuids {
                    let has_perm = engine
                        .storage
                        .check_permission(*mid, &caller_agent_id, Permission::Delegate)
                        .await?;
                    if !has_perm {
                        return Err(AppError(CoreError::PermissionDenied(format!(
                            "agent '{}' lacks delegate permission on memory {}",
                            caller_agent_id, mid
                        ))));
                    }
                }
                DelegationScope::ByMemoryId(uuids)
            }
            Err(e) => {
                return Err(AppError(CoreError::Validation(format!(
                    "invalid UUID in memory_ids: {e}"
                ))));
            }
        }
    } else if let Some(ref tags) = body.tags {
        DelegationScope::ByTag(tags.clone())
    } else {
        DelegationScope::AllMemories
    };

    let now = chrono::Utc::now();
    let expires_at = body
        .expires_in_hours
        .map(|h| (now + chrono::Duration::seconds((h * 3600.0) as i64)).to_rfc3339());

    let delegation = Delegation {
        id: Uuid::now_v7(),
        delegator_id: caller_agent_id,
        delegate_id: body.delegate_id.clone(),
        permission,
        scope,
        max_depth: body.max_depth.unwrap_or(0),
        current_depth: 0,
        parent_delegation_id: None,
        created_at: now.to_rfc3339(),
        expires_at,
        revoked_at: None,
    };

    engine.storage.insert_delegation(&delegation).await?;

    let response = serde_json::json!({
        "delegation_id": delegation.id.to_string(),
        "delegator": delegation.delegator_id,
        "delegate": delegation.delegate_id,
        "permission": delegation.permission.to_string(),
        "status": "delegated",
    });

    Ok(Json(response))
}

/// GET /v1/health
pub async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

// ---------------------------------------------------------------------------
// GenAI semantic convention helpers
// ---------------------------------------------------------------------------

struct GenAiFields {
    event_type: EventType,
    model: Option<String>,
    tokens_input: Option<i64>,
    tokens_output: Option<i64>,
    cost_usd: Option<f64>,
}

/// Extract GenAI semantic convention fields from OTLP span attributes.
/// See: <https://opentelemetry.io/docs/specs/semconv/gen-ai/>
fn extract_genai_fields(span: &serde_json::Value) -> GenAiFields {
    let attributes = span.get("attributes").and_then(|v| v.as_array());

    let mut model = None;
    let mut tokens_input = None;
    let mut tokens_output = None;
    let mut cost_usd = None;
    let mut operation_name = None;

    if let Some(attrs) = attributes {
        for attr in attrs {
            let key = match attr.get("key").and_then(|k| k.as_str()) {
                Some(k) => k,
                None => continue,
            };
            let value = attr.get("value");

            match key {
                "gen_ai.request.model" => {
                    model = value
                        .and_then(|v| v.get("stringValue"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
                "gen_ai.usage.input_tokens" => {
                    tokens_input = value.and_then(|v| v.get("intValue")).and_then(|v| {
                        v.as_str()
                            .and_then(|s| s.parse::<i64>().ok())
                            .or_else(|| v.as_i64())
                    });
                }
                "gen_ai.usage.output_tokens" => {
                    tokens_output = value.and_then(|v| v.get("intValue")).and_then(|v| {
                        v.as_str()
                            .and_then(|s| s.parse::<i64>().ok())
                            .or_else(|| v.as_i64())
                    });
                }
                "gen_ai.usage.cost" => {
                    cost_usd = value
                        .and_then(|v| v.get("doubleValue"))
                        .and_then(|v| v.as_f64());
                }
                "gen_ai.operation.name" => {
                    operation_name = value
                        .and_then(|v| v.get("stringValue"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
                _ => {}
            }
        }
    }

    // If no operation_name from attributes, fall back to span name.
    let op = operation_name.or_else(|| {
        span.get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    // Map operation name to EventType.
    let event_type = match op.as_deref() {
        Some(s) if s.contains("chat") => EventType::AssistantMessage,
        Some(s) if s.contains("embed") => EventType::RetrievalQuery,
        Some(s) if s.contains("tool") => EventType::ToolCall,
        _ => EventType::ToolCall, // default
    };

    GenAiFields {
        event_type,
        model,
        tokens_input,
        tokens_output,
        cost_usd,
    }
}

/// POST /v1/ingest/otlp -- ingest simplified OTLP JSON spans as agent events.
pub async fn otlp_ingest_handler(
    State(engine): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let resource_spans = body
        .get("resourceSpans")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut count: u64 = 0;

    for rs in &resource_spans {
        // Extract agent_id from resource attributes (service.name or agent.id).
        let resource_agent_id = rs
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .and_then(|attrs| attrs.as_array())
            .and_then(|attrs| {
                attrs.iter().find_map(|attr| {
                    let key = attr.get("key")?.as_str()?;
                    if key == "agent.id" || key == "service.name" {
                        attr.get("value")
                            .and_then(|v| v.get("stringValue"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
            });

        let scope_spans = rs
            .get("scopeSpans")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for ss in &scope_spans {
            let spans = ss
                .get("spans")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            for span in &spans {
                let trace_id = span
                    .get("traceId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let span_id = span
                    .get("spanId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let agent_id = resource_agent_id
                    .clone()
                    .unwrap_or_else(|| engine.default_agent_id.clone());

                // Compute latency from start/end nanosecond timestamps.
                // OTLP encodes nanos as either JSON strings or integers.
                let start_nano: u64 = span
                    .get("startTimeUnixNano")
                    .and_then(|v| {
                        v.as_str()
                            .and_then(|s| s.parse::<u64>().ok())
                            .or_else(|| v.as_u64())
                    })
                    .unwrap_or(0);

                let end_nano: u64 = span
                    .get("endTimeUnixNano")
                    .and_then(|v| {
                        v.as_str()
                            .and_then(|s| s.parse::<u64>().ok())
                            .or_else(|| v.as_u64())
                    })
                    .unwrap_or(0);

                let latency_ms = if end_nano > start_nano {
                    Some(((end_nano - start_nano) / 1_000_000) as i64)
                } else {
                    None
                };

                // Convert startTimeUnixNano to RFC3339 timestamp.
                let timestamp = if start_nano > 0 {
                    let secs = (start_nano / 1_000_000_000) as i64;
                    let nsecs = (start_nano % 1_000_000_000) as u32;
                    chrono::DateTime::from_timestamp(secs, nsecs)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339())
                } else {
                    chrono::Utc::now().to_rfc3339()
                };

                // Collect span attributes as the event payload.
                let payload = span
                    .get("attributes")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                let genai = extract_genai_fields(span);

                let content_hash =
                    compute_content_hash(&payload.to_string(), &agent_id, &timestamp);

                let event = AgentEvent {
                    id: Uuid::now_v7(),
                    agent_id,
                    thread_id: None,
                    run_id: None,
                    parent_event_id: None,
                    event_type: genai.event_type,
                    payload,
                    trace_id,
                    span_id,
                    model: genai.model,
                    tokens_input: genai.tokens_input,
                    tokens_output: genai.tokens_output,
                    latency_ms,
                    cost_usd: genai.cost_usd,
                    timestamp,
                    logical_clock: 0,
                    content_hash,
                    prev_hash: None,
                    embedding: None,
                };

                engine.storage.insert_event(&event).await?;
                count += 1;
            }
        }
    }

    Ok(Json(serde_json::json!({"accepted": count})))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_genai_fields_chat_span() {
        let span = serde_json::json!({
            "name": "chat gpt-4",
            "attributes": [
                {"key": "gen_ai.request.model", "value": {"stringValue": "gpt-4"}},
                {"key": "gen_ai.usage.input_tokens", "value": {"intValue": "150"}},
                {"key": "gen_ai.usage.output_tokens", "value": {"intValue": "50"}},
                {"key": "gen_ai.usage.cost", "value": {"doubleValue": 0.006}},
                {"key": "gen_ai.operation.name", "value": {"stringValue": "chat"}}
            ]
        });
        let fields = extract_genai_fields(&span);
        assert_eq!(fields.event_type, EventType::AssistantMessage);
        assert_eq!(fields.model.as_deref(), Some("gpt-4"));
        assert_eq!(fields.tokens_input, Some(150));
        assert_eq!(fields.tokens_output, Some(50));
        assert!((fields.cost_usd.unwrap() - 0.006).abs() < 1e-9);
    }

    #[test]
    fn test_extract_genai_fields_non_genai_default() {
        let span = serde_json::json!({
            "name": "http.request",
            "attributes": [
                {"key": "http.method", "value": {"stringValue": "GET"}}
            ]
        });
        let fields = extract_genai_fields(&span);
        assert_eq!(fields.event_type, EventType::ToolCall);
        assert!(fields.model.is_none());
        assert!(fields.tokens_input.is_none());
    }
}
