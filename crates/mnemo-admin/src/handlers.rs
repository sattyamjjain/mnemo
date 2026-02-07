use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mnemo_core::error::Error as CoreError;
use mnemo_core::query::MnemoEngine;
use mnemo_core::storage::MemoryFilter;

type AppState = Arc<MnemoEngine>;

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

pub struct AdminError(CoreError);

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self.0 {
            CoreError::Validation(m) => (StatusCode::BAD_REQUEST, m.clone()),
            CoreError::PermissionDenied(m) => (StatusCode::FORBIDDEN, m.clone()),
            CoreError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        };
        (status, Json(serde_json::json!({"error": msg}))).into_response()
    }
}

impl From<CoreError> for AdminError {
    fn from(e: CoreError) -> Self {
        AdminError(e)
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub memory_count: usize,
    pub event_count: usize,
    pub agent_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MemorySummary {
    pub id: String,
    pub agent_id: String,
    pub content_preview: String,
    pub memory_type: String,
    pub scope: String,
    pub importance: f32,
    pub quarantined: bool,
    pub quarantine_reason: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct EventSummary {
    pub id: String,
    pub agent_id: String,
    pub event_type: String,
    pub thread_id: Option<String>,
    pub timestamp: String,
    pub model: Option<String>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedMemories {
    pub memories: Vec<MemorySummary>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub struct PaginatedEvents {
    pub events: Vec<EventSummary>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub struct QuarantineResponse {
    pub id: String,
    pub quarantined: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MemoryQueryParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EventQueryParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /admin/ -- serve the embedded HTML dashboard.
pub async fn dashboard_handler() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

/// GET /admin/api/stats -- aggregate statistics.
pub async fn stats_handler(
    State(engine): State<AppState>,
) -> Result<Json<StatsResponse>, AdminError> {
    // Fetch a large batch of memories to count and extract unique agent IDs.
    // The storage backend does not expose a dedicated count or distinct-agents
    // query, so we page through with a generous limit.
    let filter = MemoryFilter::default();
    let memories = engine.storage.list_memories(&filter, 10_000, 0).await?;

    let memory_count = memories.len();
    let mut agent_ids: Vec<String> = memories
        .iter()
        .map(|m| m.agent_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    agent_ids.sort();

    // Sum up events across all known agents.
    let mut event_count: usize = 0;
    for aid in &agent_ids {
        let events = engine.storage.list_events(aid, 10_000, 0).await?;
        event_count += events.len();
    }

    Ok(Json(StatsResponse {
        memory_count,
        event_count,
        agent_ids,
    }))
}

/// GET /admin/api/agents -- list distinct agent IDs.
pub async fn agents_handler(
    State(engine): State<AppState>,
) -> Result<Json<Vec<String>>, AdminError> {
    let filter = MemoryFilter::default();
    let memories = engine.storage.list_memories(&filter, 10_000, 0).await?;

    let mut agent_ids: Vec<String> = memories
        .iter()
        .map(|m| m.agent_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    agent_ids.sort();

    Ok(Json(agent_ids))
}

/// GET /admin/api/memories?limit=50&offset=0&agent_id=X -- paginated memory browser.
pub async fn memories_handler(
    State(engine): State<AppState>,
    Query(params): Query<MemoryQueryParams>,
) -> Result<Json<PaginatedMemories>, AdminError> {
    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);

    let filter = MemoryFilter {
        agent_id: params.agent_id,
        ..Default::default()
    };

    // Fetch one extra so we can tell if there are more pages.
    let memories = engine
        .storage
        .list_memories(&filter, limit + 1, offset)
        .await?;

    let has_more = memories.len() > limit;
    let page: Vec<_> = memories.into_iter().take(limit).collect();

    let summaries: Vec<MemorySummary> = page
        .iter()
        .map(|m| {
            let preview = if m.content.len() > 100 {
                let end = m.content.char_indices()
                    .take_while(|(i, _)| *i <= 100)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(m.content.len());
                format!("{}...", &m.content[..end])
            } else {
                m.content.clone()
            };
            MemorySummary {
                id: m.id.to_string(),
                agent_id: m.agent_id.clone(),
                content_preview: preview,
                memory_type: m.memory_type.to_string(),
                scope: m.scope.to_string(),
                importance: m.importance,
                quarantined: m.quarantined,
                quarantine_reason: m.quarantine_reason.clone(),
                tags: m.tags.clone(),
                created_at: m.created_at.clone(),
                updated_at: m.updated_at.clone(),
            }
        })
        .collect();

    // We cannot know the exact total without a COUNT query, so estimate.
    let total = if has_more {
        offset + limit + 1
    } else {
        offset + page.len()
    };

    Ok(Json(PaginatedMemories {
        memories: summaries,
        total,
        limit,
        offset,
    }))
}

/// GET /admin/api/events?limit=50&offset=0 -- paginated event timeline.
pub async fn events_handler(
    State(engine): State<AppState>,
    Query(params): Query<EventQueryParams>,
) -> Result<Json<PaginatedEvents>, AdminError> {
    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);

    // The storage backend requires an agent_id for list_events, so we first
    // discover all agents, then collect events from each.
    let filter = MemoryFilter::default();
    let memories = engine.storage.list_memories(&filter, 10_000, 0).await?;

    let agent_ids: Vec<String> = memories
        .iter()
        .map(|m| m.agent_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut all_events = Vec::new();
    for aid in &agent_ids {
        let events = engine.storage.list_events(aid, 10_000, 0).await?;
        all_events.extend(events);
    }

    // Sort by timestamp descending (newest first).
    all_events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let total = all_events.len();
    let page: Vec<_> = all_events.into_iter().skip(offset).take(limit).collect();

    let summaries: Vec<EventSummary> = page
        .iter()
        .map(|e| EventSummary {
            id: e.id.to_string(),
            agent_id: e.agent_id.clone(),
            event_type: e.event_type.to_string(),
            thread_id: e.thread_id.clone(),
            timestamp: e.timestamp.clone(),
            model: e.model.clone(),
            tokens_input: e.tokens_input,
            tokens_output: e.tokens_output,
        })
        .collect();

    Ok(Json(PaginatedEvents {
        events: summaries,
        total,
        limit,
        offset,
    }))
}

/// POST /admin/api/quarantine/:id -- quarantine a memory.
pub async fn quarantine_handler(
    State(engine): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<QuarantineResponse>, AdminError> {
    let record = engine
        .storage
        .get_memory(id)
        .await?
        .ok_or_else(|| CoreError::NotFound(format!("memory {id} not found")))?;

    let mut updated = record;
    updated.quarantined = true;
    updated.quarantine_reason = Some("Quarantined by admin".to_string());
    engine.storage.update_memory(&updated).await?;

    Ok(Json(QuarantineResponse {
        id: id.to_string(),
        quarantined: true,
        message: "Memory quarantined successfully".to_string(),
    }))
}

/// POST /admin/api/unquarantine/:id -- release a memory from quarantine.
pub async fn unquarantine_handler(
    State(engine): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<QuarantineResponse>, AdminError> {
    let record = engine
        .storage
        .get_memory(id)
        .await?
        .ok_or_else(|| CoreError::NotFound(format!("memory {id} not found")))?;

    let mut updated = record;
    updated.quarantined = false;
    updated.quarantine_reason = None;
    engine.storage.update_memory(&updated).await?;

    Ok(Json(QuarantineResponse {
        id: id.to_string(),
        quarantined: false,
        message: "Memory released from quarantine".to_string(),
    }))
}

/// GET /admin/api/health -- simple health check.
pub async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "service": "mnemo-admin"}))
}
