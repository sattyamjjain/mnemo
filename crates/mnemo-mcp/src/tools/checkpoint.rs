use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckpointInput {
    /// The thread ID to create a checkpoint for.
    pub thread_id: String,
    /// The branch name. Defaults to "main".
    pub branch_name: Option<String>,
    /// A JSON snapshot of the current agent state.
    pub state_snapshot: serde_json::Value,
    /// An optional label for this checkpoint.
    pub label: Option<String>,
    /// Optional metadata as key-value pairs.
    pub metadata: Option<serde_json::Value>,
}
