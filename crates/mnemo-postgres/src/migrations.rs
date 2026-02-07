use mnemo_core::error::{Error, Result};

/// Run all PostgreSQL schema migrations.
///
/// The `dimensions` parameter controls the width of the pgvector `vector` column
/// on the `memories` table (e.g. 1536 for OpenAI ada-002 embeddings).
pub async fn run_migrations(pool: &sqlx::PgPool, dimensions: usize) -> Result<()> {
    // Enable pgvector extension
    sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
        .execute(pool)
        .await
        .map_err(|e| Error::Storage(format!("failed to create vector extension: {e}")))?;

    // 1. memories
    let create_memories = format!(
        r#"
CREATE TABLE IF NOT EXISTS memories (
    id UUID PRIMARY KEY,
    agent_id VARCHAR NOT NULL,
    content TEXT NOT NULL,
    memory_type VARCHAR NOT NULL,
    scope VARCHAR NOT NULL DEFAULT 'private',
    importance REAL NOT NULL DEFAULT 0.5,
    tags TEXT[],
    metadata JSONB,
    embedding vector({dimensions}),
    content_hash BYTEA NOT NULL,
    prev_hash BYTEA,
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
    decay_rate REAL,
    created_by VARCHAR,
    version INTEGER NOT NULL DEFAULT 1,
    prev_version_id UUID,
    quarantined BOOLEAN NOT NULL DEFAULT FALSE,
    quarantine_reason VARCHAR,
    decay_function VARCHAR
)
"#
    );
    sqlx::query(&create_memories)
        .execute(pool)
        .await
        .map_err(|e| Error::Storage(format!("create memories: {e}")))?;

    // 2. acls
    sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS acls (
    id UUID PRIMARY KEY,
    memory_id UUID NOT NULL,
    principal_type VARCHAR NOT NULL,
    principal_id VARCHAR NOT NULL,
    permission VARCHAR NOT NULL,
    granted_by VARCHAR NOT NULL,
    created_at VARCHAR NOT NULL,
    expires_at VARCHAR
)
"#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Storage(format!("create acls: {e}")))?;

    // 3. relations
    sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS relations (
    id UUID PRIMARY KEY,
    source_id UUID NOT NULL,
    target_id UUID NOT NULL,
    relation_type VARCHAR NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    metadata JSONB,
    created_at VARCHAR NOT NULL
)
"#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Storage(format!("create relations: {e}")))?;

    // 4. agent_events
    sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS agent_events (
    id UUID PRIMARY KEY,
    agent_id VARCHAR NOT NULL,
    thread_id VARCHAR,
    run_id VARCHAR,
    parent_event_id UUID,
    event_type VARCHAR NOT NULL,
    payload JSONB,
    trace_id VARCHAR,
    span_id VARCHAR,
    model VARCHAR,
    tokens_input BIGINT,
    tokens_output BIGINT,
    latency_ms BIGINT,
    cost_usd DOUBLE PRECISION,
    "timestamp" VARCHAR NOT NULL,
    logical_clock BIGINT NOT NULL DEFAULT 0,
    content_hash BYTEA NOT NULL,
    prev_hash BYTEA,
    embedding BYTEA
)
"#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Storage(format!("create agent_events: {e}")))?;

    // 5. checkpoints
    sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS checkpoints (
    id UUID PRIMARY KEY,
    thread_id VARCHAR NOT NULL,
    agent_id VARCHAR NOT NULL,
    parent_id UUID,
    branch_name VARCHAR NOT NULL DEFAULT 'main',
    state_snapshot JSONB,
    state_diff JSONB,
    memory_refs TEXT[],
    event_cursor UUID,
    label VARCHAR,
    created_at VARCHAR NOT NULL,
    metadata JSONB
)
"#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Storage(format!("create checkpoints: {e}")))?;

    // 6. delegations
    sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS delegations (
    id UUID PRIMARY KEY,
    delegator_id VARCHAR NOT NULL,
    delegate_id VARCHAR NOT NULL,
    permission VARCHAR NOT NULL,
    scope_type VARCHAR NOT NULL DEFAULT 'all_memories',
    scope_value JSONB,
    max_depth INTEGER NOT NULL DEFAULT 0,
    current_depth INTEGER NOT NULL DEFAULT 0,
    parent_delegation_id UUID,
    created_at VARCHAR NOT NULL,
    expires_at VARCHAR,
    revoked_at VARCHAR
)
"#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Storage(format!("create delegations: {e}")))?;

    // 7. agent_profiles
    sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS agent_profiles (
    agent_id VARCHAR PRIMARY KEY,
    avg_importance DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    avg_content_length DOUBLE PRECISION NOT NULL DEFAULT 100,
    total_memories BIGINT NOT NULL DEFAULT 0,
    last_updated VARCHAR NOT NULL
)
"#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Storage(format!("create agent_profiles: {e}")))?;

    // 8. sync_metadata
    sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS sync_metadata (
    key VARCHAR PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at VARCHAR NOT NULL
)
"#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Storage(format!("create sync_metadata: {e}")))?;

    // ---- Indexes ----
    let index_stmts: &[&str] = &[
        "CREATE INDEX IF NOT EXISTS idx_memories_agent ON memories(agent_id)",
        "CREATE INDEX IF NOT EXISTS idx_memories_thread ON memories(agent_id, thread_id)",
        "CREATE INDEX IF NOT EXISTS idx_acls_memory ON acls(memory_id)",
        "CREATE INDEX IF NOT EXISTS idx_acls_principal ON acls(principal_id)",
        "CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_id)",
        "CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_id)",
        "CREATE INDEX IF NOT EXISTS idx_events_agent ON agent_events(agent_id)",
        "CREATE INDEX IF NOT EXISTS idx_events_thread ON agent_events(thread_id)",
        "CREATE INDEX IF NOT EXISTS idx_events_parent ON agent_events(parent_event_id)",
        "CREATE INDEX IF NOT EXISTS idx_checkpoints_thread ON checkpoints(thread_id, branch_name)",
        "CREATE INDEX IF NOT EXISTS idx_delegations_delegator ON delegations(delegator_id)",
        "CREATE INDEX IF NOT EXISTS idx_delegations_delegate ON delegations(delegate_id)",
    ];

    for stmt in index_stmts {
        sqlx::query(stmt)
            .execute(pool)
            .await
            .map_err(|e| Error::Storage(format!("create index: {e}")))?;
    }

    // Append-only enforcement on agent_events: prevent UPDATE/DELETE at schema level
    sqlx::query(
        r#"
CREATE OR REPLACE FUNCTION prevent_event_modification() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'agent_events is append-only: % not allowed', TG_OP;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql
"#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Storage(format!("create append-only function: {e}")))?;

    sqlx::query(
        r#"
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'enforce_append_only_events') THEN
        CREATE TRIGGER enforce_append_only_events
            BEFORE UPDATE OR DELETE ON agent_events
            FOR EACH ROW EXECUTE FUNCTION prevent_event_modification();
    END IF;
END $$
"#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Storage(format!("create append-only trigger: {e}")))?;

    // HNSW vector index for cosine similarity
    // Use DO block to make this idempotent (pgvector HNSW index creation
    // does not support IF NOT EXISTS in all versions).
    let hnsw_sql = r#"
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes WHERE indexname = 'idx_memories_embedding_hnsw'
    ) THEN
        CREATE INDEX idx_memories_embedding_hnsw ON memories USING hnsw (embedding vector_cosine_ops);
    END IF;
END
$$
"#;
    sqlx::query(hnsw_sql)
        .execute(pool)
        .await
        .map_err(|e| Error::Storage(format!("create hnsw index: {e}")))?;

    Ok(())
}
