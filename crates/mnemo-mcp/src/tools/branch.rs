use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BranchInput {
    /// The thread ID to branch from.
    pub thread_id: String,
    /// The name for the new branch.
    pub new_branch_name: String,
    /// The checkpoint ID to branch from. If not specified, uses the latest checkpoint on the source branch.
    pub source_checkpoint_id: Option<String>,
    /// The source branch to branch from. Defaults to "main".
    pub source_branch: Option<String>,
}
