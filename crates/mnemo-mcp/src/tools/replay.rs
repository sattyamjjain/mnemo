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
    /// RFC3339 timestamp. When set, synthesizes a virtual checkpoint from the
    /// memories and events that existed at that instant. Overrides `checkpoint_id`.
    pub as_of: Option<String>,
}
