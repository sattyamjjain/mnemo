# Mnemo

**MCP-native memory database for AI agents.**

Mnemo (from Greek *mneme* — memory) is an embedded database whose primitives are **REMEMBER**, **RECALL**, **FORGET**, and **SHARE** — exposed as [MCP](https://modelcontextprotocol.io/) tools that any AI agent can connect to directly.

## Quickstart

### 1. Build

```bash
cargo build --release
```

### 2. Configure your AI agent

Add to your MCP client configuration (e.g. Claude Desktop, Cursor, etc.):

```json
{
  "mcpServers": {
    "mnemo": {
      "command": "./target/release/mnemo",
      "args": ["--db-path", "./agent.mnemo.db"],
      "env": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }
}
```

### 3. Use it

Your AI agent now has persistent memory with 10 MCP tools:

| Tool | Description |
|------|-------------|
| `mnemo.remember` | Store a new memory with semantic embeddings |
| `mnemo.recall` | Search memories by semantic similarity, keywords, or hybrid |
| `mnemo.forget` | Delete memories (soft delete, hard delete, decay, consolidate, archive) |
| `mnemo.share` | Share a memory with another agent |
| `mnemo.checkpoint` | Snapshot the current agent memory state |
| `mnemo.branch` | Create a branch from a checkpoint for experimentation |
| `mnemo.merge` | Merge a branch back into the main state |
| `mnemo.replay` | Replay events from a checkpoint |
| `mnemo.delegate` | Delegate scoped, time-bounded permissions to another agent |
| `mnemo.verify` | Verify SHA-256 hash chain integrity |

## Access Protocols

| Protocol | Crate | Use Case |
|----------|-------|----------|
| **MCP** (stdio) | `mnemo-mcp` | AI agent integration via rmcp 0.14 |
| **REST** (HTTP) | `mnemo-rest` | Web clients, dashboards, OTLP ingest |
| **gRPC** | `mnemo-grpc` | High-performance service-to-service |
| **pgwire** | `mnemo-pgwire` | Connect with any PostgreSQL client (`psql`) |

## SDKs

### Python

```bash
pip install mnemo
```

```python
from mnemo import MnemoClient

client = MnemoClient(db_path="agent.mnemo.db")
result = client.remember("The user prefers dark mode", tags=["preference"])
memories = client.recall("user preferences", limit=5)
client.forget([result["id"]])

# Mem0-compatible aliases also available:
# client.add(), client.search(), client.delete()
```

Integrations: OpenAI Agents SDK, LangGraph, CrewAI.

### TypeScript

```typescript
import { MnemoClient } from "@mnemo/sdk";

const client = new MnemoClient({ dbPath: "agent.mnemo.db" });
await client.remember("User prefers dark mode");
const memories = await client.recall("user preferences");
```

### Go

```go
client := mnemo.NewClient("agent.mnemo.db")
client.Remember("User prefers dark mode")
memories := client.Recall("user preferences")
```

## Storage Backends

| Backend | Best For |
|---------|----------|
| **DuckDB** (default) | Single-agent, embedded, zero-config |
| **PostgreSQL** + pgvector | Multi-agent, distributed, production |

## Key Features

- **Hybrid retrieval** — Reciprocal Rank Fusion combining semantic vectors (USearch/pgvector), BM25 keywords (Tantivy), knowledge graph signals, and recency scoring
- **AES-256-GCM encryption** — at-rest content encryption via `MNEMO_ENCRYPTION_KEY`
- **Hash chain integrity** — SHA-256 content hashes with chain linking and `verify` tool
- **Memory poisoning detection** — anomaly scoring with prompt injection pattern detection; quarantine for flagged content
- **Cognitive forgetting** — five strategies: soft delete, hard delete, decay, consolidation, archive
- **Branching and replay** — checkpoint, branch, merge, and replay agent memory timelines
- **Point-in-time queries** — recall memories as they existed at any timestamp with `as_of`
- **Causal debugging** — trace event causality chains up/down with type filtering
- **RBAC + delegation** — ACL-based permissions with scoped, depth-limited transitive delegation
- **OTLP observability** — ingest OpenTelemetry GenAI spans as agent events

## CLI Options

```
mnemo [OPTIONS]

Options:
  --db-path <PATH>           Database file path [default: mnemo.db] [env: MNEMO_DB_PATH]
  --openai-api-key <KEY>     OpenAI API key [env: OPENAI_API_KEY]
  --embedding-model <MODEL>  Embedding model [default: text-embedding-3-small] [env: MNEMO_EMBEDDING_MODEL]
  --dimensions <DIM>         Embedding dimensions [default: 1536] [env: MNEMO_DIMENSIONS]
  --agent-id <ID>            Default agent ID [default: default] [env: MNEMO_AGENT_ID]
  --org-id <ID>              Organization ID [env: MNEMO_ORG_ID]
  --rest-port <PORT>         Enable REST API on this port [env: MNEMO_REST_PORT]
  --postgres-url <URL>       Use PostgreSQL backend [env: MNEMO_POSTGRES_URL]
  --encryption-key <HEX>     AES-256-GCM encryption key (64 hex chars) [env: MNEMO_ENCRYPTION_KEY]
```

## Architecture

```
┌──────────┐  ┌───────────┐  ┌──────────┐  ┌──────────┐
│MCP Client│  │REST Client│  │  gRPC    │  │  psql    │
│ (stdio)  │  │  (HTTP)   │  │          │  │ (pgwire) │
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
│  │   └──────────┘              └─────────────┘     │  │
│  └──────────────────────────────────────────────────┘  │
│                                                         │
│  ┌────────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐ │
│  │VectorIndex │ │FullText  │ │Embeddings│ │Encrypt │ │
│  │USearch/PG  │ │ Tantivy  │ │OpenAI/   │ │AES-256 │ │
│  │            │ │          │ │ONNX/Noop │ │GCM     │ │
│  └────────────┘ └──────────┘ └──────────┘ └────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Development

```bash
# Run all tests (132 tests)
cargo test --all

# Run integration tests only
cargo test -p mnemo-core --test integration_test

# Run benchmarks
cargo bench -p mnemo-core

# Build Python SDK
cd python && maturin develop
```

## License

Apache-2.0
