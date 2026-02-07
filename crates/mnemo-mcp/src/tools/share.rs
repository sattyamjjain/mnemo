use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShareInput {
    /// The ID of the memory to share.
    pub memory_id: String,
    /// Share multiple memories at once. Takes precedence over memory_id if set.
    pub memory_ids: Option<Vec<String>>,
    /// The agent ID to share the memory with.
    pub target_agent_id: String,
    /// Share with multiple agents at once. Takes precedence over target_agent_id if set.
    pub target_agent_ids: Option<Vec<String>>,
    /// Permission level to grant: "read" (view only), "write" (can modify), or "admin" (full control). Defaults to "read".
    pub permission: Option<String>,
    /// Number of hours until the share expires. If not set, the share does not expire.
    pub expires_in_hours: Option<f64>,
}
