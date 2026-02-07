use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReplayInput {
    /// The thread ID to replay.
    pub thread_id: String,
    /// Specific checkpoint ID to replay. If not specified, uses the latest checkpoint on the branch.
    pub checkpoint_id: Option<String>,
    /// The branch name. Defaults to "main".
    pub branch_name: Option<String>,
}
