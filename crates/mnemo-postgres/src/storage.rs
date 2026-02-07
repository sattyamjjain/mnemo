use mnemo_core::error::{Error, Result};
use mnemo_core::model::acl::{Acl, Permission};
use mnemo_core::model::agent_profile::AgentProfile;
use mnemo_core::model::checkpoint::Checkpoint;
use mnemo_core::model::delegation::{Delegation, DelegationScope};
use mnemo_core::model::event::AgentEvent;
use mnemo_core::model::memory::MemoryRecord;
use mnemo_core::model::relation::Relation;
use mnemo_core::storage::{MemoryFilter, StorageBackend};
use pgvector::Vector;
use sqlx::Row;
use uuid::Uuid;

/// PostgreSQL-backed storage for Mnemo.
///
/// Wraps a `sqlx::PgPool` and runs schema migrations on construction.
/// Embeddings are stored using the pgvector `vector` column type, while
/// event embeddings are stored as `BYTEA` (serialised `Vec<f32>` in
/// little-endian byte order), matching the DuckDB backend convention.
pub struct PgStorage {
    pool: sqlx::PgPool,
    #[allow(dead_code)]
    dimensions: usize,
}

impl PgStorage {
    /// Connect to a PostgreSQL database and run migrations.
    ///
    /// `url` is a standard `postgres://` connection string.
    /// `dimensions` controls the width of the pgvector `vector` column.
    pub async fn connect(url: &str, dimensions: usize) -> Result<Self> {
        let pool = sqlx::PgPool::connect(url)
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;
        let storage = Self { pool, dimensions };
        crate::migrations::run_migrations(&storage.pool, dimensions).await?;
        Ok(storage)
    }

    /// Build a `PgStorage` from an existing pool (useful for tests).
    pub async fn from_pool(pool: sqlx::PgPool, dimensions: usize) -> Result<Self> {
        crate::migrations::run_migrations(&pool, dimensions).await?;
        Ok(Self { pool, dimensions })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn map_sqlx(e: sqlx::Error) -> Error {
    Error::Storage(e.to_string())
}

fn serialize_embedding(embedding: &Option<Vec<f32>>) -> Option<Vec<u8>> {
    embedding
        .as_ref()
        .map(|v| v.iter().flat_map(|f| f.to_le_bytes()).collect())
}

fn deserialize_embedding(blob: Option<Vec<u8>>) -> Option<Vec<f32>> {
    blob.map(|bytes| {
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    })
}

fn row_to_memory(row: &sqlx::postgres::PgRow) -> std::result::Result<MemoryRecord, sqlx::Error> {
    let tags: Vec<String> = row.try_get::<Vec<String>, _>("tags").unwrap_or_default();
    let metadata: serde_json::Value = row
        .try_get("metadata")
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    // pgvector stores the embedding as its own type; we retrieve the raw text
    // representation and parse back to Vec<f32>. If the column is NULL we get None.
    let embedding: Option<Vec<f32>> = {
        let raw: Option<String> = row.try_get("embedding_text").ok().flatten();
        raw.and_then(|s| {
            // pgvector text output looks like "[0.1,0.2,0.3]"
            let trimmed = s.trim_start_matches('[').trim_end_matches(']');
            if trimmed.is_empty() {
                None
            } else {
                Some(
                    trimmed
                        .split(',')
                        .filter_map(|v| v.trim().parse::<f32>().ok())
                        .collect(),
                )
            }
        })
    };

    Ok(MemoryRecord {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        content: row.get("content"),
        memory_type: row
            .get::<String, _>("memory_type")
            .parse()
            .unwrap_or(mnemo_core::model::memory::MemoryType::Semantic),
        scope: row
            .get::<String, _>("scope")
            .parse()
            .unwrap_or(mnemo_core::model::memory::Scope::Private),
        importance: row.get("importance"),
        tags,
        metadata,
        embedding,
        content_hash: row.get("content_hash"),
        prev_hash: row.get("prev_hash"),
        source_type: row
            .get::<String, _>("source_type")
            .parse()
            .unwrap_or(mnemo_core::model::memory::SourceType::Agent),
        source_id: row.get("source_id"),
        consolidation_state: row
            .get::<String, _>("consolidation_state")
            .parse()
            .unwrap_or(mnemo_core::model::memory::ConsolidationState::Raw),
        access_count: row.get::<i64, _>("access_count") as u64,
        org_id: row.get("org_id"),
        thread_id: row.get("thread_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        last_accessed_at: row.get("last_accessed_at"),
        expires_at: row.get("expires_at"),
        deleted_at: row.get("deleted_at"),
        decay_rate: row.get("decay_rate"),
        created_by: row.get("created_by"),
        version: row.get::<i32, _>("version") as u32,
        prev_version_id: row.get("prev_version_id"),
        quarantined: row.get("quarantined"),
        quarantine_reason: row.get("quarantine_reason"),
        decay_function: row.get("decay_function"),
    })
}

/// The standard SELECT column list for the memories table.
/// We cast the pgvector `embedding` column to text so we can parse it
/// back into `Vec<f32>` without depending on a pgvector Rust decode path.
const MEMORY_COLUMNS: &str = r#"
    id, agent_id, content, memory_type, scope, importance,
    tags, metadata, embedding::text AS embedding_text,
    content_hash, prev_hash, source_type, source_id,
    consolidation_state, access_count, org_id, thread_id,
    created_at, updated_at, last_accessed_at, expires_at,
    deleted_at, decay_rate, created_by, version, prev_version_id,
    quarantined, quarantine_reason, decay_function
"#;

fn row_to_event(row: &sqlx::postgres::PgRow) -> std::result::Result<AgentEvent, sqlx::Error> {
    let payload: serde_json::Value = row
        .try_get("payload")
        .unwrap_or(serde_json::Value::Null);
    let embedding_blob: Option<Vec<u8>> = row.try_get("embedding").unwrap_or(None);

    Ok(AgentEvent {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        thread_id: row.get("thread_id"),
        run_id: row.get("run_id"),
        parent_event_id: row.get("parent_event_id"),
        event_type: row
            .get::<String, _>("event_type")
            .parse()
            .unwrap_or(mnemo_core::model::event::EventType::Error),
        payload,
        trace_id: row.get("trace_id"),
        span_id: row.get("span_id"),
        model: row.get("model"),
        tokens_input: row.get("tokens_input"),
        tokens_output: row.get("tokens_output"),
        latency_ms: row.get("latency_ms"),
        cost_usd: row.get("cost_usd"),
        timestamp: row.get("timestamp"),
        logical_clock: row.get("logical_clock"),
        content_hash: row.get("content_hash"),
        prev_hash: row.get("prev_hash"),
        embedding: deserialize_embedding(embedding_blob),
    })
}

fn row_to_relation(row: &sqlx::postgres::PgRow) -> std::result::Result<Relation, sqlx::Error> {
    let metadata: serde_json::Value = row
        .try_get("metadata")
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    Ok(Relation {
        id: row.get("id"),
        source_id: row.get("source_id"),
        target_id: row.get("target_id"),
        relation_type: row.get("relation_type"),
        weight: row.get("weight"),
        metadata,
        created_at: row.get("created_at"),
    })
}

fn row_to_checkpoint(
    row: &sqlx::postgres::PgRow,
) -> std::result::Result<Checkpoint, sqlx::Error> {
    let state_snapshot: serde_json::Value = row
        .try_get("state_snapshot")
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let state_diff: Option<serde_json::Value> = row.try_get("state_diff").unwrap_or(None);

    // memory_refs is stored as TEXT[] of UUID strings
    let memory_refs_raw: Vec<String> = row.try_get("memory_refs").unwrap_or_default();
    let memory_refs: Vec<Uuid> = memory_refs_raw
        .iter()
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect();

    let metadata: serde_json::Value = row
        .try_get("metadata")
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    Ok(Checkpoint {
        id: row.get("id"),
        thread_id: row.get("thread_id"),
        agent_id: row.get("agent_id"),
        parent_id: row.get("parent_id"),
        branch_name: row.get("branch_name"),
        state_snapshot,
        state_diff,
        memory_refs,
        event_cursor: row.get("event_cursor"),
        label: row.get("label"),
        created_at: row.get("created_at"),
        metadata,
    })
}

fn row_to_delegation(
    row: &sqlx::postgres::PgRow,
) -> std::result::Result<Delegation, sqlx::Error> {
    let scope_type: String = row.get("scope_type");
    let scope_value: Option<serde_json::Value> = row.try_get("scope_value").unwrap_or(None);

    let scope = match scope_type.as_str() {
        "by_tag" => {
            let tags: Vec<String> = scope_value
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            DelegationScope::ByTag(tags)
        }
        "by_memory_id" => {
            let id_strs: Vec<String> = scope_value
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let uuids = id_strs
                .into_iter()
                .filter_map(|s| Uuid::parse_str(&s).ok())
                .collect();
            DelegationScope::ByMemoryId(uuids)
        }
        _ => DelegationScope::AllMemories,
    };

    Ok(Delegation {
        id: row.get("id"),
        delegator_id: row.get("delegator_id"),
        delegate_id: row.get("delegate_id"),
        permission: row
            .get::<String, _>("permission")
            .parse()
            .unwrap_or(Permission::Read),
        scope,
        max_depth: row.get::<i32, _>("max_depth") as u32,
        current_depth: row.get::<i32, _>("current_depth") as u32,
        parent_delegation_id: row.get("parent_delegation_id"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        revoked_at: row.get("revoked_at"),
    })
}

// ---------------------------------------------------------------------------
// StorageBackend implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl StorageBackend for PgStorage {
    // -----------------------------------------------------------------------
    // Memory CRUD
    // -----------------------------------------------------------------------

    async fn insert_memory(&self, record: &MemoryRecord) -> Result<()> {
        let embedding_param: Option<Vector> = record
            .embedding
            .as_ref()
            .map(|v| Vector::from(v.clone()));

        let tags_slice: &[String] = &record.tags;

        sqlx::query(
            r#"
INSERT INTO memories (
    id, agent_id, content, memory_type, scope, importance,
    tags, metadata, embedding,
    content_hash, prev_hash, source_type, source_id,
    consolidation_state, access_count, org_id, thread_id,
    created_at, updated_at, last_accessed_at, expires_at,
    deleted_at, decay_rate, created_by, version, prev_version_id,
    quarantined, quarantine_reason, decay_function
) VALUES (
    $1, $2, $3, $4, $5, $6,
    $7, $8, $9,
    $10, $11, $12, $13,
    $14, $15, $16, $17,
    $18, $19, $20, $21,
    $22, $23, $24, $25, $26,
    $27, $28, $29
)
"#,
        )
            .bind(record.id)
            .bind(&record.agent_id)
            .bind(&record.content)
            .bind(record.memory_type.to_string())
            .bind(record.scope.to_string())
            .bind(record.importance)
            .bind(tags_slice)
            .bind(&record.metadata)
            .bind(&embedding_param)
            .bind(&record.content_hash)
            .bind(&record.prev_hash)
            .bind(record.source_type.to_string())
            .bind(&record.source_id)
            .bind(record.consolidation_state.to_string())
            .bind(record.access_count as i64)
            .bind(&record.org_id)
            .bind(&record.thread_id)
            .bind(&record.created_at)
            .bind(&record.updated_at)
            .bind(&record.last_accessed_at)
            .bind(&record.expires_at)
            .bind(&record.deleted_at)
            .bind(record.decay_rate)
            .bind(&record.created_by)
            .bind(record.version as i32)
            .bind(record.prev_version_id)
            .bind(record.quarantined)
            .bind(&record.quarantine_reason)
            .bind(&record.decay_function)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;

        Ok(())
    }

    async fn get_memory(&self, id: Uuid) -> Result<Option<MemoryRecord>> {
        let sql = format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE id = $1");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;

        match row {
            Some(r) => Ok(Some(row_to_memory(&r).map_err(map_sqlx)?)),
            None => Ok(None),
        }
    }

    async fn update_memory(&self, record: &MemoryRecord) -> Result<()> {
        let embedding_param: Option<Vector> = record
            .embedding
            .as_ref()
            .map(|v| Vector::from(v.clone()));

        let tags_slice: &[String] = &record.tags;

        let result = sqlx::query(
            r#"
UPDATE memories SET
    agent_id = $1, content = $2, memory_type = $3, scope = $4,
    importance = $5, tags = $6, metadata = $7,
    embedding = $8,
    content_hash = $9, prev_hash = $10, source_type = $11,
    source_id = $12, consolidation_state = $13, access_count = $14,
    org_id = $15, thread_id = $16, updated_at = $17,
    last_accessed_at = $18, expires_at = $19, deleted_at = $20,
    decay_rate = $21, created_by = $22, version = $23,
    prev_version_id = $24, quarantined = $25, quarantine_reason = $26,
    decay_function = $27
WHERE id = $28
"#,
        )
            .bind(&record.agent_id)
            .bind(&record.content)
            .bind(record.memory_type.to_string())
            .bind(record.scope.to_string())
            .bind(record.importance)
            .bind(tags_slice)
            .bind(&record.metadata)
            .bind(&embedding_param)
            .bind(&record.content_hash)
            .bind(&record.prev_hash)
            .bind(record.source_type.to_string())
            .bind(&record.source_id)
            .bind(record.consolidation_state.to_string())
            .bind(record.access_count as i64)
            .bind(&record.org_id)
            .bind(&record.thread_id)
            .bind(&record.updated_at)
            .bind(&record.last_accessed_at)
            .bind(&record.expires_at)
            .bind(&record.deleted_at)
            .bind(record.decay_rate)
            .bind(&record.created_by)
            .bind(record.version as i32)
            .bind(record.prev_version_id)
            .bind(record.quarantined)
            .bind(&record.quarantine_reason)
            .bind(&record.decay_function)
            .bind(record.id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("memory {} not found", record.id)));
        }
        Ok(())
    }

    async fn soft_delete_memory(&self, id: Uuid) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE memories SET deleted_at = $1, updated_at = $2 WHERE id = $3 AND deleted_at IS NULL",
        )
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!(
                "memory {id} not found or already deleted"
            )));
        }
        Ok(())
    }

    async fn hard_delete_memory(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM memories WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("memory {id} not found")));
        }

        // Clean up ACLs for this memory
        sqlx::query("DELETE FROM acls WHERE memory_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;

        Ok(())
    }

    async fn list_memories(
        &self,
        filter: &MemoryFilter,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<MemoryRecord>> {
        let mut conditions: Vec<String> = Vec::new();
        // We'll track bind-parameter index manually.
        // The MEMORY_COLUMNS select doesn't use numbered params.
        let mut param_idx: usize = 0;

        // We accumulate bind values in a specific order and push them later
        // via a dynamic query builder. Unfortunately sqlx's dynamic queries
        // require us to build the SQL string with numbered placeholders and
        // bind all values in order.

        // We'll collect (sql_fragment, value_type) tuples, then bind them.
        // Use a simpler approach: build the query string, then bind
        // parameters positionally.

        if !filter.include_deleted {
            conditions.push("deleted_at IS NULL".to_string());
        }

        // We'll use an enum-based approach below to track what to bind.
        #[derive(Debug)]
        enum Param {
            Str(String),
            F32(f32),
        }
        let mut params: Vec<Param> = Vec::new();

        if let Some(ref agent_id) = filter.agent_id {
            param_idx += 1;
            conditions.push(format!("agent_id = ${param_idx}"));
            params.push(Param::Str(agent_id.clone()));
        }
        if let Some(memory_type) = filter.memory_type {
            param_idx += 1;
            conditions.push(format!("memory_type = ${param_idx}"));
            params.push(Param::Str(memory_type.to_string()));
        }
        if let Some(scope) = filter.scope {
            param_idx += 1;
            conditions.push(format!("scope = ${param_idx}"));
            params.push(Param::Str(scope.to_string()));
        }
        if let Some(min_importance) = filter.min_importance {
            param_idx += 1;
            conditions.push(format!("importance >= ${param_idx}"));
            params.push(Param::F32(min_importance));
        }
        if let Some(ref org_id) = filter.org_id {
            param_idx += 1;
            conditions.push(format!("org_id = ${param_idx}"));
            params.push(Param::Str(org_id.clone()));
        }
        if let Some(ref thread_id) = filter.thread_id {
            param_idx += 1;
            conditions.push(format!("thread_id = ${param_idx}"));
            params.push(Param::Str(thread_id.clone()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT {MEMORY_COLUMNS} FROM memories {where_clause} ORDER BY created_at DESC LIMIT {limit} OFFSET {offset}"
        );

        let mut query = sqlx::query(&sql);
        for p in &params {
            match p {
                Param::Str(s) => query = query.bind(s),
                Param::F32(f) => query = query.bind(*f),
            }
        }

        let rows = query.fetch_all(&self.pool).await.map_err(map_sqlx)?;
        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_memory(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    async fn touch_memory(&self, id: Uuid) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE memories SET access_count = access_count + 1, last_accessed_at = $1 WHERE id = $2",
        )
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // ACL
    // -----------------------------------------------------------------------

    async fn insert_acl(&self, acl: &Acl) -> Result<()> {
        sqlx::query(
            r#"
INSERT INTO acls (id, memory_id, principal_type, principal_id, permission, granted_by, created_at, expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
"#,
        )
        .bind(acl.id)
        .bind(acl.memory_id)
        .bind(acl.principal_type.to_string())
        .bind(&acl.principal_id)
        .bind(acl.permission.to_string())
        .bind(&acl.granted_by)
        .bind(&acl.created_at)
        .bind(&acl.expires_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn check_permission(
        &self,
        memory_id: Uuid,
        principal_id: &str,
        required: Permission,
    ) -> Result<bool> {
        // Check if the principal is the owner
        let owner_row = sqlx::query("SELECT agent_id FROM memories WHERE id = $1")
            .bind(memory_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;

        match owner_row {
            None => return Err(Error::NotFound(format!("memory {memory_id} not found"))),
            Some(row) => {
                let owner: String = row.get("agent_id");
                if owner == principal_id {
                    return Ok(true);
                }
            }
        }

        // Check ACLs (direct grants)
        let now = chrono::Utc::now().to_rfc3339();
        let acl_rows = sqlx::query(
            "SELECT permission FROM acls WHERE memory_id = $1 AND principal_id = $2 AND (expires_at IS NULL OR expires_at > $3)",
        )
        .bind(memory_id)
        .bind(principal_id)
        .bind(&now)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        for row in &acl_rows {
            let perm_str: String = row.get("permission");
            if let Ok(perm) = perm_str.parse::<Permission>() {
                if perm.satisfies(required) {
                    return Ok(true);
                }
            }
        }

        // Check public ACLs
        let public_rows = sqlx::query(
            "SELECT permission FROM acls WHERE memory_id = $1 AND principal_type = 'public' AND (expires_at IS NULL OR expires_at > $2)",
        )
        .bind(memory_id)
        .bind(&now)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        for row in &public_rows {
            let perm_str: String = row.get("permission");
            if let Ok(perm) = perm_str.parse::<Permission>() {
                if perm.satisfies(required) {
                    return Ok(true);
                }
            }
        }

        // Check delegations
        if self
            .check_delegation(principal_id, memory_id, required)
            .await?
        {
            return Ok(true);
        }

        Ok(false)
    }

    // -----------------------------------------------------------------------
    // Relations
    // -----------------------------------------------------------------------

    async fn insert_relation(&self, relation: &Relation) -> Result<()> {
        sqlx::query(
            r#"
INSERT INTO relations (id, source_id, target_id, relation_type, weight, metadata, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
"#,
        )
        .bind(relation.id)
        .bind(relation.source_id)
        .bind(relation.target_id)
        .bind(&relation.relation_type)
        .bind(relation.weight)
        .bind(&relation.metadata)
        .bind(&relation.created_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn get_relations_from(&self, source_id: Uuid) -> Result<Vec<Relation>> {
        let rows = sqlx::query(
            "SELECT id, source_id, target_id, relation_type, weight, metadata, created_at FROM relations WHERE source_id = $1",
        )
        .bind(source_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_relation(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    async fn get_relations_to(&self, target_id: Uuid) -> Result<Vec<Relation>> {
        let rows = sqlx::query(
            "SELECT id, source_id, target_id, relation_type, weight, metadata, created_at FROM relations WHERE target_id = $1",
        )
        .bind(target_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_relation(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    async fn delete_relation(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM relations WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("relation {id} not found")));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Chain linking
    // -----------------------------------------------------------------------

    async fn get_latest_memory_hash(
        &self,
        agent_id: &str,
        thread_id: Option<&str>,
    ) -> Result<Option<Vec<u8>>> {
        let row = if let Some(tid) = thread_id {
            sqlx::query(
                "SELECT content_hash FROM memories WHERE agent_id = $1 AND thread_id = $2 AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
            )
            .bind(agent_id)
            .bind(tid)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?
        } else {
            sqlx::query(
                "SELECT content_hash FROM memories WHERE agent_id = $1 AND thread_id IS NULL AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
            )
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?
        };

        Ok(row.map(|r| r.get::<Vec<u8>, _>("content_hash")))
    }

    async fn get_latest_event_hash(
        &self,
        agent_id: &str,
        thread_id: Option<&str>,
    ) -> Result<Option<Vec<u8>>> {
        let row = if let Some(tid) = thread_id {
            sqlx::query(
                "SELECT content_hash FROM agent_events WHERE agent_id = $1 AND thread_id = $2 ORDER BY timestamp DESC LIMIT 1",
            )
            .bind(agent_id)
            .bind(tid)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?
        } else {
            sqlx::query(
                "SELECT content_hash FROM agent_events WHERE agent_id = $1 ORDER BY timestamp DESC LIMIT 1",
            )
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?
        };
        Ok(row.map(|r| r.get::<Vec<u8>, _>("content_hash")))
    }

    async fn get_sync_watermark(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM sync_metadata WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    async fn set_sync_watermark(&self, key: &str, value: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO sync_metadata (key, value, updated_at) VALUES ($1, $2, $3) ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = $3",
        )
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Permission-safe ANN
    // -----------------------------------------------------------------------

    async fn list_accessible_memory_ids(
        &self,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<Uuid>> {
        let now = chrono::Utc::now().to_rfc3339();
        let rows = sqlx::query(
            r#"
SELECT id FROM memories
WHERE (
    agent_id = $1
    OR scope = 'public'
    OR id IN (
        SELECT memory_id FROM acls
        WHERE principal_id = $2 AND (expires_at IS NULL OR expires_at > $3)
    )
)
AND deleted_at IS NULL
LIMIT $4
"#,
        )
        .bind(agent_id)
        .bind(agent_id)
        .bind(&now)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let ids: Vec<Uuid> = rows.iter().map(|r| r.get("id")).collect();
        Ok(ids)
    }

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    async fn insert_event(&self, event: &AgentEvent) -> Result<()> {
        let payload_json = &event.payload;
        let embedding_blob = serialize_embedding(&event.embedding);

        sqlx::query(
            r#"
INSERT INTO agent_events (
    id, agent_id, thread_id, run_id, parent_event_id, event_type,
    payload, trace_id, span_id, model, tokens_input, tokens_output,
    latency_ms, cost_usd, "timestamp", logical_clock, content_hash,
    prev_hash, embedding
) VALUES (
    $1, $2, $3, $4, $5, $6,
    $7, $8, $9, $10, $11, $12,
    $13, $14, $15, $16, $17,
    $18, $19
)
"#,
        )
        .bind(event.id)
        .bind(&event.agent_id)
        .bind(&event.thread_id)
        .bind(&event.run_id)
        .bind(event.parent_event_id)
        .bind(event.event_type.to_string())
        .bind(payload_json)
        .bind(&event.trace_id)
        .bind(&event.span_id)
        .bind(&event.model)
        .bind(event.tokens_input)
        .bind(event.tokens_output)
        .bind(event.latency_ms)
        .bind(event.cost_usd)
        .bind(&event.timestamp)
        .bind(event.logical_clock)
        .bind(&event.content_hash)
        .bind(&event.prev_hash)
        .bind(&embedding_blob)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn list_events(
        &self,
        agent_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<AgentEvent>> {
        let rows = sqlx::query(
            r#"
SELECT id, agent_id, thread_id, run_id, parent_event_id, event_type,
       payload, trace_id, span_id, model, tokens_input, tokens_output,
       latency_ms, cost_usd, "timestamp", logical_clock, content_hash,
       prev_hash, embedding
FROM agent_events
WHERE agent_id = $1
ORDER BY "timestamp" DESC
LIMIT $2 OFFSET $3
"#,
        )
        .bind(agent_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_event(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    async fn get_events_by_thread(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<AgentEvent>> {
        let rows = sqlx::query(
            r#"
SELECT id, agent_id, thread_id, run_id, parent_event_id, event_type,
       payload, trace_id, span_id, model, tokens_input, tokens_output,
       latency_ms, cost_usd, "timestamp", logical_clock, content_hash,
       prev_hash, embedding
FROM agent_events
WHERE thread_id = $1
ORDER BY "timestamp" ASC
LIMIT $2
"#,
        )
        .bind(thread_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_event(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    async fn get_event(&self, id: Uuid) -> Result<Option<AgentEvent>> {
        let row = sqlx::query(
            r#"
SELECT id, agent_id, thread_id, run_id, parent_event_id, event_type,
       payload, trace_id, span_id, model, tokens_input, tokens_output,
       latency_ms, cost_usd, "timestamp", logical_clock, content_hash,
       prev_hash, embedding
FROM agent_events
WHERE id = $1
"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        match row {
            Some(r) => Ok(Some(row_to_event(&r).map_err(map_sqlx)?)),
            None => Ok(None),
        }
    }

    async fn list_child_events(
        &self,
        parent_event_id: Uuid,
        limit: usize,
    ) -> Result<Vec<AgentEvent>> {
        let rows = sqlx::query(
            r#"
SELECT id, agent_id, thread_id, run_id, parent_event_id, event_type,
       payload, trace_id, span_id, model, tokens_input, tokens_output,
       latency_ms, cost_usd, "timestamp", logical_clock, content_hash,
       prev_hash, embedding
FROM agent_events
WHERE parent_event_id = $1
ORDER BY "timestamp" ASC
LIMIT $2
"#,
        )
        .bind(parent_event_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_event(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    // -----------------------------------------------------------------------
    // Ordered listing
    // -----------------------------------------------------------------------

    async fn list_memories_by_agent_ordered(
        &self,
        agent_id: &str,
        thread_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        let rows = if let Some(tid) = thread_id {
            let sql = format!(
                "SELECT {MEMORY_COLUMNS} FROM memories WHERE agent_id = $1 AND thread_id = $2 AND deleted_at IS NULL ORDER BY created_at ASC LIMIT $3"
            );
            sqlx::query(&sql)
                .bind(agent_id)
                .bind(tid)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(map_sqlx)?
        } else {
            let sql = format!(
                "SELECT {MEMORY_COLUMNS} FROM memories WHERE agent_id = $1 AND deleted_at IS NULL ORDER BY created_at ASC LIMIT $2"
            );
            sqlx::query(&sql)
                .bind(agent_id)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(map_sqlx)?
        };

        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_memory(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    // -----------------------------------------------------------------------
    // Sync support
    // -----------------------------------------------------------------------

    async fn list_memories_since(
        &self,
        updated_after: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        let sql = format!(
            "SELECT {MEMORY_COLUMNS} FROM memories WHERE updated_at > $1 ORDER BY updated_at ASC LIMIT $2"
        );
        let rows = sqlx::query(&sql)
            .bind(updated_after)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;

        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_memory(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    async fn upsert_memory(&self, record: &MemoryRecord) -> Result<()> {
        match self.update_memory(record).await {
            Ok(()) => Ok(()),
            Err(Error::NotFound(_)) => self.insert_memory(record).await,
            Err(e) => Err(e),
        }
    }

    // -----------------------------------------------------------------------
    // Expired memory cleanup
    // -----------------------------------------------------------------------

    async fn cleanup_expired(&self) -> Result<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE memories SET deleted_at = $1 WHERE expires_at IS NOT NULL AND expires_at < $2 AND deleted_at IS NULL",
        )
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(result.rows_affected() as usize)
    }

    // -----------------------------------------------------------------------
    // Delegations
    // -----------------------------------------------------------------------

    async fn insert_delegation(&self, d: &Delegation) -> Result<()> {
        let scope_type = d.scope.to_string();
        let scope_value: serde_json::Value = match &d.scope {
            DelegationScope::AllMemories => serde_json::Value::Null,
            DelegationScope::ByTag(tags) => serde_json::json!(tags),
            DelegationScope::ByMemoryId(ids) => {
                serde_json::json!(ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
            }
        };

        sqlx::query(
            r#"
INSERT INTO delegations (
    id, delegator_id, delegate_id, permission, scope_type, scope_value,
    max_depth, current_depth, parent_delegation_id,
    created_at, expires_at, revoked_at
) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
"#,
        )
        .bind(d.id)
        .bind(&d.delegator_id)
        .bind(&d.delegate_id)
        .bind(d.permission.to_string())
        .bind(&scope_type)
        .bind(&scope_value)
        .bind(d.max_depth as i32)
        .bind(d.current_depth as i32)
        .bind(d.parent_delegation_id)
        .bind(&d.created_at)
        .bind(&d.expires_at)
        .bind(&d.revoked_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn list_delegations_for(&self, delegate_id: &str) -> Result<Vec<Delegation>> {
        let now = chrono::Utc::now().to_rfc3339();
        let rows = sqlx::query(
            r#"
SELECT id, delegator_id, delegate_id, permission, scope_type, scope_value,
       max_depth, current_depth, parent_delegation_id,
       created_at, expires_at, revoked_at
FROM delegations
WHERE delegate_id = $1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > $2)
"#,
        )
        .bind(delegate_id)
        .bind(&now)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_delegation(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    async fn revoke_delegation(&self, id: Uuid) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE delegations SET revoked_at = $1 WHERE id = $2 AND revoked_at IS NULL",
        )
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!(
                "delegation {id} not found or already revoked"
            )));
        }
        Ok(())
    }

    async fn check_delegation(
        &self,
        delegate_id: &str,
        memory_id: Uuid,
        required: Permission,
    ) -> Result<bool> {
        let delegations = self.list_delegations_for(delegate_id).await?;

        // Get the memory to inspect its tags for scope matching
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

    // -----------------------------------------------------------------------
    // Agent Profiles
    // -----------------------------------------------------------------------

    async fn insert_or_update_agent_profile(&self, profile: &AgentProfile) -> Result<()> {
        sqlx::query(
            r#"
INSERT INTO agent_profiles (agent_id, avg_importance, avg_content_length, total_memories, last_updated)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (agent_id) DO UPDATE SET
    avg_importance = EXCLUDED.avg_importance,
    avg_content_length = EXCLUDED.avg_content_length,
    total_memories = EXCLUDED.total_memories,
    last_updated = EXCLUDED.last_updated
"#,
        )
        .bind(&profile.agent_id)
        .bind(profile.avg_importance)
        .bind(profile.avg_content_length)
        .bind(profile.total_memories as i64)
        .bind(&profile.last_updated)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn get_agent_profile(&self, agent_id: &str) -> Result<Option<AgentProfile>> {
        let row = sqlx::query(
            "SELECT agent_id, avg_importance, avg_content_length, total_memories, last_updated FROM agent_profiles WHERE agent_id = $1",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(row.map(|r| AgentProfile {
            agent_id: r.get("agent_id"),
            avg_importance: r.get("avg_importance"),
            avg_content_length: r.get("avg_content_length"),
            total_memories: r.get::<i64, _>("total_memories") as u64,
            last_updated: r.get("last_updated"),
        }))
    }

    // -----------------------------------------------------------------------
    // Checkpoints
    // -----------------------------------------------------------------------

    async fn insert_checkpoint(&self, cp: &Checkpoint) -> Result<()> {
        let memory_refs_strs: Vec<String> =
            cp.memory_refs.iter().map(|id| id.to_string()).collect();
        let refs_slice: &[String] = &memory_refs_strs;

        sqlx::query(
            r#"
INSERT INTO checkpoints (
    id, thread_id, agent_id, parent_id, branch_name,
    state_snapshot, state_diff, memory_refs, event_cursor,
    label, created_at, metadata
) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
"#,
        )
        .bind(cp.id)
        .bind(&cp.thread_id)
        .bind(&cp.agent_id)
        .bind(cp.parent_id)
        .bind(&cp.branch_name)
        .bind(&cp.state_snapshot)
        .bind(&cp.state_diff)
        .bind(refs_slice)
        .bind(cp.event_cursor)
        .bind(&cp.label)
        .bind(&cp.created_at)
        .bind(&cp.metadata)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn get_checkpoint(&self, id: Uuid) -> Result<Option<Checkpoint>> {
        let row = sqlx::query(
            r#"
SELECT id, thread_id, agent_id, parent_id, branch_name,
       state_snapshot, state_diff, memory_refs, event_cursor,
       label, created_at, metadata
FROM checkpoints WHERE id = $1
"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        match row {
            Some(r) => Ok(Some(row_to_checkpoint(&r).map_err(map_sqlx)?)),
            None => Ok(None),
        }
    }

    async fn list_checkpoints(
        &self,
        thread_id: &str,
        branch: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Checkpoint>> {
        let rows = if let Some(branch_name) = branch {
            sqlx::query(
                r#"
SELECT id, thread_id, agent_id, parent_id, branch_name,
       state_snapshot, state_diff, memory_refs, event_cursor,
       label, created_at, metadata
FROM checkpoints
WHERE thread_id = $1 AND branch_name = $2
ORDER BY created_at DESC
LIMIT $3
"#,
            )
            .bind(thread_id)
            .bind(branch_name)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?
        } else {
            sqlx::query(
                r#"
SELECT id, thread_id, agent_id, parent_id, branch_name,
       state_snapshot, state_diff, memory_refs, event_cursor,
       label, created_at, metadata
FROM checkpoints
WHERE thread_id = $1
ORDER BY created_at DESC
LIMIT $2
"#,
            )
            .bind(thread_id)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?
        };

        let mut results = Vec::with_capacity(rows.len());
        for r in &rows {
            results.push(row_to_checkpoint(r).map_err(map_sqlx)?);
        }
        Ok(results)
    }

    async fn get_latest_checkpoint(
        &self,
        thread_id: &str,
        branch: &str,
    ) -> Result<Option<Checkpoint>> {
        let row = sqlx::query(
            r#"
SELECT id, thread_id, agent_id, parent_id, branch_name,
       state_snapshot, state_diff, memory_refs, event_cursor,
       label, created_at, metadata
FROM checkpoints
WHERE thread_id = $1 AND branch_name = $2
ORDER BY created_at DESC
LIMIT 1
"#,
        )
        .bind(thread_id)
        .bind(branch)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        match row {
            Some(r) => Ok(Some(row_to_checkpoint(&r).map_err(map_sqlx)?)),
            None => Ok(None),
        }
    }
}
