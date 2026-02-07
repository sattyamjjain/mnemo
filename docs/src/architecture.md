# Architecture

## System Overview

```
┌──────────┐  ┌───────────┐  ┌──────────┐  ┌──────────┐
│MCP Client│  │REST Client│  │  gRPC    │  │  psql    │
│ (stdio)  │  │  (HTTP)   │  │ (tonic)  │  │ (pgwire) │
└────┬─────┘  └─────┬─────┘  └────┬─────┘  └────┬─────┘
     │              │              │              │
     ▼              ▼              ▼              ▼
┌────────────────────────────────────────────────────────┐
│                    MnemoEngine                          │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │ Remember │ │  Recall  │ │ Forget/  │ │Checkpoint│ │
│  │ Pipeline │ │ Pipeline │ │Share/... │ │/Branch/  │ │
│  │          │ │  (RRF)   │ │          │ │Merge     │ │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ │
│       └─────────────┴────────────┴────────────┘       │
│                         │                              │
│  ┌──────────────────────▼──────────────────────────┐  │
│  │          StorageBackend (trait)                   │  │
│  │   ┌──────────┐              ┌─────────────┐     │  │
│  │   │  DuckDB   │              │  PostgreSQL  │     │  │
│  │   │           │              │  + pgvector  │     │  │
│  │   └──────────┘              └─────────────┘     │  │
│  └──────────────────────────────────────────────────┘  │
│                                                         │
│  ┌────────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐ │
│  │VectorIndex │ │FullText  │ │Embeddings│ │Encrypt │ │
│  │USearch/PG  │ │ Tantivy  │ │OpenAI/   │ │AES-256 │ │
│  │  (HNSW)   │ │ (BM25)   │ │ONNX/Noop │ │  GCM   │ │
│  └────────────┘ └──────────┘ └──────────┘ └────────┘ │
│                                                         │
│  ┌────────────┐ ┌──────────┐ ┌──────────────────────┐ │
│  │   Cache    │ │ColdStore │ │  Poisoning Detection  │ │
│  │ (in-mem)   │ │  (S3)    │ │  + Prompt Injection   │ │
│  └────────────┘ └──────────┘ └──────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Crate Structure

| Crate | Purpose |
|-------|---------|
| `mnemo-core` | Storage, data model, query engine, indexing, encryption |
| `mnemo-mcp` | MCP server via rmcp 0.14 (STDIO transport) |
| `mnemo-cli` | CLI binary with clap argument parsing |
| `mnemo-postgres` | PostgreSQL storage backend via sqlx + pgvector |
| `mnemo-rest` | REST API via Axum 0.8 |
| `mnemo-admin` | Admin dashboard endpoints (agent stats) |
| `mnemo-pgwire` | PostgreSQL wire protocol server |
| `mnemo-grpc` | gRPC API via tonic 0.12 |
| `python` | Python bindings via PyO3 |

## Data Model

### MemoryRecord

The core data structure. Key fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID v7 | Time-ordered unique identifier |
| `agent_id` | String | Owning agent |
| `content` | String | Memory content (encrypted at rest if enabled) |
| `memory_type` | Enum | Episodic, Semantic, Procedural, Strategic |
| `scope` | Enum | Private, Shared, Global |
| `importance` | f32 | 0.0-1.0 importance score |
| `tags` | Vec | Searchable tags |
| `embedding` | Vec | Vector embedding |
| `content_hash` | Vec\<u8\> | SHA-256 hash |
| `prev_hash` | Option | Previous record hash (chain) |
| `quarantined` | bool | Flagged by poisoning detection |
| `decay_rate` | Option\<f32\> | Custom decay rate |
| `decay_function` | Option | Custom decay function |

### Retrieval Pipeline

Recall uses Reciprocal Rank Fusion (RRF) to combine:

1. **Vector similarity** (cosine via USearch or pgvector HNSW)
2. **BM25 full-text** (Tantivy)
3. **Recency scoring** (exponential decay with configurable half-life)
4. **Graph expansion** (1-2 hop relation traversal)

Weights are configurable via `hybrid_weights` parameter. Permission-safe ANN pre-filtering ensures only authorized memories appear in results.

### Access Control

Three-tier permission model:

1. **Owner**: Agent who created the memory has full access
2. **ACL**: Explicit grants via `share` with permission levels (Read, Write, Delete, Share, Delegate)
3. **Delegation**: Transitive, scoped, time-bounded permission delegation with depth limits

### Hash Chain Integrity

Every memory record is linked via SHA-256 hashes:

```
Record₁ → content_hash = SHA256(content + agent_id + timestamp)
Record₂ → prev_hash = SHA256(content_hash₂ + content_hash₁)
Record₃ → prev_hash = SHA256(content_hash₃ + content_hash₂)
```

The `verify` tool checks the entire chain for tampering using constant-time comparisons.

### Security Layers

- **Encryption**: AES-256-GCM at-rest content encryption (pluggable via `ContentEncryption`)
- **Validation**: agent_id charset/length validation at engine level
- **Poisoning**: anomaly scoring + prompt injection pattern detection → quarantine
- **CORS**: configurable origin allowlist, defaults to localhost
- **Error sanitization**: internal errors logged only, generic messages returned
