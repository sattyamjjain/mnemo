use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DelegateInput {
    /// Agent ID to delegate permissions to.
    pub delegate_id: String,
    /// Permission to delegate: "read", "write", "delete", "share", "delegate", or "admin".
    pub permission: String,
    /// Specific memory IDs to scope the delegation to. If empty, delegates for all memories or by tags.
    pub memory_ids: Option<Vec<String>>,
    /// Tags to scope the delegation to. If both memory_ids and tags are empty, delegates for all memories.
    pub tags: Option<Vec<String>>,
    /// Maximum re-delegation depth. 0 means the delegate cannot further delegate.
    pub max_depth: Option<u32>,
    /// Hours until this delegation expires. If not set, delegation is permanent until revoked.
    pub expires_in_hours: Option<f64>,
}
