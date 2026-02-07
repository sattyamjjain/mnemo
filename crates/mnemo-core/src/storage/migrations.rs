pub const CREATE_MEMORIES_TABLE: &str = "
CREATE TABLE IF NOT EXISTS memories (
    id VARCHAR PRIMARY KEY,
    agent_id VARCHAR NOT NULL,
    content TEXT NOT NULL,
    memory_type VARCHAR NOT NULL,
    scope VARCHAR NOT NULL DEFAULT 'private',
    importance FLOAT NOT NULL DEFAULT 0.5,
    tags JSON,
    metadata JSON,
    embedding BLOB,
    content_hash BLOB NOT NULL,
    prev_hash BLOB,
    source_type VARCHAR NOT NULL DEFAULT 'agent',
    source_id VARCHAR,
    consolidation_state VARCHAR NOT NULL DEFAULT 'raw',
    access_count BIGINT NOT NULL DEFAULT 0,
    org_id VARCHAR,
    thread_id VARCHAR,
    created_at VARCHAR NOT NULL,
    updated_at VARCHAR NOT NULL,
    last_accessed_at VARCHAR,
    expires_at VARCHAR,
    deleted_at VARCHAR,
    decay_rate FLOAT,
    created_by VARCHAR,
    version INTEGER NOT NULL DEFAULT 1,
    prev_version_id VARCHAR,
    quarantined BOOLEAN NOT NULL DEFAULT false,
    quarantine_reason VARCHAR,
    decay_function VARCHAR
);
CREATE INDEX IF NOT EXISTS idx_memories_agent_id ON memories(agent_id);
CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);
CREATE INDEX IF NOT EXISTS idx_memories_memory_type ON memories(memory_type);
CREATE INDEX IF NOT EXISTS idx_memories_org_id ON memories(org_id);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at);
CREATE INDEX IF NOT EXISTS idx_memories_deleted_at ON memories(deleted_at);
CREATE INDEX IF NOT EXISTS idx_memories_thread_id ON memories(thread_id);
CREATE INDEX IF NOT EXISTS idx_memories_expires_at ON memories(expires_at);
CREATE INDEX IF NOT EXISTS idx_memories_consolidation_state ON memories(consolidation_state);
";

pub const CREATE_ACLS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS acls (
    id VARCHAR PRIMARY KEY,
    memory_id VARCHAR NOT NULL,
    principal_type VARCHAR NOT NULL,
    principal_id VARCHAR NOT NULL,
    permission VARCHAR NOT NULL,
    granted_by VARCHAR NOT NULL,
    created_at VARCHAR NOT NULL,
    expires_at VARCHAR
);
CREATE INDEX IF NOT EXISTS idx_acls_memory_id ON acls(memory_id);
CREATE INDEX IF NOT EXISTS idx_acls_principal ON acls(principal_type, principal_id);
";

pub const CREATE_RELATIONS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS relations (
    id VARCHAR PRIMARY KEY,
    source_id VARCHAR NOT NULL,
    target_id VARCHAR NOT NULL,
    relation_type VARCHAR NOT NULL,
    weight FLOAT NOT NULL DEFAULT 1.0,
    metadata JSON,
    created_at VARCHAR NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_id);
CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_id);
";

// NOTE: agent_events is append-only by design. DuckDB lacks trigger support,
// so enforcement is application-level. The PostgreSQL backend uses a
// BEFORE UPDATE OR DELETE trigger (prevent_event_modification) to enforce
// this at the schema level. Application code must never UPDATE or DELETE
// rows from this table.
pub const CREATE_AGENT_EVENTS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS agent_events (
    id VARCHAR PRIMARY KEY,
    agent_id VARCHAR NOT NULL,
    thread_id VARCHAR,
    run_id VARCHAR,
    parent_event_id VARCHAR,
    event_type VARCHAR NOT NULL,
    payload JSON,
    trace_id VARCHAR,
    span_id VARCHAR,
    model VARCHAR,
    tokens_input BIGINT,
    tokens_output BIGINT,
    latency_ms BIGINT,
    cost_usd DOUBLE,
    timestamp VARCHAR NOT NULL,
    logical_clock BIGINT NOT NULL DEFAULT 0,
    content_hash BLOB NOT NULL,
    prev_hash BLOB,
    embedding BLOB
);
CREATE INDEX IF NOT EXISTS idx_events_agent_id ON agent_events(agent_id);
CREATE INDEX IF NOT EXISTS idx_events_thread_id ON agent_events(thread_id);
CREATE INDEX IF NOT EXISTS idx_events_event_type ON agent_events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON agent_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_trace_id ON agent_events(trace_id);
CREATE INDEX IF NOT EXISTS idx_events_parent ON agent_events(parent_event_id);
";

pub const CREATE_CHECKPOINTS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS checkpoints (
    id VARCHAR PRIMARY KEY,
    thread_id VARCHAR NOT NULL,
    agent_id VARCHAR NOT NULL,
    parent_id VARCHAR,
    branch_name VARCHAR NOT NULL DEFAULT 'main',
    state_snapshot JSON,
    state_diff JSON,
    memory_refs JSON,
    event_cursor VARCHAR,
    label VARCHAR,
    created_at VARCHAR NOT NULL,
    metadata JSON
);
CREATE INDEX IF NOT EXISTS idx_checkpoints_thread_id ON checkpoints(thread_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_branch ON checkpoints(thread_id, branch_name);
CREATE INDEX IF NOT EXISTS idx_checkpoints_agent ON checkpoints(agent_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_created_at ON checkpoints(created_at);
";

// Sprint 3 ALTER TABLE migrations for upgrading existing databases.
// New databases already have these columns in CREATE TABLE.
// DuckDB doesn't support ADD COLUMN IF NOT EXISTS, so we skip ALTER for fresh DBs.
// These are only needed when upgrading from Sprint 2 databases.
pub const SPRINT3_COLUMN_ALTERS: &[&str] = &[
    "ALTER TABLE memories ADD COLUMN decay_rate FLOAT",
    "ALTER TABLE memories ADD COLUMN created_by VARCHAR",
    "ALTER TABLE memories ADD COLUMN version INTEGER DEFAULT 1",
    "ALTER TABLE memories ADD COLUMN prev_version_id VARCHAR",
    "ALTER TABLE memories ADD COLUMN quarantined BOOLEAN DEFAULT false",
    "ALTER TABLE memories ADD COLUMN quarantine_reason VARCHAR",
];

// Sprint 4 migrations for event embeddings and custom decay functions
pub const SPRINT4_COLUMN_ALTERS: &[&str] = &[
    "ALTER TABLE agent_events ADD COLUMN embedding BLOB",
    "ALTER TABLE memories ADD COLUMN decay_function VARCHAR",
];

pub const CREATE_DELEGATIONS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS delegations (
    id VARCHAR PRIMARY KEY,
    delegator_id VARCHAR NOT NULL,
    delegate_id VARCHAR NOT NULL,
    permission VARCHAR NOT NULL,
    scope_type VARCHAR NOT NULL DEFAULT 'all_memories',
    scope_value JSON,
    max_depth INTEGER NOT NULL DEFAULT 0,
    current_depth INTEGER NOT NULL DEFAULT 0,
    parent_delegation_id VARCHAR,
    created_at VARCHAR NOT NULL,
    expires_at VARCHAR,
    revoked_at VARCHAR
);
CREATE INDEX IF NOT EXISTS idx_delegations_delegator ON delegations(delegator_id);
CREATE INDEX IF NOT EXISTS idx_delegations_delegate ON delegations(delegate_id);
";

pub const CREATE_AGENT_PROFILES_TABLE: &str = "
CREATE TABLE IF NOT EXISTS agent_profiles (
    agent_id VARCHAR PRIMARY KEY,
    avg_importance DOUBLE NOT NULL DEFAULT 0.5,
    avg_content_length DOUBLE NOT NULL DEFAULT 100,
    total_memories BIGINT NOT NULL DEFAULT 0,
    last_updated VARCHAR NOT NULL
);
";

pub const CREATE_SYNC_METADATA_TABLE: &str = "
CREATE TABLE IF NOT EXISTS sync_metadata (
    key VARCHAR PRIMARY KEY,
    value VARCHAR NOT NULL,
    updated_at VARCHAR NOT NULL DEFAULT CURRENT_TIMESTAMP
);
";

pub fn run_migrations(conn: &duckdb::Connection) -> duckdb::Result<()> {
    conn.execute_batch(CREATE_MEMORIES_TABLE)?;
    conn.execute_batch(CREATE_ACLS_TABLE)?;
    conn.execute_batch(CREATE_RELATIONS_TABLE)?;
    conn.execute_batch(CREATE_AGENT_EVENTS_TABLE)?;
    conn.execute_batch(CREATE_CHECKPOINTS_TABLE)?;
    // Sprint 3 column upgrades — silently ignore if columns already exist
    for alter_sql in SPRINT3_COLUMN_ALTERS {
        let _ = conn.execute(alter_sql, []);
    }
    conn.execute_batch(CREATE_DELEGATIONS_TABLE)?;
    conn.execute_batch(CREATE_AGENT_PROFILES_TABLE)?;
    // Sprint 4 column upgrades — silently ignore if columns already exist
    for alter_sql in SPRINT4_COLUMN_ALTERS {
        let _ = conn.execute(alter_sql, []);
    }
    // Create parent_event_id index if missing
    let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_events_parent ON agent_events(parent_event_id)", []);
    // Sprint 8: sync watermarks table
    conn.execute_batch(CREATE_SYNC_METADATA_TABLE)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_run_on_in_memory_db() {
        let conn = duckdb::Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        // Verify tables exist by querying them
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM memories").unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);

        let mut stmt = conn.prepare("SELECT COUNT(*) FROM agent_events").unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);

        let mut stmt = conn.prepare("SELECT COUNT(*) FROM checkpoints").unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }
}
