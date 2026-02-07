# PostgreSQL Mode

PostgreSQL mode enables distributed, multi-instance Mnemo deployments with pgvector for vector search.

## Setup

### 1. PostgreSQL with pgvector

```bash
docker run -d \
  --name mnemo-pg \
  -e POSTGRES_USER=mnemo \
  -e POSTGRES_PASSWORD=changeme_use_strong_password \
  -e POSTGRES_DB=mnemo \
  -p 5432:5432 \
  pgvector/pgvector:pg16
```

### 2. Start Mnemo with PostgreSQL

```bash
mnemo --postgres-url "postgres://mnemo:$POSTGRES_PASSWORD@localhost/mnemo"
```

Or build with the postgres feature:

```bash
cargo build --release --features postgres
```

## Schema

Mnemo automatically creates all required tables on first connection:

- `memories` with pgvector `vector(N)` column and HNSW index
- `acls`, `delegations`, `relations`, `agent_events`
- `checkpoints`, `agent_profiles`

## Differences from DuckDB Mode

| Feature | DuckDB | PostgreSQL |
|---------|--------|------------|
| Vector index | USearch (HNSW) | pgvector (HNSW) |
| Full-text | Tantivy | PostgreSQL FTS (planned) |
| Concurrency | Single-writer | Multi-writer |
| Persistence | File-based | Server-based |
| Scaling | Single instance | Multiple instances |
