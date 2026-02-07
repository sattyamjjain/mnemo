pub mod cold;
pub mod duckdb;
pub mod migrations;

use crate::error::Result;
use crate::model::acl::{Acl, Permission};
use crate::model::agent_profile::AgentProfile;
use crate::model::checkpoint::Checkpoint;
use crate::model::delegation::Delegation;
use crate::model::event::AgentEvent;
use crate::model::memory::MemoryRecord;
use crate::model::relation::Relation;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct MemoryFilter {
    pub agent_id: Option<String>,
    pub memory_type: Option<crate::model::memory::MemoryType>,
    pub scope: Option<crate::model::memory::Scope>,
    pub tags: Option<Vec<String>>,
    pub min_importance: Option<f32>,
    pub org_id: Option<String>,
    pub thread_id: Option<String>,
    pub include_deleted: bool,
}

#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    // Memory CRUD
    async fn insert_memory(&self, record: &MemoryRecord) -> Result<()>;
    async fn get_memory(&self, id: Uuid) -> Result<Option<MemoryRecord>>;
    async fn update_memory(&self, record: &MemoryRecord) -> Result<()>;
    async fn soft_delete_memory(&self, id: Uuid) -> Result<()>;
    async fn hard_delete_memory(&self, id: Uuid) -> Result<()>;
    async fn list_memories(&self, filter: &MemoryFilter, limit: usize, offset: usize) -> Result<Vec<MemoryRecord>>;
    async fn touch_memory(&self, id: Uuid) -> Result<()>;

    // ACL
    async fn insert_acl(&self, acl: &Acl) -> Result<()>;
    async fn check_permission(&self, memory_id: Uuid, principal_id: &str, required: Permission) -> Result<bool>;

    // Relations
    async fn insert_relation(&self, relation: &Relation) -> Result<()>;
    async fn get_relations_from(&self, source_id: Uuid) -> Result<Vec<Relation>>;
    async fn get_relations_to(&self, target_id: Uuid) -> Result<Vec<Relation>>;
    async fn delete_relation(&self, id: Uuid) -> Result<()>;

    // Chain linking
    async fn get_latest_memory_hash(&self, agent_id: &str, thread_id: Option<&str>) -> Result<Option<Vec<u8>>>;
    async fn get_latest_event_hash(&self, agent_id: &str, thread_id: Option<&str>) -> Result<Option<Vec<u8>>>;

    // Sync watermarks
    async fn get_sync_watermark(&self, key: &str) -> Result<Option<String>>;
    async fn set_sync_watermark(&self, key: &str, value: &str) -> Result<()>;

    // Permission-safe ANN
    async fn list_accessible_memory_ids(&self, agent_id: &str, limit: usize) -> Result<Vec<Uuid>>;

    // Events
    async fn insert_event(&self, event: &AgentEvent) -> Result<()>;
    async fn list_events(&self, agent_id: &str, limit: usize, offset: usize) -> Result<Vec<AgentEvent>>;
    async fn get_events_by_thread(&self, thread_id: &str, limit: usize) -> Result<Vec<AgentEvent>>;
    async fn get_event(&self, id: Uuid) -> Result<Option<AgentEvent>>;
    async fn list_child_events(&self, parent_event_id: Uuid, limit: usize) -> Result<Vec<AgentEvent>>;

    // Ordered listing for chain verification
    async fn list_memories_by_agent_ordered(&self, agent_id: &str, thread_id: Option<&str>, limit: usize) -> Result<Vec<MemoryRecord>>;

    // Sync support
    async fn list_memories_since(&self, updated_after: &str, limit: usize) -> Result<Vec<MemoryRecord>>;
    async fn upsert_memory(&self, record: &MemoryRecord) -> Result<()>;

    // Expired memory cleanup
    async fn cleanup_expired(&self) -> Result<usize>;

    // Delegations
    async fn insert_delegation(&self, d: &Delegation) -> Result<()>;
    async fn list_delegations_for(&self, delegate_id: &str) -> Result<Vec<Delegation>>;
    async fn revoke_delegation(&self, id: Uuid) -> Result<()>;
    async fn check_delegation(&self, delegate_id: &str, memory_id: Uuid, required: Permission) -> Result<bool>;

    // Agent Profiles
    async fn insert_or_update_agent_profile(&self, profile: &AgentProfile) -> Result<()>;
    async fn get_agent_profile(&self, agent_id: &str) -> Result<Option<AgentProfile>>;

    // Checkpoints
    async fn insert_checkpoint(&self, cp: &Checkpoint) -> Result<()>;
    async fn get_checkpoint(&self, id: Uuid) -> Result<Option<Checkpoint>>;
    async fn list_checkpoints(&self, thread_id: &str, branch: Option<&str>, limit: usize) -> Result<Vec<Checkpoint>>;
    async fn get_latest_checkpoint(&self, thread_id: &str, branch: &str) -> Result<Option<Checkpoint>>;
}
