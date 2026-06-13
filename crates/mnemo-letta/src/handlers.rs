//! Axum handlers wiring Letta-protocol shapes to `MnemoEngine`
//! (v0.4.0-rc3 Task B5).
//!
//! These handlers are deliberately thin — they translate field names
//! between the Letta wire shape and our `Remember`/`Recall` requests,
//! never adding business logic. Anything that should affect every
//! Letta-protocol caller (rate limits, auth) belongs in a tower
//! middleware layered on top of [`crate::router`], not inside the
//! handlers.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mnemo_core::model::memory::MemoryType;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::MemoryFilter;

use crate::model::{
    CreateAgentRequest, CreateAgentResponse, GetMemoryResponse, MemoryBlock, MessageFrame,
    SendMessageRequest, SendMessageResponse,
};

/// `POST /v1/agents`. Persists the persona/human blocks (when given)
/// as Semantic memories tagged `letta-block:persona` /
/// `letta-block:human`, and returns the agent id the caller can use
/// for subsequent calls.
pub async fn create_agent(
    State(engine): State<Arc<MnemoEngine>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<CreateAgentResponse>, LettaError> {
    if req.name.trim().is_empty() {
        return Err(LettaError::BadRequest("agent name is required".into()));
    }
    let agent_id = req.name.clone();

    if let Some(persona) = req.persona.as_ref().filter(|p| !p.is_empty()) {
        store_block(&engine, &agent_id, "persona", persona).await?;
    }
    if let Some(human) = req.human.as_ref().filter(|h| !h.is_empty()) {
        store_block(&engine, &agent_id, "human", human).await?;
    }

    Ok(Json(CreateAgentResponse {
        agent_id,
        name: req.name,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// `POST /v1/agents/{agent_id}/messages`. Persists the user message
/// as Episodic memory and runs a recall to surface a contextual
/// reply. The "reply" is intentionally a deterministic summary of
/// recalled memories — wiring this to a real LLM is the caller's
/// responsibility (the Letta SDK does this on the client side; our
/// compat layer just handles state).
pub async fn send_message(
    State(engine): State<Arc<MnemoEngine>>,
    Path(agent_id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, LettaError> {
    if req.content.trim().is_empty() {
        return Err(LettaError::BadRequest("message content is required".into()));
    }

    // Persist the user turn.
    let mut remember = RememberRequest::new(req.content.clone());
    remember.agent_id = Some(agent_id.clone());
    remember.memory_type = Some(MemoryType::Episodic);
    remember.tags = Some(vec!["letta-message".into(), format!("role:{}", req.role)]);
    engine
        .remember(remember)
        .await
        .map_err(|e| LettaError::Engine(e.to_string()))?;

    // Surface up to 5 related memories — that's the size Letta-Code
    // benchmarks expect in the assistant frame.
    let recall = RecallRequest {
        query: req.content.clone(),
        agent_id: Some(agent_id.clone()),
        limit: Some(5),
        memory_type: None,
        memory_types: None,
        scope: None,
        min_importance: None,
        tags: None,
        org_id: None,
        strategy: Some("hybrid".to_string()),
        temporal_range: None,
        recency_half_life_hours: None,
        hybrid_weights: None,
        rrf_k: None,
        as_of: None,
        explain: None,
        with_provenance: None,
        mode: None,
        current_fact_resolver: None,
        orientation_cache: None,
        evidence_budget: None,
        retained_token_budget: None,
        domain_scope: None,
    };
    let resp = engine
        .recall(recall)
        .await
        .map_err(|e| LettaError::Engine(e.to_string()))?;

    let summary = if resp.memories.is_empty() {
        "No prior memories — starting fresh.".to_string()
    } else {
        let lines: Vec<String> = resp
            .memories
            .iter()
            .take(5)
            .map(|m| format!("- {}", m.content))
            .collect();
        format!("Recalled {}:\n{}", resp.memories.len(), lines.join("\n"))
    };

    Ok(Json(SendMessageResponse {
        agent_id,
        messages: vec![MessageFrame {
            role: "assistant".to_string(),
            content: summary,
            created_at: chrono::Utc::now().to_rfc3339(),
        }],
    }))
}

/// `GET /v1/agents/{agent_id}/memory`. Returns the persona + human
/// core-memory blocks as Letta exposes them to its agent loop.
pub async fn get_memory(
    State(engine): State<Arc<MnemoEngine>>,
    Path(agent_id): Path<String>,
) -> Result<Json<GetMemoryResponse>, LettaError> {
    let mut blocks = Vec::new();
    for label in ["persona", "human"] {
        if let Some(value) = load_block(&engine, &agent_id, label).await? {
            blocks.push(MemoryBlock {
                label: label.to_string(),
                value,
                limit: Some(2000),
            });
        }
    }
    Ok(Json(GetMemoryResponse {
        agent_id,
        memory: blocks,
    }))
}

async fn store_block(
    engine: &MnemoEngine,
    agent_id: &str,
    label: &str,
    value: &str,
) -> Result<(), LettaError> {
    let mut req = RememberRequest::new(value.to_string());
    req.agent_id = Some(agent_id.to_string());
    req.memory_type = Some(MemoryType::Semantic);
    req.tags = Some(vec![format!("letta-block:{label}")]);
    req.importance = Some(1.0);
    engine
        .remember(req)
        .await
        .map_err(|e| LettaError::Engine(e.to_string()))?;
    Ok(())
}

async fn load_block(
    engine: &MnemoEngine,
    agent_id: &str,
    label: &str,
) -> Result<Option<String>, LettaError> {
    // Latest record wins — Letta semantics treat blocks as
    // overwritable strings, so a later store_block supersedes earlier
    // values.
    let filter = MemoryFilter {
        agent_id: Some(agent_id.to_string()),
        tags: Some(vec![format!("letta-block:{label}")]),
        ..Default::default()
    };
    let records = engine
        .storage
        .list_memories(&filter, 1, 0)
        .await
        .map_err(|e| LettaError::Engine(e.to_string()))?;
    Ok(records.into_iter().next().map(|r| r.content))
}

/// Errors surfaced to Letta clients. Mapped to HTTP via `IntoResponse`.
#[derive(Debug, thiserror::Error)]
pub enum LettaError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("engine error: {0}")]
    Engine(String),
}

impl IntoResponse for LettaError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            LettaError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            LettaError::Engine(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(serde_json::json!({"error": msg}))).into_response()
    }
}
