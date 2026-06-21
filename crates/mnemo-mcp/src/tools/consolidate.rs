use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConsolidateInput {
    /// The member memory IDs (UUID strings) to collect as evidence into the
    /// topic document. Must be non-empty; duplicates are ignored.
    pub memory_ids: Vec<String>,
    /// The topic document's name / fact key. Reuse the same name across
    /// revisions of the same fact so the current-fact resolver can collapse
    /// to the current view.
    pub topic_name: String,
    /// Agent that owns the topic document. Defaults to the server's agent.
    pub agent_id: Option<String>,
    /// Optional document body. When omitted, the body is synthesised
    /// deterministically from the member contents.
    pub summary: Option<String>,
    /// Optional UUID of an existing topic document this one revises. When set,
    /// the new document becomes the current version and the old one is
    /// retained, marked superseded, in history.
    pub supersede: Option<String>,
    /// Optional thread/session scope.
    pub thread_id: Option<String>,
    /// Optional caller metadata, merged with the provenance keys.
    pub metadata: Option<serde_json::Value>,
}
