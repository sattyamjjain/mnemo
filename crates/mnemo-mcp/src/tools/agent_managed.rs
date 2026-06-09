//! Agent-controlled memory mode — MCP tool inputs (AutoMEM,
//! arXiv:2606.04315).
//!
//! AutoMEM's finding: on long-horizon, multi-session workloads an agent
//! that **manages its own memory** over a simple flat store — deciding
//! what to write, revising stale entries, and forgetting — can beat a
//! fixed ingestion+retrieval pipeline, because the *agent* (not an
//! ingestion heuristic) controls what persists. The pipeline still wins
//! single-shot retrieval, so this is an additive mode, not a
//! replacement.
//!
//! These four tools (`mnemo.mem_write` / `mem_read` / `mem_revise` /
//! `mem_forget`) are the agent-facing surface. They are **thin
//! compositions over the verified engine primitives** `remember` /
//! `recall` / `forget` (no new engine enum or method): every
//! agent-managed entry carries the reserved [`AGENT_MANAGED_TAG`] so the
//! flat store is a self-contained, agent-curated subset of the same
//! backend. The default `mnemo.recall` pipeline is untouched and remains
//! the fallback for single-shot queries.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Reserved tag stamped on every agent-managed entry. `mem_read` filters
/// on it so the agent reads back only its own curated flat store, and
/// the eval uses it to separate the agent-managed corpus from a
/// fixed-pipeline ingest of the same facts.
pub const AGENT_MANAGED_TAG: &str = "agent-managed";

/// `mnemo.mem_write` — the agent appends an entry it judged worth
/// keeping. Maps to `engine.remember` with [`AGENT_MANAGED_TAG`] added.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemWriteInput {
    /// The verbatim content the agent decided to persist.
    pub content: String,
    /// Additional caller tags (the reserved agent-managed tag is always
    /// added on top).
    pub tags: Option<Vec<String>>,
    /// Importance in [0.0, 1.0]. Defaults to the engine default.
    pub importance: Option<f32>,
    /// `episodic` | `semantic` | `procedural` | `working`.
    pub memory_type: Option<String>,
    /// Free-form metadata stored verbatim alongside the entry.
    pub metadata: Option<serde_json::Value>,
    /// Override the writing agent id (defaults to the server's agent).
    pub agent_id: Option<String>,
    /// Organization id for multi-tenant scoping.
    pub org_id: Option<String>,
}

/// `mnemo.mem_read` — the agent reads back its own flat store. Maps to
/// `engine.recall` filtered to [`AGENT_MANAGED_TAG`], so it never
/// searches the whole backend (that is what `mnemo.recall` is for).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemReadInput {
    /// Query string. Use a broad term to list the store, a specific term
    /// to read a topic.
    pub query: String,
    /// Max entries to return (defaults to 10).
    pub limit: Option<usize>,
    /// Restrict to entries also carrying these caller tags (the reserved
    /// agent-managed tag is always required on top).
    pub tags: Option<Vec<String>>,
    /// Override the reading agent id.
    pub agent_id: Option<String>,
    /// Organization id for multi-tenant scoping.
    pub org_id: Option<String>,
}

/// `mnemo.mem_revise` — the agent supersedes a stale entry with a
/// corrected one. Composed from existing primitives: soft-`forget` the
/// old id, then `remember` the new content (tagged agent-managed, with
/// `metadata.revises = <old_id>`). The newest write wins on subsequent
/// reads — no new engine op, no hash-chain surgery.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemReviseInput {
    /// Id of the entry being revised (it is soft-deleted).
    pub id: String,
    /// The corrected content to persist in its place.
    pub content: String,
    /// Caller tags for the revised entry (agent-managed tag always added).
    pub tags: Option<Vec<String>>,
    /// Importance for the revised entry.
    pub importance: Option<f32>,
    /// Override the revising agent id.
    pub agent_id: Option<String>,
    /// Organization id for multi-tenant scoping.
    pub org_id: Option<String>,
}

/// `mnemo.mem_forget` — the agent drops an entry it no longer wants.
/// Maps to `engine.forget` (soft by default, hard on request).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemForgetInput {
    /// Id of the entry to forget.
    pub id: String,
    /// When `true`, hard-delete (permanent) instead of soft-delete.
    pub hard: Option<bool>,
    /// Override the forgetting agent id.
    pub agent_id: Option<String>,
}
