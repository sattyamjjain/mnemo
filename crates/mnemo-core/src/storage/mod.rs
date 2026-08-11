pub mod cold;
pub mod duckdb;
pub mod migrations;

use crate::error::Result;
use crate::model::acl::{Acl, Permission};
use crate::model::agent_profile::AgentProfile;
use crate::model::checkpoint::Checkpoint;
use crate::model::delegation::Delegation;
use crate::model::embedding_baseline::EmbeddingBaseline;
use crate::model::event::AgentEvent;
use crate::model::memory::MemoryRecord;
use crate::model::relation::Relation;
use crate::model::write_provenance::WriteProvenance;
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
    async fn list_memories(
        &self,
        filter: &MemoryFilter,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<MemoryRecord>>;
    async fn touch_memory(&self, id: Uuid) -> Result<()>;

    // ACL
    async fn insert_acl(&self, acl: &Acl) -> Result<()>;
    async fn check_permission(
        &self,
        memory_id: Uuid,
        principal_id: &str,
        required: Permission,
    ) -> Result<bool>;

    // Relations
    async fn insert_relation(&self, relation: &Relation) -> Result<()>;
    async fn get_relations_from(&self, source_id: Uuid) -> Result<Vec<Relation>>;
    async fn get_relations_to(&self, target_id: Uuid) -> Result<Vec<Relation>>;
    async fn delete_relation(&self, id: Uuid) -> Result<()>;

    // Chain linking
    async fn get_latest_memory_hash(
        &self,
        agent_id: &str,
        thread_id: Option<&str>,
    ) -> Result<Option<Vec<u8>>>;
    async fn get_latest_event_hash(
        &self,
        agent_id: &str,
        thread_id: Option<&str>,
    ) -> Result<Option<Vec<u8>>>;

    // Sync watermarks
    async fn get_sync_watermark(&self, key: &str) -> Result<Option<String>>;
    async fn set_sync_watermark(&self, key: &str, value: &str) -> Result<()>;

    // Permission-safe ANN
    async fn list_accessible_memory_ids(&self, agent_id: &str, limit: usize) -> Result<Vec<Uuid>>;

    // Events
    async fn insert_event(&self, event: &AgentEvent) -> Result<()>;
    async fn list_events(
        &self,
        agent_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<AgentEvent>>;
    async fn get_events_by_thread(&self, thread_id: &str, limit: usize) -> Result<Vec<AgentEvent>>;
    async fn get_event(&self, id: Uuid) -> Result<Option<AgentEvent>>;
    async fn list_child_events(
        &self,
        parent_event_id: Uuid,
        limit: usize,
    ) -> Result<Vec<AgentEvent>>;

    // Ordered listing for chain verification
    async fn list_memories_by_agent_ordered(
        &self,
        agent_id: &str,
        thread_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>>;

    // Sync support
    async fn list_memories_since(
        &self,
        updated_after: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>>;
    async fn upsert_memory(&self, record: &MemoryRecord) -> Result<()>;

    // Expired memory cleanup
    async fn cleanup_expired(&self) -> Result<usize>;

    // Delegations
    async fn insert_delegation(&self, d: &Delegation) -> Result<()>;
    async fn list_delegations_for(&self, delegate_id: &str) -> Result<Vec<Delegation>>;
    async fn revoke_delegation(&self, id: Uuid) -> Result<()>;
    async fn check_delegation(
        &self,
        delegate_id: &str,
        memory_id: Uuid,
        required: Permission,
    ) -> Result<bool>;

    // Agent Profiles
    async fn insert_or_update_agent_profile(&self, profile: &AgentProfile) -> Result<()>;
    async fn get_agent_profile(&self, agent_id: &str) -> Result<Option<AgentProfile>>;

    // Embedding baselines (v0.3.3, z-score outlier detector)
    async fn insert_or_update_embedding_baseline(&self, baseline: &EmbeddingBaseline)
    -> Result<()>;
    async fn get_embedding_baseline(&self, agent_id: &str) -> Result<Option<EmbeddingBaseline>>;

    // Checkpoints
    async fn insert_checkpoint(&self, cp: &Checkpoint) -> Result<()>;
    async fn get_checkpoint(&self, id: Uuid) -> Result<Option<Checkpoint>>;
    async fn list_checkpoints(
        &self,
        thread_id: &str,
        branch: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Checkpoint>>;
    async fn get_latest_checkpoint(
        &self,
        thread_id: &str,
        branch: &str,
    ) -> Result<Option<Checkpoint>>;

    // --- Write provenance (who wrote each memory, under what authority) -------
    // A tamper-evident, hash-chained record per memory write. Default impls are
    // graceful (no-op / empty) so a backend opts in by overriding them —
    // provenance is recorded only where a backend implements it. DuckDB
    // implements the full set; PostgreSQL support is tracked alongside.

    /// Append a tamper-evident write-provenance record. Default: no-op.
    async fn insert_write_provenance(&self, _prov: &WriteProvenance) -> Result<()> {
        Ok(())
    }
    /// The provenance for one memory id, if recorded. Default: `None`.
    async fn get_write_provenance(&self, _memory_id: Uuid) -> Result<Option<WriteProvenance>> {
        Ok(None)
    }
    /// `content_hash` of the most recent provenance record (the chain head), or
    /// `None` when the chain is empty. Used to link the next record. Default: `None`.
    async fn get_latest_provenance_hash(&self) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
    /// All provenance written by `principal`, newest first. Default: empty.
    async fn list_provenance_by_principal(
        &self,
        _principal: &str,
        _limit: usize,
    ) -> Result<Vec<WriteProvenance>> {
        Ok(Vec::new())
    }
    /// All provenance written under `session_id`, newest first. Default: empty.
    async fn list_provenance_by_session(
        &self,
        _session_id: &str,
        _limit: usize,
    ) -> Result<Vec<WriteProvenance>> {
        Ok(Vec::new())
    }
    /// Memory ids written by `principal` — the target set for FORGET BY
    /// PROVENANCE. Default: empty.
    async fn list_memory_ids_by_principal(&self, _principal: &str) -> Result<Vec<Uuid>> {
        Ok(Vec::new())
    }
    /// Memory ids written under `session_id`. Default: empty.
    async fn list_memory_ids_by_session(&self, _session_id: &str) -> Result<Vec<Uuid>> {
        Ok(Vec::new())
    }
    /// The whole provenance chain, oldest-first (append order), for
    /// tamper-evidence verification. Default: empty.
    async fn list_all_provenance(&self, _limit: usize) -> Result<Vec<WriteProvenance>> {
        Ok(Vec::new())
    }

    /// Whether this backend records write provenance (overridden to `true` by
    /// backends that implement the methods above). Lets the write path skip the
    /// provenance write on a backend that would no-op it.
    fn records_write_provenance(&self) -> bool {
        false
    }

    /// Short, stable label for the backend implementation (e.g. `"duckdb"`,
    /// `"postgres"`). Used in diagnostics such as
    /// [`crate::error::Error::EmbedderNotConfigured`] so an error names the
    /// backend in play. Defaults to `"unknown"`; real backends override it.
    fn backend_name(&self) -> &'static str {
        "unknown"
    }

    /// Whether this backend guarantees the `agent_events` log is **append-only**
    /// — no code path (and, where enforceable, no schema path) can delete or
    /// rewrite an event row. Both shipped backends guarantee this: DuckDB has no
    /// `DELETE`/`UPDATE` on `agent_events`, and PostgreSQL additionally enforces
    /// it with a `prevent_event_modification` trigger. A retention-conformance
    /// profile relies on this to promise a retention floor; a backend that
    /// cannot honour it should override this to `false` so the profile fails
    /// loud (see `mnemo-compliance`'s `RetentionProfile`). Defaults to `true`.
    fn events_are_append_only(&self) -> bool {
        true
    }
}
