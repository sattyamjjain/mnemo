use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::{Error, Result};
use crate::model::acl::{Acl, Permission};
use crate::model::agent_profile::AgentProfile;
use crate::model::checkpoint::Checkpoint;
use crate::model::delegation::{Delegation, DelegationScope};
use crate::model::event::AgentEvent;
use crate::model::memory::MemoryRecord;
use crate::model::relation::Relation;
use crate::storage::{MemoryFilter, StorageBackend};
use uuid::Uuid;

pub struct DuckDbStorage {
    conn: Arc<Mutex<duckdb::Connection>>,
}

impl DuckDbStorage {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = duckdb::Connection::open(path)?;
        super::migrations::run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = duckdb::Connection::open_in_memory()?;
        super::migrations::run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

fn serialize_embedding(embedding: &Option<Vec<f32>>) -> Option<Vec<u8>> {
    embedding.as_ref().map(|v| {
        v.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect()
    })
}

fn deserialize_embedding(blob: Option<Vec<u8>>) -> Option<Vec<f32>> {
    blob.map(|bytes| {
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    })
}

fn row_to_memory(row: &duckdb::Row<'_>) -> duckdb::Result<MemoryRecord> {
    let id_str: String = row.get(0)?;
    let tags_json: Option<String> = row.get(6)?;
    let metadata_json: Option<String> = row.get(7)?;
    let embedding_blob: Option<Vec<u8>> = row.get(8)?;
    let content_hash: Vec<u8> = row.get(9)?;
    let prev_hash: Option<Vec<u8>> = row.get(10)?;

    Ok(MemoryRecord {
        id: Uuid::parse_str(&id_str).unwrap(),
        agent_id: row.get(1)?,
        content: row.get(2)?,
        memory_type: row.get::<_, String>(3)?.parse().unwrap(),
        scope: row.get::<_, String>(4)?.parse().unwrap(),
        importance: row.get(5)?,
        tags: tags_json
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        metadata: metadata_json
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        embedding: deserialize_embedding(embedding_blob),
        content_hash,
        prev_hash,
        source_type: row.get::<_, String>(11)?.parse().unwrap(),
        source_id: row.get(12)?,
        consolidation_state: row.get::<_, String>(13)?.parse().unwrap(),
        access_count: row.get::<_, i64>(14)? as u64,
        org_id: row.get(15)?,
        thread_id: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
        last_accessed_at: row.get(19)?,
        expires_at: row.get(20)?,
        deleted_at: row.get(21)?,
        decay_rate: row.get(22)?,
        created_by: row.get(23)?,
        version: row.get::<_, i32>(24)? as u32,
        prev_version_id: row.get::<_, Option<String>>(25)?
            .and_then(|s| Uuid::parse_str(&s).ok()),
        quarantined: row.get::<_, bool>(26)?,
        quarantine_reason: row.get(27)?,
        decay_function: row.get(28).unwrap_or(None),
    })
}

#[async_trait::async_trait]
impl StorageBackend for DuckDbStorage {
    async fn insert_memory(&self, record: &MemoryRecord) -> Result<()> {
        let conn = self.conn.lock().await;
        let tags_json = serde_json::to_string(&record.tags)?;
        let metadata_json = serde_json::to_string(&record.metadata)?;
        let embedding_blob = serialize_embedding(&record.embedding);

        conn.execute(
            "INSERT INTO memories (id, agent_id, content, memory_type, scope, importance, tags, metadata, embedding, content_hash, prev_hash, source_type, source_id, consolidation_state, access_count, org_id, thread_id, created_at, updated_at, last_accessed_at, expires_at, deleted_at, decay_rate, created_by, version, prev_version_id, quarantined, quarantine_reason, decay_function) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                record.id.to_string(),
                record.agent_id,
                record.content,
                record.memory_type.to_string(),
                record.scope.to_string(),
                record.importance,
                tags_json,
                metadata_json,
                embedding_blob,
                record.content_hash,
                record.prev_hash,
                record.source_type.to_string(),
                record.source_id,
                record.consolidation_state.to_string(),
                record.access_count as i64,
                record.org_id,
                record.thread_id,
                record.created_at,
                record.updated_at,
                record.last_accessed_at,
                record.expires_at,
                record.deleted_at,
                record.decay_rate,
                record.created_by,
                record.version as i32,
                record.prev_version_id.map(|id| id.to_string()),
                record.quarantined,
                record.quarantine_reason,
                record.decay_function,
            ],
        )?;
        Ok(())
    }

    async fn get_memory(&self, id: Uuid) -> Result<Option<MemoryRecord>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, content, memory_type, scope, importance, tags, metadata, embedding, content_hash, prev_hash, source_type, source_id, consolidation_state, access_count, org_id, thread_id, created_at, updated_at, last_accessed_at, expires_at, deleted_at, decay_rate, created_by, version, prev_version_id, quarantined, quarantine_reason, decay_function FROM memories WHERE id = ?",
        )?;
        let result = stmt.query_row([id.to_string()], row_to_memory);
        match result {
            Ok(record) => Ok(Some(record)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }

    async fn update_memory(&self, record: &MemoryRecord) -> Result<()> {
        let conn = self.conn.lock().await;
        let tags_json = serde_json::to_string(&record.tags)?;
        let metadata_json = serde_json::to_string(&record.metadata)?;
        let embedding_blob = serialize_embedding(&record.embedding);

        let affected = conn.execute(
            "UPDATE memories SET agent_id=?, content=?, memory_type=?, scope=?, importance=?, tags=?, metadata=?, embedding=?, content_hash=?, prev_hash=?, source_type=?, source_id=?, consolidation_state=?, access_count=?, org_id=?, thread_id=?, updated_at=?, last_accessed_at=?, expires_at=?, deleted_at=?, decay_rate=?, created_by=?, version=?, prev_version_id=?, quarantined=?, quarantine_reason=?, decay_function=? WHERE id=?",
            duckdb::params![
                record.agent_id,
                record.content,
                record.memory_type.to_string(),
                record.scope.to_string(),
                record.importance,
                tags_json,
                metadata_json,
                embedding_blob,
                record.content_hash,
                record.prev_hash,
                record.source_type.to_string(),
                record.source_id,
                record.consolidation_state.to_string(),
                record.access_count as i64,
                record.org_id,
                record.thread_id,
                record.updated_at,
                record.last_accessed_at,
                record.expires_at,
                record.deleted_at,
                record.decay_rate,
                record.created_by,
                record.version as i32,
                record.prev_version_id.map(|id| id.to_string()),
                record.quarantined,
                record.quarantine_reason,
                record.decay_function,
                record.id.to_string(),
            ],
        )?;
        if affected == 0 {
            return Err(Error::NotFound(format!("memory {} not found", record.id)));
        }
        Ok(())
    }

    async fn soft_delete_memory(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        let affected = conn.execute(
            "UPDATE memories SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
            duckdb::params![now, now, id.to_string()],
        )?;
        if affected == 0 {
            return Err(Error::NotFound(format!("memory {id} not found or already deleted")));
        }
        Ok(())
    }

    async fn hard_delete_memory(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().await;
        let affected = conn.execute(
            "DELETE FROM memories WHERE id = ?",
            duckdb::params![id.to_string()],
        )?;
        if affected == 0 {
            return Err(Error::NotFound(format!("memory {id} not found")));
        }
        // Also clean up ACLs
        conn.execute(
            "DELETE FROM acls WHERE memory_id = ?",
            duckdb::params![id.to_string()],
        )?;
        Ok(())
    }

    async fn list_memories(&self, filter: &MemoryFilter, limit: usize, offset: usize) -> Result<Vec<MemoryRecord>> {
        let conn = self.conn.lock().await;
        let mut conditions = Vec::new();
        let mut params: Vec<Box<dyn duckdb::ToSql>> = Vec::new();

        if !filter.include_deleted {
            conditions.push("deleted_at IS NULL".to_string());
        }

        if let Some(ref agent_id) = filter.agent_id {
            conditions.push(format!("agent_id = ${}", params.len() + 1));
            params.push(Box::new(agent_id.clone()));
        }

        if let Some(memory_type) = filter.memory_type {
            conditions.push(format!("memory_type = ${}", params.len() + 1));
            params.push(Box::new(memory_type.to_string()));
        }

        if let Some(scope) = filter.scope {
            conditions.push(format!("scope = ${}", params.len() + 1));
            params.push(Box::new(scope.to_string()));
        }

        if let Some(min_importance) = filter.min_importance {
            conditions.push(format!("importance >= ${}", params.len() + 1));
            params.push(Box::new(min_importance));
        }

        if let Some(ref org_id) = filter.org_id {
            conditions.push(format!("org_id = ${}", params.len() + 1));
            params.push(Box::new(org_id.clone()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        if let Some(ref thread_id) = filter.thread_id {
            conditions.push(format!("thread_id = ${}", params.len() + 1));
            params.push(Box::new(thread_id.clone()));
        }

        let sql = format!(
            "SELECT id, agent_id, content, memory_type, scope, importance, tags, metadata, embedding, content_hash, prev_hash, source_type, source_id, consolidation_state, access_count, org_id, thread_id, created_at, updated_at, last_accessed_at, expires_at, deleted_at, decay_rate, created_by, version, prev_version_id, quarantined, quarantine_reason, decay_function FROM memories {where_clause} ORDER BY created_at DESC LIMIT {limit} OFFSET {offset}"
        );

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), row_to_memory)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
        }
        Ok(results)
    }

    async fn touch_memory(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE memories SET access_count = access_count + 1, last_accessed_at = ? WHERE id = ?",
            duckdb::params![now, id.to_string()],
        )?;
        Ok(())
    }

    async fn insert_acl(&self, acl: &Acl) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO acls (id, memory_id, principal_type, principal_id, permission, granted_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                acl.id.to_string(),
                acl.memory_id.to_string(),
                acl.principal_type.to_string(),
                acl.principal_id,
                acl.permission.to_string(),
                acl.granted_by,
                acl.created_at,
                acl.expires_at,
            ],
        )?;
        Ok(())
    }

    async fn check_permission(&self, memory_id: Uuid, principal_id: &str, required: Permission) -> Result<bool> {
        // Do all DuckDB work in one block, then release the lock before delegation check
        let acl_result = {
            let conn = self.conn.lock().await;

            // Check if the principal is the owner (agent_id matches)
            let mut stmt = conn.prepare(
                "SELECT agent_id FROM memories WHERE id = ?",
            )?;
            let owner_result = stmt.query_row([memory_id.to_string()], |row| {
                row.get::<_, String>(0)
            });
            match owner_result {
                Ok(owner) if owner == principal_id => return Ok(true),
                Err(duckdb::Error::QueryReturnedNoRows) => {
                    return Err(Error::NotFound(format!("memory {memory_id} not found")));
                }
                _ => {}
            }

            // Check ACLs
            let now = chrono::Utc::now().to_rfc3339();
            let mut stmt = conn.prepare(
                "SELECT permission FROM acls WHERE memory_id = ? AND principal_id = ? AND (expires_at IS NULL OR expires_at > ?)",
            )?;
            let rows = stmt.query_map(
                duckdb::params![memory_id.to_string(), principal_id, now.clone()],
                |row| row.get::<_, String>(0),
            )?;

            let mut perms: Vec<String> = Vec::new();
            for row in rows {
                perms.push(row.map_err(|e| Error::Storage(e.to_string()))?);
            }

            // Check public ACLs
            let mut stmt = conn.prepare(
                "SELECT permission FROM acls WHERE memory_id = ? AND principal_type = 'public' AND (expires_at IS NULL OR expires_at > ?)",
            )?;
            let rows = stmt.query_map(
                duckdb::params![memory_id.to_string(), now],
                |row| row.get::<_, String>(0),
            )?;

            for row in rows {
                perms.push(row.map_err(|e| Error::Storage(e.to_string()))?);
            }

            perms
        }; // conn lock dropped here

        for perm_str in &acl_result {
            if let Ok(perm) = perm_str.parse::<Permission>() {
                if perm.satisfies(required) {
                    return Ok(true);
                }
            }
        }

        // Check delegations (conn lock is released)
        if self.check_delegation(principal_id, memory_id, required).await? {
            return Ok(true);
        }

        Ok(false)
    }

    async fn insert_relation(&self, relation: &Relation) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO relations (id, source_id, target_id, relation_type, weight, metadata, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                relation.id.to_string(),
                relation.source_id.to_string(),
                relation.target_id.to_string(),
                relation.relation_type,
                relation.weight,
                serde_json::to_string(&relation.metadata)?,
                relation.created_at,
            ],
        )?;
        Ok(())
    }

    async fn get_relations_from(&self, source_id: Uuid) -> Result<Vec<Relation>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, source_id, target_id, relation_type, weight, metadata, created_at FROM relations WHERE source_id = ?",
        )?;
        let rows = stmt.query_map([source_id.to_string()], row_to_relation)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
        }
        Ok(results)
    }

    async fn get_relations_to(&self, target_id: Uuid) -> Result<Vec<Relation>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, source_id, target_id, relation_type, weight, metadata, created_at FROM relations WHERE target_id = ?",
        )?;
        let rows = stmt.query_map([target_id.to_string()], row_to_relation)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
        }
        Ok(results)
    }

    async fn delete_relation(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().await;
        let affected = conn.execute(
            "DELETE FROM relations WHERE id = ?",
            duckdb::params![id.to_string()],
        )?;
        if affected == 0 {
            return Err(Error::NotFound(format!("relation {id} not found")));
        }
        Ok(())
    }

    async fn get_latest_memory_hash(&self, agent_id: &str, thread_id: Option<&str>) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().await;
        let (sql, result) = if let Some(tid) = thread_id {
            let mut stmt = conn.prepare(
                "SELECT content_hash FROM memories WHERE agent_id = ? AND thread_id = ? AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
            )?;
            let r = stmt.query_row(duckdb::params![agent_id, tid], |row| row.get::<_, Vec<u8>>(0));
            ((), r)
        } else {
            let mut stmt = conn.prepare(
                "SELECT content_hash FROM memories WHERE agent_id = ? AND thread_id IS NULL AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
            )?;
            let r = stmt.query_row(duckdb::params![agent_id], |row| row.get::<_, Vec<u8>>(0));
            ((), r)
        };
        let _ = sql;
        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }

    async fn get_latest_event_hash(&self, agent_id: &str, thread_id: Option<&str>) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().await;
        let result = if let Some(tid) = thread_id {
            let mut stmt = conn.prepare(
                "SELECT content_hash FROM agent_events WHERE agent_id = ? AND thread_id = ? ORDER BY timestamp DESC LIMIT 1",
            )?;
            stmt.query_row(duckdb::params![agent_id, tid], |row| row.get::<_, Vec<u8>>(0))
        } else {
            let mut stmt = conn.prepare(
                "SELECT content_hash FROM agent_events WHERE agent_id = ? ORDER BY timestamp DESC LIMIT 1",
            )?;
            stmt.query_row(duckdb::params![agent_id], |row| row.get::<_, Vec<u8>>(0))
        };
        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }

    async fn get_sync_watermark(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT value FROM sync_metadata WHERE key = ?",
        )?;
        let result = stmt.query_row(duckdb::params![key], |row| row.get::<_, String>(0));
        match result {
            Ok(value) => Ok(Some(value)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }

    async fn set_sync_watermark(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        // Try update first, then insert
        let affected = conn.execute(
            "UPDATE sync_metadata SET value = ?, updated_at = ? WHERE key = ?",
            duckdb::params![value, now, key],
        )?;
        if affected == 0 {
            conn.execute(
                "INSERT INTO sync_metadata (key, value, updated_at) VALUES (?, ?, ?)",
                duckdb::params![key, value, now],
            )?;
        }
        Ok(())
    }

    async fn list_accessible_memory_ids(&self, agent_id: &str, limit: usize) -> Result<Vec<Uuid>> {
        let conn = self.conn.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT id FROM memories WHERE (agent_id = ? OR scope = 'public' OR id IN (SELECT memory_id FROM acls WHERE principal_id = ? AND (expires_at IS NULL OR expires_at > ?))) AND deleted_at IS NULL LIMIT ?",
        )?;
        let rows = stmt.query_map(
            duckdb::params![agent_id, agent_id, now, limit as i64],
            |row| row.get::<_, String>(0),
        )?;
        let mut ids = Vec::new();
        for row in rows {
            let id_str = row.map_err(|e| Error::Storage(e.to_string()))?;
            ids.push(Uuid::parse_str(&id_str).map_err(|e| Error::Storage(e.to_string()))?);
        }
        Ok(ids)
    }

    async fn insert_event(&self, event: &AgentEvent) -> Result<()> {
        let conn = self.conn.lock().await;
        let payload_json = serde_json::to_string(&event.payload)?;
        let embedding_blob = serialize_embedding(&event.embedding);
        conn.execute(
            "INSERT INTO agent_events (id, agent_id, thread_id, run_id, parent_event_id, event_type, payload, trace_id, span_id, model, tokens_input, tokens_output, latency_ms, cost_usd, timestamp, logical_clock, content_hash, prev_hash, embedding) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                event.id.to_string(),
                event.agent_id,
                event.thread_id,
                event.run_id,
                event.parent_event_id.map(|id| id.to_string()),
                event.event_type.to_string(),
                payload_json,
                event.trace_id,
                event.span_id,
                event.model,
                event.tokens_input,
                event.tokens_output,
                event.latency_ms,
                event.cost_usd,
                event.timestamp,
                event.logical_clock,
                event.content_hash,
                event.prev_hash,
                embedding_blob,
            ],
        )?;
        Ok(())
    }

    async fn list_events(&self, agent_id: &str, limit: usize, offset: usize) -> Result<Vec<AgentEvent>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, thread_id, run_id, parent_event_id, event_type, payload, trace_id, span_id, model, tokens_input, tokens_output, latency_ms, cost_usd, timestamp, logical_clock, content_hash, prev_hash, embedding FROM agent_events WHERE agent_id = ? ORDER BY timestamp DESC LIMIT ? OFFSET ?",
        )?;
        let rows = stmt.query_map(
            duckdb::params![agent_id, limit as i64, offset as i64],
            row_to_event,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
        }
        Ok(results)
    }

    async fn get_events_by_thread(&self, thread_id: &str, limit: usize) -> Result<Vec<AgentEvent>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, thread_id, run_id, parent_event_id, event_type, payload, trace_id, span_id, model, tokens_input, tokens_output, latency_ms, cost_usd, timestamp, logical_clock, content_hash, prev_hash, embedding FROM agent_events WHERE thread_id = ? ORDER BY timestamp ASC LIMIT ?",
        )?;
        let rows = stmt.query_map(
            duckdb::params![thread_id, limit as i64],
            row_to_event,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
        }
        Ok(results)
    }

    async fn get_event(&self, id: Uuid) -> Result<Option<AgentEvent>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, thread_id, run_id, parent_event_id, event_type, payload, trace_id, span_id, model, tokens_input, tokens_output, latency_ms, cost_usd, timestamp, logical_clock, content_hash, prev_hash, embedding FROM agent_events WHERE id = ?",
        )?;
        let result = stmt.query_row([id.to_string()], row_to_event);
        match result {
            Ok(event) => Ok(Some(event)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }

    async fn list_child_events(&self, parent_event_id: Uuid, limit: usize) -> Result<Vec<AgentEvent>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, thread_id, run_id, parent_event_id, event_type, payload, trace_id, span_id, model, tokens_input, tokens_output, latency_ms, cost_usd, timestamp, logical_clock, content_hash, prev_hash, embedding FROM agent_events WHERE parent_event_id = ? ORDER BY timestamp ASC LIMIT ?",
        )?;
        let rows = stmt.query_map(
            duckdb::params![parent_event_id.to_string(), limit as i64],
            row_to_event,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
        }
        Ok(results)
    }

    async fn list_memories_by_agent_ordered(&self, agent_id: &str, thread_id: Option<&str>, limit: usize) -> Result<Vec<MemoryRecord>> {
        let conn = self.conn.lock().await;
        let (result,) = if let Some(tid) = thread_id {
            let mut stmt = conn.prepare(
                "SELECT id, agent_id, content, memory_type, scope, importance, tags, metadata, embedding, content_hash, prev_hash, source_type, source_id, consolidation_state, access_count, org_id, thread_id, created_at, updated_at, last_accessed_at, expires_at, deleted_at, decay_rate, created_by, version, prev_version_id, quarantined, quarantine_reason, decay_function FROM memories WHERE agent_id = ? AND thread_id = ? AND deleted_at IS NULL ORDER BY created_at ASC LIMIT ?",
            )?;
            let rows = stmt.query_map(
                duckdb::params![agent_id, tid, limit as i64],
                row_to_memory,
            )?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
            }
            (results,)
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, agent_id, content, memory_type, scope, importance, tags, metadata, embedding, content_hash, prev_hash, source_type, source_id, consolidation_state, access_count, org_id, thread_id, created_at, updated_at, last_accessed_at, expires_at, deleted_at, decay_rate, created_by, version, prev_version_id, quarantined, quarantine_reason, decay_function FROM memories WHERE agent_id = ? AND deleted_at IS NULL ORDER BY created_at ASC LIMIT ?",
            )?;
            let rows = stmt.query_map(
                duckdb::params![agent_id, limit as i64],
                row_to_memory,
            )?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
            }
            (results,)
        };
        Ok(result)
    }

    async fn list_memories_since(&self, updated_after: &str, limit: usize) -> Result<Vec<MemoryRecord>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, content, memory_type, scope, importance, tags, metadata, embedding, content_hash, prev_hash, source_type, source_id, consolidation_state, access_count, org_id, thread_id, created_at, updated_at, last_accessed_at, expires_at, deleted_at, decay_rate, created_by, version, prev_version_id, quarantined, quarantine_reason, decay_function FROM memories WHERE updated_at > ? ORDER BY updated_at ASC LIMIT ?",
        )?;
        let rows = stmt.query_map(
            duckdb::params![updated_after, limit as i64],
            row_to_memory,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
        }
        Ok(results)
    }

    async fn upsert_memory(&self, record: &MemoryRecord) -> Result<()> {
        // Try update first; if no rows affected, insert
        match self.update_memory(record).await {
            Ok(()) => Ok(()),
            Err(Error::NotFound(_)) => self.insert_memory(record).await,
            Err(e) => Err(e),
        }
    }

    async fn cleanup_expired(&self) -> Result<usize> {
        let conn = self.conn.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        let affected = conn.execute(
            "UPDATE memories SET deleted_at = ? WHERE expires_at IS NOT NULL AND expires_at < ? AND deleted_at IS NULL",
            duckdb::params![now.clone(), now],
        )?;
        Ok(affected)
    }

    async fn insert_delegation(&self, d: &Delegation) -> Result<()> {
        let conn = self.conn.lock().await;
        let scope_type = d.scope.to_string();
        let scope_value = match &d.scope {
            DelegationScope::AllMemories => serde_json::Value::Null,
            DelegationScope::ByTag(tags) => serde_json::json!(tags),
            DelegationScope::ByMemoryId(ids) => serde_json::json!(ids.iter().map(|id| id.to_string()).collect::<Vec<_>>()),
        };
        let scope_value_json = serde_json::to_string(&scope_value)?;

        conn.execute(
            "INSERT INTO delegations (id, delegator_id, delegate_id, permission, scope_type, scope_value, max_depth, current_depth, parent_delegation_id, created_at, expires_at, revoked_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                d.id.to_string(),
                d.delegator_id,
                d.delegate_id,
                d.permission.to_string(),
                scope_type,
                scope_value_json,
                d.max_depth as i32,
                d.current_depth as i32,
                d.parent_delegation_id.map(|id| id.to_string()),
                d.created_at,
                d.expires_at,
                d.revoked_at,
            ],
        )?;
        Ok(())
    }

    async fn list_delegations_for(&self, delegate_id: &str) -> Result<Vec<Delegation>> {
        let conn = self.conn.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT id, delegator_id, delegate_id, permission, scope_type, scope_value, max_depth, current_depth, parent_delegation_id, created_at, expires_at, revoked_at FROM delegations WHERE delegate_id = ? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)",
        )?;
        let rows = stmt.query_map(duckdb::params![delegate_id, now], row_to_delegation)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
        }
        Ok(results)
    }

    async fn revoke_delegation(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        let affected = conn.execute(
            "UPDATE delegations SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL",
            duckdb::params![now, id.to_string()],
        )?;
        if affected == 0 {
            return Err(Error::NotFound(format!("delegation {id} not found or already revoked")));
        }
        Ok(())
    }

    async fn check_delegation(&self, delegate_id: &str, memory_id: Uuid, required: Permission) -> Result<bool> {
        let delegations = self.list_delegations_for(delegate_id).await?;
        // Get the memory to check scope
        let memory = match self.get_memory(memory_id).await? {
            Some(m) => m,
            None => return Ok(false),
        };

        for d in &delegations {
            if !d.permission.satisfies(required) {
                continue;
            }
            match &d.scope {
                DelegationScope::AllMemories => return Ok(true),
                DelegationScope::ByMemoryId(ids) => {
                    if ids.contains(&memory_id) {
                        return Ok(true);
                    }
                }
                DelegationScope::ByTag(tags) => {
                    if tags.iter().any(|t| memory.tags.contains(t)) {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    async fn insert_or_update_agent_profile(&self, profile: &AgentProfile) -> Result<()> {
        let conn = self.conn.lock().await;
        // Try update first, then insert
        let affected = conn.execute(
            "UPDATE agent_profiles SET avg_importance = ?, avg_content_length = ?, total_memories = ?, last_updated = ? WHERE agent_id = ?",
            duckdb::params![
                profile.avg_importance,
                profile.avg_content_length,
                profile.total_memories as i64,
                profile.last_updated,
                profile.agent_id,
            ],
        )?;
        if affected == 0 {
            conn.execute(
                "INSERT INTO agent_profiles (agent_id, avg_importance, avg_content_length, total_memories, last_updated) VALUES (?, ?, ?, ?, ?)",
                duckdb::params![
                    profile.agent_id,
                    profile.avg_importance,
                    profile.avg_content_length,
                    profile.total_memories as i64,
                    profile.last_updated,
                ],
            )?;
        }
        Ok(())
    }

    async fn get_agent_profile(&self, agent_id: &str) -> Result<Option<AgentProfile>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT agent_id, avg_importance, avg_content_length, total_memories, last_updated FROM agent_profiles WHERE agent_id = ?",
        )?;
        let result = stmt.query_row([agent_id], |row| {
            Ok(AgentProfile {
                agent_id: row.get(0)?,
                avg_importance: row.get(1)?,
                avg_content_length: row.get(2)?,
                total_memories: row.get::<_, i64>(3)? as u64,
                last_updated: row.get(4)?,
            })
        });
        match result {
            Ok(profile) => Ok(Some(profile)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }

    async fn insert_checkpoint(&self, cp: &Checkpoint) -> Result<()> {
        let conn = self.conn.lock().await;
        let state_snapshot_json = serde_json::to_string(&cp.state_snapshot)?;
        let state_diff_json = cp.state_diff.as_ref().map(serde_json::to_string).transpose()?;
        let memory_refs_json = serde_json::to_string(
            &cp.memory_refs.iter().map(|id| id.to_string()).collect::<Vec<_>>()
        )?;
        let metadata_json = serde_json::to_string(&cp.metadata)?;

        conn.execute(
            "INSERT INTO checkpoints (id, thread_id, agent_id, parent_id, branch_name, state_snapshot, state_diff, memory_refs, event_cursor, label, created_at, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                cp.id.to_string(),
                cp.thread_id,
                cp.agent_id,
                cp.parent_id.map(|id| id.to_string()),
                cp.branch_name,
                state_snapshot_json,
                state_diff_json,
                memory_refs_json,
                cp.event_cursor.map(|id| id.to_string()),
                cp.label,
                cp.created_at,
                metadata_json,
            ],
        )?;
        Ok(())
    }

    async fn get_checkpoint(&self, id: Uuid) -> Result<Option<Checkpoint>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, agent_id, parent_id, branch_name, state_snapshot, state_diff, memory_refs, event_cursor, label, created_at, metadata FROM checkpoints WHERE id = ?",
        )?;
        let result = stmt.query_row([id.to_string()], row_to_checkpoint);
        match result {
            Ok(cp) => Ok(Some(cp)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }

    async fn list_checkpoints(&self, thread_id: &str, branch: Option<&str>, limit: usize) -> Result<Vec<Checkpoint>> {
        let conn = self.conn.lock().await;
        let (sql, rows_result) = if let Some(branch_name) = branch {
            let mut stmt = conn.prepare(
                "SELECT id, thread_id, agent_id, parent_id, branch_name, state_snapshot, state_diff, memory_refs, event_cursor, label, created_at, metadata FROM checkpoints WHERE thread_id = ? AND branch_name = ? ORDER BY created_at DESC LIMIT ?",
            )?;
            let rows = stmt.query_map(
                duckdb::params![thread_id, branch_name, limit as i64],
                row_to_checkpoint,
            )?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
            }
            ((), Ok(results))
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, thread_id, agent_id, parent_id, branch_name, state_snapshot, state_diff, memory_refs, event_cursor, label, created_at, metadata FROM checkpoints WHERE thread_id = ? ORDER BY created_at DESC LIMIT ?",
            )?;
            let rows = stmt.query_map(
                duckdb::params![thread_id, limit as i64],
                row_to_checkpoint,
            )?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
            }
            ((), Ok(results))
        };
        let _ = sql;
        rows_result
    }

    async fn get_latest_checkpoint(&self, thread_id: &str, branch: &str) -> Result<Option<Checkpoint>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, agent_id, parent_id, branch_name, state_snapshot, state_diff, memory_refs, event_cursor, label, created_at, metadata FROM checkpoints WHERE thread_id = ? AND branch_name = ? ORDER BY created_at DESC LIMIT 1",
        )?;
        let result = stmt.query_row(
            duckdb::params![thread_id, branch],
            row_to_checkpoint,
        );
        match result {
            Ok(cp) => Ok(Some(cp)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }
}

fn row_to_event(row: &duckdb::Row<'_>) -> duckdb::Result<AgentEvent> {
    let id_str: String = row.get(0)?;
    let parent_id_str: Option<String> = row.get(4)?;
    let payload_json: Option<String> = row.get(6)?;
    let content_hash: Vec<u8> = row.get(16)?;
    let prev_hash: Option<Vec<u8>> = row.get(17)?;
    let embedding_blob: Option<Vec<u8>> = row.get(18).unwrap_or(None);

    Ok(AgentEvent {
        id: Uuid::parse_str(&id_str).unwrap(),
        agent_id: row.get(1)?,
        thread_id: row.get(2)?,
        run_id: row.get(3)?,
        parent_event_id: parent_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
        event_type: row.get::<_, String>(5)?.parse().unwrap(),
        payload: payload_json
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Null),
        trace_id: row.get(7)?,
        span_id: row.get(8)?,
        model: row.get(9)?,
        tokens_input: row.get(10)?,
        tokens_output: row.get(11)?,
        latency_ms: row.get(12)?,
        cost_usd: row.get(13)?,
        timestamp: row.get(14)?,
        logical_clock: row.get(15)?,
        content_hash,
        prev_hash,
        embedding: deserialize_embedding(embedding_blob),
    })
}

fn row_to_checkpoint(row: &duckdb::Row<'_>) -> duckdb::Result<Checkpoint> {
    let id_str: String = row.get(0)?;
    let parent_id_str: Option<String> = row.get(3)?;
    let state_snapshot_json: Option<String> = row.get(5)?;
    let state_diff_json: Option<String> = row.get(6)?;
    let memory_refs_json: Option<String> = row.get(7)?;
    let event_cursor_str: Option<String> = row.get(8)?;
    let metadata_json: Option<String> = row.get(11)?;

    Ok(Checkpoint {
        id: Uuid::parse_str(&id_str).unwrap(),
        thread_id: row.get(1)?,
        agent_id: row.get(2)?,
        parent_id: parent_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
        branch_name: row.get(4)?,
        state_snapshot: state_snapshot_json
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        state_diff: state_diff_json.and_then(|s| serde_json::from_str(&s).ok()),
        memory_refs: memory_refs_json
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .map(|v| v.into_iter().filter_map(|s| Uuid::parse_str(&s).ok()).collect())
            .unwrap_or_default(),
        event_cursor: event_cursor_str.and_then(|s| Uuid::parse_str(&s).ok()),
        label: row.get(9)?,
        created_at: row.get(10)?,
        metadata: metadata_json
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
    })
}

fn row_to_delegation(row: &duckdb::Row<'_>) -> duckdb::Result<Delegation> {
    let id_str: String = row.get(0)?;
    let scope_type: String = row.get(4)?;
    let scope_value_json: Option<String> = row.get(5)?;
    let parent_id_str: Option<String> = row.get(8)?;

    let scope = match scope_type.as_str() {
        "by_tag" => {
            let tags: Vec<String> = scope_value_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            DelegationScope::ByTag(tags)
        }
        "by_memory_id" => {
            let ids: Vec<String> = scope_value_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            let uuids = ids.into_iter().filter_map(|s| Uuid::parse_str(&s).ok()).collect();
            DelegationScope::ByMemoryId(uuids)
        }
        _ => DelegationScope::AllMemories,
    };

    Ok(Delegation {
        id: Uuid::parse_str(&id_str).unwrap(),
        delegator_id: row.get(1)?,
        delegate_id: row.get(2)?,
        permission: row.get::<_, String>(3)?.parse().unwrap(),
        scope,
        max_depth: row.get::<_, i32>(6)? as u32,
        current_depth: row.get::<_, i32>(7)? as u32,
        parent_delegation_id: parent_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
        created_at: row.get(9)?,
        expires_at: row.get(10)?,
        revoked_at: row.get(11)?,
    })
}

fn row_to_relation(row: &duckdb::Row<'_>) -> duckdb::Result<Relation> {
    let id_str: String = row.get(0)?;
    let source_str: String = row.get(1)?;
    let target_str: String = row.get(2)?;
    let metadata_json: Option<String> = row.get(5)?;

    Ok(Relation {
        id: Uuid::parse_str(&id_str).unwrap(),
        source_id: Uuid::parse_str(&source_str).unwrap(),
        target_id: Uuid::parse_str(&target_str).unwrap(),
        relation_type: row.get(3)?,
        weight: row.get(4)?,
        metadata: metadata_json
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::compute_content_hash;
    use crate::model::acl::PrincipalType;
    use crate::model::checkpoint::Checkpoint;
    use crate::model::event::{AgentEvent, EventType};
    use crate::model::memory::{ConsolidationState, MemoryType, Scope, SourceType};

    fn make_record(agent_id: &str) -> MemoryRecord {
        let now = chrono::Utc::now().to_rfc3339();
        let content = "test memory content";
        MemoryRecord {
            id: Uuid::now_v7(),
            agent_id: agent_id.to_string(),
            content: content.to_string(),
            memory_type: MemoryType::Semantic,
            scope: Scope::Private,
            importance: 0.7,
            tags: vec!["test".to_string()],
            metadata: serde_json::json!({"key": "value"}),
            embedding: Some(vec![0.1, 0.2, 0.3]),
            content_hash: compute_content_hash(content, agent_id, &now),
            prev_hash: None,
            source_type: SourceType::Agent,
            source_id: None,
            consolidation_state: ConsolidationState::Raw,
            access_count: 0,
            org_id: None,
            thread_id: None,
            created_at: now.clone(),
            updated_at: now,
            last_accessed_at: None,
            expires_at: None,
            deleted_at: None,
            decay_rate: None,
            created_by: None,
            version: 1,
            prev_version_id: None,
            quarantined: false,
            quarantine_reason: None,
            decay_function: None,
        }
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let storage = DuckDbStorage::open_in_memory().unwrap();
        let record = make_record("agent-1");
        storage.insert_memory(&record).await.unwrap();

        let fetched = storage.get_memory(record.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, record.id);
        assert_eq!(fetched.content, record.content);
        assert_eq!(fetched.agent_id, record.agent_id);
        assert_eq!(fetched.memory_type, record.memory_type);
        assert_eq!(fetched.tags, record.tags);
        assert_eq!(fetched.embedding, record.embedding);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let storage = DuckDbStorage::open_in_memory().unwrap();
        let result = storage.get_memory(Uuid::now_v7()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_soft_delete() {
        let storage = DuckDbStorage::open_in_memory().unwrap();
        let record = make_record("agent-1");
        storage.insert_memory(&record).await.unwrap();

        storage.soft_delete_memory(record.id).await.unwrap();

        // Should still exist in DB but with deleted_at set
        let fetched = storage.get_memory(record.id).await.unwrap().unwrap();
        assert!(fetched.deleted_at.is_some());

        // Should not appear in list by default
        let filter = MemoryFilter::default();
        let list = storage.list_memories(&filter, 100, 0).await.unwrap();
        assert!(list.is_empty());

        // Should appear with include_deleted
        let filter_with_deleted = MemoryFilter {
            include_deleted: true,
            ..Default::default()
        };
        let list = storage.list_memories(&filter_with_deleted, 100, 0).await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_hard_delete() {
        let storage = DuckDbStorage::open_in_memory().unwrap();
        let record = make_record("agent-1");
        storage.insert_memory(&record).await.unwrap();

        storage.hard_delete_memory(record.id).await.unwrap();

        let result = storage.get_memory(record.id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_with_filters() {
        let storage = DuckDbStorage::open_in_memory().unwrap();

        let mut r1 = make_record("agent-1");
        r1.memory_type = MemoryType::Episodic;
        storage.insert_memory(&r1).await.unwrap();

        let mut r2 = make_record("agent-1");
        r2.memory_type = MemoryType::Semantic;
        storage.insert_memory(&r2).await.unwrap();

        let mut r3 = make_record("agent-2");
        r3.memory_type = MemoryType::Semantic;
        storage.insert_memory(&r3).await.unwrap();

        // Filter by agent
        let filter = MemoryFilter {
            agent_id: Some("agent-1".to_string()),
            ..Default::default()
        };
        let list = storage.list_memories(&filter, 100, 0).await.unwrap();
        assert_eq!(list.len(), 2);

        // Filter by type
        let filter = MemoryFilter {
            memory_type: Some(MemoryType::Semantic),
            ..Default::default()
        };
        let list = storage.list_memories(&filter, 100, 0).await.unwrap();
        assert_eq!(list.len(), 2);

        // Filter by agent + type
        let filter = MemoryFilter {
            agent_id: Some("agent-1".to_string()),
            memory_type: Some(MemoryType::Episodic),
            ..Default::default()
        };
        let list = storage.list_memories(&filter, 100, 0).await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_touch_memory() {
        let storage = DuckDbStorage::open_in_memory().unwrap();
        let record = make_record("agent-1");
        storage.insert_memory(&record).await.unwrap();

        storage.touch_memory(record.id).await.unwrap();
        storage.touch_memory(record.id).await.unwrap();

        let fetched = storage.get_memory(record.id).await.unwrap().unwrap();
        assert_eq!(fetched.access_count, 2);
        assert!(fetched.last_accessed_at.is_some());
    }

    #[tokio::test]
    async fn test_acl_and_permission_check() {
        let storage = DuckDbStorage::open_in_memory().unwrap();
        let record = make_record("agent-1");
        storage.insert_memory(&record).await.unwrap();

        // Owner always has permission
        assert!(storage.check_permission(record.id, "agent-1", Permission::Admin).await.unwrap());

        // Non-owner has no permission by default
        assert!(!storage.check_permission(record.id, "agent-2", Permission::Read).await.unwrap());

        // Grant read to agent-2
        let acl = Acl {
            id: Uuid::now_v7(),
            memory_id: record.id,
            principal_type: PrincipalType::Agent,
            principal_id: "agent-2".to_string(),
            permission: Permission::Read,
            granted_by: "agent-1".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: None,
        };
        storage.insert_acl(&acl).await.unwrap();

        // Now agent-2 can read
        assert!(storage.check_permission(record.id, "agent-2", Permission::Read).await.unwrap());
        // But not write
        assert!(!storage.check_permission(record.id, "agent-2", Permission::Write).await.unwrap());
    }

    #[tokio::test]
    async fn test_event_insert_and_list() {
        let storage = DuckDbStorage::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let event = AgentEvent {
            id: Uuid::now_v7(),
            agent_id: "agent-1".to_string(),
            thread_id: Some("thread-1".to_string()),
            run_id: None,
            parent_event_id: None,
            event_type: EventType::MemoryWrite,
            payload: serde_json::json!({"memory_id": "abc"}),
            trace_id: None,
            span_id: None,
            model: None,
            tokens_input: None,
            tokens_output: None,
            latency_ms: None,
            cost_usd: None,
            timestamp: now.clone(),
            logical_clock: 1,
            content_hash: vec![1, 2, 3],
            prev_hash: None,
            embedding: None,
        };

        storage.insert_event(&event).await.unwrap();

        let events = storage.list_events("agent-1", 10, 0).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, event.id);
        assert_eq!(events[0].event_type, EventType::MemoryWrite);
        assert_eq!(events[0].agent_id, "agent-1");

        // Get single event
        let fetched = storage.get_event(event.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, event.id);
        assert_eq!(fetched.content_hash, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_events_by_thread() {
        let storage = DuckDbStorage::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        for i in 0..3 {
            let event = AgentEvent {
                id: Uuid::now_v7(),
                agent_id: "agent-1".to_string(),
                thread_id: Some("thread-A".to_string()),
                run_id: None,
                parent_event_id: None,
                event_type: EventType::MemoryWrite,
                payload: serde_json::json!({"i": i}),
                trace_id: None,
                span_id: None,
                model: None,
                tokens_input: None,
                tokens_output: None,
                latency_ms: None,
                cost_usd: None,
                timestamp: now.clone(),
                logical_clock: i,
                content_hash: vec![i as u8],
                prev_hash: None,
                embedding: None,
            };
            storage.insert_event(&event).await.unwrap();
        }

        // Different thread
        let event = AgentEvent {
            id: Uuid::now_v7(),
            agent_id: "agent-1".to_string(),
            thread_id: Some("thread-B".to_string()),
            run_id: None,
            parent_event_id: None,
            event_type: EventType::MemoryRead,
            payload: serde_json::json!({}),
            trace_id: None,
            span_id: None,
            model: None,
            tokens_input: None,
            tokens_output: None,
            latency_ms: None,
            cost_usd: None,
            timestamp: now.clone(),
            logical_clock: 0,
            content_hash: vec![99],
            prev_hash: None,
            embedding: None,
        };
        storage.insert_event(&event).await.unwrap();

        let thread_a = storage.get_events_by_thread("thread-A", 10).await.unwrap();
        assert_eq!(thread_a.len(), 3);

        let thread_b = storage.get_events_by_thread("thread-B", 10).await.unwrap();
        assert_eq!(thread_b.len(), 1);
        assert_eq!(thread_b[0].event_type, EventType::MemoryRead);
    }

    #[tokio::test]
    async fn test_checkpoint_insert_and_get() {
        let storage = DuckDbStorage::open_in_memory().unwrap();
        let mem_id = Uuid::now_v7();
        let cp = Checkpoint {
            id: Uuid::now_v7(),
            thread_id: "thread-1".to_string(),
            agent_id: "agent-1".to_string(),
            parent_id: None,
            branch_name: "main".to_string(),
            state_snapshot: serde_json::json!({"step": 1}),
            state_diff: None,
            memory_refs: vec![mem_id],
            event_cursor: None,
            label: Some("initial".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            metadata: serde_json::json!({}),
        };

        storage.insert_checkpoint(&cp).await.unwrap();

        let fetched = storage.get_checkpoint(cp.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, cp.id);
        assert_eq!(fetched.thread_id, "thread-1");
        assert_eq!(fetched.branch_name, "main");
        assert_eq!(fetched.memory_refs, vec![mem_id]);
        assert_eq!(fetched.label, Some("initial".to_string()));
    }

    #[tokio::test]
    async fn test_checkpoint_list_and_latest() {
        let storage = DuckDbStorage::open_in_memory().unwrap();

        let cp1 = Checkpoint {
            id: Uuid::now_v7(),
            thread_id: "thread-1".to_string(),
            agent_id: "agent-1".to_string(),
            parent_id: None,
            branch_name: "main".to_string(),
            state_snapshot: serde_json::json!({"step": 1}),
            state_diff: None,
            memory_refs: vec![],
            event_cursor: None,
            label: Some("first".to_string()),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            metadata: serde_json::json!({}),
        };
        storage.insert_checkpoint(&cp1).await.unwrap();

        let cp2 = Checkpoint {
            id: Uuid::now_v7(),
            thread_id: "thread-1".to_string(),
            agent_id: "agent-1".to_string(),
            parent_id: Some(cp1.id),
            branch_name: "main".to_string(),
            state_snapshot: serde_json::json!({"step": 2}),
            state_diff: Some(serde_json::json!({"step": [1, 2]})),
            memory_refs: vec![],
            event_cursor: None,
            label: Some("second".to_string()),
            created_at: "2025-01-02T00:00:00Z".to_string(),
            metadata: serde_json::json!({}),
        };
        storage.insert_checkpoint(&cp2).await.unwrap();

        let cp3 = Checkpoint {
            id: Uuid::now_v7(),
            thread_id: "thread-1".to_string(),
            agent_id: "agent-1".to_string(),
            parent_id: Some(cp1.id),
            branch_name: "experiment".to_string(),
            state_snapshot: serde_json::json!({"step": "alt"}),
            state_diff: None,
            memory_refs: vec![],
            event_cursor: None,
            label: None,
            created_at: "2025-01-03T00:00:00Z".to_string(),
            metadata: serde_json::json!({}),
        };
        storage.insert_checkpoint(&cp3).await.unwrap();

        // List all for thread
        let all = storage.list_checkpoints("thread-1", None, 10).await.unwrap();
        assert_eq!(all.len(), 3);

        // List by branch
        let main_cps = storage.list_checkpoints("thread-1", Some("main"), 10).await.unwrap();
        assert_eq!(main_cps.len(), 2);

        let exp_cps = storage.list_checkpoints("thread-1", Some("experiment"), 10).await.unwrap();
        assert_eq!(exp_cps.len(), 1);

        // Latest on main
        let latest = storage.get_latest_checkpoint("thread-1", "main").await.unwrap().unwrap();
        assert_eq!(latest.id, cp2.id);

        // Latest on experiment
        let latest_exp = storage.get_latest_checkpoint("thread-1", "experiment").await.unwrap().unwrap();
        assert_eq!(latest_exp.id, cp3.id);

        // No checkpoints for nonexistent branch
        let none = storage.get_latest_checkpoint("thread-1", "nonexistent").await.unwrap();
        assert!(none.is_none());
    }
}
