use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VerifyInput {
    /// Agent ID to verify chain integrity for. Uses default if not specified.
    pub agent_id: Option<String>,
    /// Optional thread ID to limit verification to a specific thread.
    pub thread_id: Option<String>,
}
