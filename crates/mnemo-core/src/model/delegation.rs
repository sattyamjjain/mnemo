use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::acl::Permission;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Delegation {
    pub id: Uuid,
    pub delegator_id: String,
    pub delegate_id: String,
    pub permission: Permission,
    pub scope: DelegationScope,
    pub max_depth: u32,
    pub current_depth: u32,
    pub parent_delegation_id: Option<Uuid>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DelegationScope {
    AllMemories,
    ByTag(Vec<String>),
    ByMemoryId(Vec<Uuid>),
}

impl std::fmt::Display for DelegationScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DelegationScope::AllMemories => write!(f, "all_memories"),
            DelegationScope::ByTag(_) => write!(f, "by_tag"),
            DelegationScope::ByMemoryId(_) => write!(f, "by_memory_id"),
        }
    }
}
