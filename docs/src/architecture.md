# Architecture

## System Overview

```
┌─────────────┐  ┌──────────────┐  ┌──────────────┐
│  MCP Client  │  │  REST Client  │  │  Python SDK  │
│  (stdio)     │  │  (HTTP)       │  │  (PyO3)      │
└──────┬───────┘  └──────┬────────┘  └──────┬───────┘
       │                 │                   │
       ▼                 ▼                   ▼
┌─────────────────────────────────────────────────────┐
│                   MnemoEngine                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
│  │ Remember  │  │  Recall   │  │  Forget/Share/   │  │
│  │ Pipeline  │  │  Pipeline │  │  Branch/Merge... │  │
│  └────┬──────┘  └────┬──────┘  └────┬─────────────┘  │
│       │              │              │                  │
│  ┌────▼──────────────▼──────────────▼──────────┐     │
│  │            StorageBackend (trait)             │     │
│  │   ┌──────────┐         ┌─────────────┐      │     │
│  │   │  DuckDB   │         │  PostgreSQL  │      │     │
│  │   └──────────┘         └─────────────┘      │     │
│  └──────────────────────────────────────────────┘     │
│                                                        │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────┐   │
│  │ VectorIndex  │  │ FullTextIndex │  │ Embeddings  │   │
│  │ (USearch/PG) │  │  (Tantivy)    │  │ (OpenAI/    │   │
│  │              │  │               │  │  Noop)      │   │
│  └─────────────┘  └──────────────┘  └────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Data Model

### MemoryRecord

The core data structure. Key fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID v7 | Time-ordered unique identifier |
| `agent_id` | String | Owning agent |
| `content` | String | Memory content |
| `memory_type` | Enum | Episodic, Semantic, Procedural, Strategic |
| `scope` | Enum | Private, Shared, Global |
| `importance` | f32 | 0.0-1.0 importance score |
| `tags` | Vec | Searchable tags |
| `embedding` | Vec | Vector embedding |
| `content_hash` | String | SHA-256 hash |
| `prev_hash` | Option | Previous record hash (chain) |

### Retrieval Pipeline

Recall uses Reciprocal Rank Fusion (RRF) to combine:

1. **Vector similarity** (cosine via USearch or pgvector)
2. **BM25 full-text** (Tantivy)
3. **Recency scoring** (exponential decay)
4. **Graph expansion** (1-2 hop relation traversal)

### Access Control

Three-tier permission model:

1. **Owner**: Agent who created the memory has full access
2. **ACL**: Explicit grants via `share` with permission levels (Read, Write, Delete, Share, Delegate)
3. **Delegation**: Transitive, scoped, time-bounded permission delegation

### Hash Chain Integrity

Every memory record is linked via SHA-256 hashes:

```
Record₁ → hash(content₁)
Record₂ → hash(content₂ + hash₁)
Record₃ → hash(content₃ + hash₂)
```

The `verify` tool checks the entire chain for tampering.
