use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MergeInput {
    /// The thread ID containing the branches to merge.
    pub thread_id: String,
    /// The branch to merge from.
    pub source_branch: String,
    /// The branch to merge into. Defaults to "main".
    pub target_branch: Option<String>,
    /// Merge strategy: "full_merge" (all memories), "cherry_pick" (specific memories), or "squash". Defaults to "full_merge".
    pub strategy: Option<String>,
    /// Memory IDs to cherry-pick (only used with "cherry_pick" strategy).
    pub cherry_pick_ids: Option<Vec<String>>,
}
