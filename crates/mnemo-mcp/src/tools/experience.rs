//! Experience-memory tier — MCP tool inputs (DocTrace, arXiv:2606.10921).
//!
//! Two ops the agent calls to cache and replay successful
//! retrieval/reasoning plans: `mnemo.remember_plan` (persist a confirmed
//! good plan) and `mnemo.recall_plan` (replay the best plan for a
//! structurally-similar query). Both are inert unless the server's engine
//! was built with `MnemoEngine::with_experience_memory()`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `mnemo.remember_plan` — cache a successful plan.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RememberPlanInput {
    /// The query the plan succeeded for (its signature is derived from
    /// this).
    pub query: String,
    /// Ordered retrieval/reasoning steps to replay later.
    pub steps: Vec<String>,
    /// Ids of the chunks that led to the confirmed-good outcome.
    pub chunk_ids: Vec<String>,
    /// Confirmed outcome score in [0.0, 1.0]. Plans below the success
    /// threshold are not cached.
    pub outcome_score: f32,
    /// Visibility scope: `private` (default), `shared`, `public`, `global`.
    pub scope: Option<String>,
    /// Override the owning agent id.
    pub agent_id: Option<String>,
    /// Organization id for multi-tenant scoping.
    pub org_id: Option<String>,
}

/// `mnemo.recall_plan` — replay the best plan for a new query.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecallPlanInput {
    /// The new query to look up a replayable plan for.
    pub query: String,
    /// Override the replay similarity threshold (default 0.7).
    pub similarity_threshold: Option<f32>,
    /// Override the requesting agent id (RBAC: only visible plans match).
    pub agent_id: Option<String>,
    /// Organization id for multi-tenant scoping.
    pub org_id: Option<String>,
}
