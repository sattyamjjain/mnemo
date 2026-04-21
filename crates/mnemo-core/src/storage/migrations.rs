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

/// Schema version stamp table. One row per database file, populated on
/// first `run_migrations` call. A missing row on an existing database
/// indicates a pre-0.3.1 file and is treated as version 1.
pub const CREATE_MNEMO_META_TABLE: &str = "
CREATE TABLE IF NOT EXISTS mnemo_meta (
    key VARCHAR PRIMARY KEY,
    value VARCHAR NOT NULL,
    updated_at VARCHAR NOT NULL DEFAULT CURRENT_TIMESTAMP
);
";

/// Persistence format version this release writes. Bump when the on-disk
/// schema changes in a way that requires a migrator pass.
pub const CURRENT_PERSISTENCE_VERSION: u32 = 3;

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
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_events_parent ON agent_events(parent_event_id)",
        [],
    );
    // Sprint 8: sync watermarks table
    conn.execute_batch(CREATE_SYNC_METADATA_TABLE)?;
    // v0.3.2: persistence-version stamp.
    conn.execute_batch(CREATE_MNEMO_META_TABLE)?;
    stamp_persistence_version(conn)?;
    Ok(())
}

/// Read the stored persistence version, or `None` if the marker is absent
/// (i.e. this is a fresh database OR a pre-0.3.2 file).
pub fn read_persistence_version(conn: &duckdb::Connection) -> duckdb::Result<Option<u32>> {
    let mut stmt =
        conn.prepare("SELECT value FROM mnemo_meta WHERE key = 'persistence_version'")?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        Ok(raw.parse::<u32>().ok())
    } else {
        Ok(None)
    }
}

/// Write / update the persistence version stamp. Called at the end of
/// `run_migrations` after every schema operation has succeeded.
///
/// * If the stamp is missing, this is either a fresh DB or a pre-0.3.2
///   file. Either way the post-run schema is the current one, so we
///   write `CURRENT_PERSISTENCE_VERSION`.
/// * If the stamp is older than `CURRENT_PERSISTENCE_VERSION`, we've
///   just run a migrator over a legacy file; update to current.
/// * If the stamp is already current, no-op.
fn stamp_persistence_version(conn: &duckdb::Connection) -> duckdb::Result<()> {
    let existing = read_persistence_version(conn)?;
    let current = CURRENT_PERSISTENCE_VERSION;
    if let Some(v) = existing
        && v == current
    {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    // DuckDB's ON CONFLICT parser is picky with DEFAULT columns; drive the
    // updated_at value from Rust explicitly instead.
    conn.execute(
        "DELETE FROM mnemo_meta WHERE key = 'persistence_version'",
        [],
    )?;
    conn.execute(
        "INSERT INTO mnemo_meta(key, value, updated_at) VALUES ('persistence_version', ?, ?)",
        duckdb::params![current.to_string(), now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_db_stamps_current_persistence_version() {
        let conn = duckdb::Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let v = read_persistence_version(&conn).unwrap();
        assert_eq!(v, Some(CURRENT_PERSISTENCE_VERSION));
    }

    /// A "legacy" database (no mnemo_meta row) must get stamped to the
    /// current version the first time run_migrations sees it. Subsequent
    /// passes are no-ops. This mirrors what will happen when a v0.1.1
    /// DuckDB file is opened by a v0.3.2 reader.
    #[test]
    fn test_legacy_db_gets_stamped_on_open() {
        let conn = duckdb::Connection::open_in_memory().unwrap();
        // Simulate a pre-0.3.2 file: create every table EXCEPT mnemo_meta.
        conn.execute_batch(CREATE_MEMORIES_TABLE).unwrap();
        conn.execute_batch(CREATE_ACLS_TABLE).unwrap();
        conn.execute_batch(CREATE_RELATIONS_TABLE).unwrap();
        conn.execute_batch(CREATE_AGENT_EVENTS_TABLE).unwrap();
        conn.execute_batch(CREATE_CHECKPOINTS_TABLE).unwrap();
        conn.execute_batch(CREATE_DELEGATIONS_TABLE).unwrap();
        conn.execute_batch(CREATE_AGENT_PROFILES_TABLE).unwrap();
        conn.execute_batch(CREATE_SYNC_METADATA_TABLE).unwrap();

        assert!(
            read_persistence_version(&conn).is_err()
                || read_persistence_version(&conn).unwrap().is_none(),
            "pre-migration legacy file should have no stamp"
        );

        run_migrations(&conn).unwrap();
        assert_eq!(
            read_persistence_version(&conn).unwrap(),
            Some(CURRENT_PERSISTENCE_VERSION)
        );

        // Second pass is a no-op.
        run_migrations(&conn).unwrap();
        assert_eq!(
            read_persistence_version(&conn).unwrap(),
            Some(CURRENT_PERSISTENCE_VERSION)
        );
    }

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
