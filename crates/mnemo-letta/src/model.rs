//! Wire shapes for the Letta-compat surface (v0.4.0-rc3, B5).
//!
//! All structs deliberately tolerate unknown fields — Letta has been
//! evolving this protocol every few weeks and we'd rather drop
//! unfamiliar metadata silently than 400-out an integration that
//! happens to send a new key.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Letta `POST /v1/agents` request body.
///
/// `name` is the only field the Letta upstream contract treats as
/// mandatory. `persona` and `human` come straight from the Letta
/// "core memory" concept — short blocks the agent's prompt always
/// sees. We treat them as optional Semantic memories.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub persona: Option<String>,
    #[serde(default)]
    pub human: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateAgentResponse {
    pub agent_id: String,
    pub name: String,
    pub created_at: String,
}

/// Letta `POST /v1/agents/{agent_id}/messages` request body.
#[derive(Debug, Clone, Deserialize)]
pub struct SendMessageRequest {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One message frame in the response. Letta returns an array so the
/// model's tool-call sequence is preserved; in our compat layer we
/// always return a single assistant frame summarising recalled memory.
#[derive(Debug, Clone, Serialize)]
pub struct MessageFrame {
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendMessageResponse {
    pub agent_id: String,
    pub messages: Vec<MessageFrame>,
}

/// One block of Letta core memory.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryBlock {
    pub label: String,
    pub value: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetMemoryResponse {
    pub agent_id: String,
    pub memory: Vec<MemoryBlock>,
}
