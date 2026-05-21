//! v0.4.5 — Inputs for the two new MCP tools `mnemo.attention_state.put`
//! and `mnemo.attention_state.get`, anchored on arXiv:2605.18226
//! (Context Memorization). See `crates/mnemo-attention-state` for the
//! storage substrate.
//!
//! The tools are registered on [`crate::server::MnemoServer`] only when
//! the server is constructed via
//! [`MnemoServer::with_attention_state`][crate::server::MnemoServer::with_attention_state].
//! Calls in the unconfigured case return a spec-shaped error result, not
//! a panic.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AttentionStatePutInput {
    /// Owning agent. Matches mnemo's standard `agent_id` scoping —
    /// the store keys by `(agent_id, prefix_hash)`.
    pub agent_id: String,
    /// Caller-chosen prefix identity (convention: hex-encoded SHA-256
    /// of the producer's prompt tokens; the store treats it as an
    /// opaque key).
    pub prefix_hash: String,
    /// Opaque attention-state blob. Hex-encoded so the JSON-RPC wire
    /// stays string-safe. The producer's bytes are recovered via
    /// `hex::decode` on the server side.
    pub state_blob_hex: String,
    /// Optional producer model identifier (e.g. `"claude-sonnet-4.6@bf16-tp1"`).
    /// Stored as record metadata so a future consumer can refuse a
    /// state blob produced under incompatible quantization.
    pub model: Option<String>,
    /// Optional producer TTL in seconds. The in-memory store does NOT
    /// enforce expiry; the operator is responsible at the engine /
    /// tool layer.
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AttentionStateGetInput {
    pub agent_id: String,
    pub prefix_hash: String,
}
