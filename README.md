# Mnemo

[![CI](https://github.com/sattyamjjain/mnemo/actions/workflows/ci.yml/badge.svg)](https://github.com/sattyamjjain/mnemo/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)

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
| **gRPC** | `mnemo-grpc` | High-performance service-to-service (11 RPCs) |
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

#### Framework Integrations

Mnemo provides native integration modules for 15 agent frameworks:

| Framework | Integration Class | Connection |
|-----------|------------------|------------|
| [OpenAI Agents SDK](https://github.com/openai/openai-agents-python) | `MnemoAgentMemory` | MCP stdio |
| [LangGraph](https://github.com/langchain-ai/langgraph) | `MnemoLangGraphTools` | MCP stdio |
| [CrewAI](https://github.com/crewAIInc/crewAI) | `ASMDMemory` | Direct PyO3 |
| [Google ADK](https://github.com/google/adk-python) | `MnemoADKToolset` | MCP stdio |
| [Agno](https://github.com/agno-agi/agno) | `MnemoAgnoTools` | MCP stdio |
| [Pydantic AI](https://github.com/pydantic/pydantic-ai) | `MnemoPydanticToolset` | MCP stdio |
| [AutoGen](https://github.com/microsoft/autogen) | `MnemoAutoGenWorkbench` | MCP stdio |
| [Smolagents](https://github.com/huggingface/smolagents) | `MnemoSmolagentsTools` | MCP stdio |
| [Strands Agents](https://github.com/strands-agents/sdk-python) | `MnemoStrandsClient` | MCP stdio |
| [Semantic Kernel](https://github.com/microsoft/semantic-kernel) | `MnemoSKPlugin` | MCP stdio |
| [Llama Stack](https://github.com/meta-llama/llama-stack) | `register_mnemo_toolgroup` | REST API |
| [DSPy](https://github.com/stanfordnlp/dspy) | `create_mnemo_tools` | Direct PyO3 |
| [CAMEL AI](https://github.com/camel-ai/camel) | `create_mnemo_camel_tools` | Direct PyO3 |
| [Mem0](https://github.com/mem0ai/mem0) (compat) | `Mem0Compat` | Direct PyO3 |
| LangGraph Checkpointer | `ASMDCheckpointer` | Direct PyO3 |

All integrations are auto-imported via `from mnemo import <ClassName>` — dependencies fail gracefully if not installed.

### TypeScript

```typescript
import { MnemoClient } from "@mnemo/sdk";

const client = new MnemoClient({ dbPath: "agent.mnemo.db" });
await client.connect();

const { id } = await client.remember({ content: "User prefers dark mode" });
const { memories } = await client.recall({ query: "user preferences" });

await client.close();
```

### Go

```go
import "github.com/sattyamjjain/mnemo/sdks/go"

client, err := mnemo.NewClient(mnemo.ClientOptions{DbPath: "agent.mnemo.db"})
defer client.Close()

result, _ := client.Remember(mnemo.RememberInput{Content: "User prefers dark mode"})
memories, _ := client.Recall(mnemo.RecallInput{Query: "user preferences"})
```

## Storage Backends

| Backend | Best For |
|---------|----------|
| **DuckDB** (default) | Single-agent, embedded, zero-config |
| **PostgreSQL** + pgvector | Multi-agent, distributed, production |

## Key Features

- **Hybrid retrieval** — Reciprocal Rank Fusion combining semantic vectors (USearch/pgvector), BM25 keywords (Tantivy), knowledge graph signals, and recency scoring with configurable weights
- **AES-256-GCM encryption** — at-rest content encryption via `MNEMO_ENCRYPTION_KEY`
- **Hash chain integrity** — SHA-256 content hashes with chain linking and `verify` tool
- **Memory poisoning detection** — anomaly scoring with prompt injection pattern detection; quarantine for flagged content
- **Cognitive forgetting** — five strategies: soft delete, hard delete, decay, consolidation, archive
- **Branching and replay** — checkpoint, branch, merge, and replay agent memory timelines
- **Point-in-time queries** — recall memories as they existed at any timestamp with `as_of`
- **Causal debugging** — trace event causality chains up/down with type filtering
- **RBAC + delegation** — ACL-based permissions with scoped, depth-limited transitive delegation
- **Permission-safe ANN** — iterative oversampling with post-filtering for ACL compliance
- **ONNX local embeddings** — run embeddings locally without API calls via `MNEMO_ONNX_MODEL_PATH`
- **S3 cold storage** — archive old memories to S3-compatible storage (feature-gated)
- **LRU cache** — in-memory caching layer for frequently accessed memories
- **Scale-to-zero** — auto-shutdown after configurable idle timeout with checkpoint-on-shutdown
- **OTLP observability** — ingest OpenTelemetry GenAI spans as agent events
- **Append-only audit log** — immutable event log with database-enforced triggers (PostgreSQL)
- **Evidence-weighted conflict resolution** — resolve multi-agent conflicts using source reliability scoring

## Examples

The `examples/` directory contains working integration examples for all major agent frameworks:

| Example | Framework | Language |
|---------|-----------|----------|
| [`openai_agents_example.py`](examples/openai_agents_example.py) | OpenAI Agents SDK | Python |
| [`langgraph_mcp_example.py`](examples/langgraph_mcp_example.py) | LangGraph + MCP | Python |
| [`crewai_mcp_example.py`](examples/crewai_mcp_example.py) | CrewAI + MCP | Python |
| [`google_adk_example.py`](examples/google_adk_example.py) | Google ADK | Python |
| [`agno_example.py`](examples/agno_example.py) | Agno | Python |
| [`pydantic_ai_example.py`](examples/pydantic_ai_example.py) | Pydantic AI | Python |
| [`autogen_example.py`](examples/autogen_example.py) | AutoGen | Python |
| [`smolagents_example.py`](examples/smolagents_example.py) | HuggingFace Smolagents | Python |
| [`strands_agents_example.py`](examples/strands_agents_example.py) | AWS Strands Agents | Python |
| [`semantic_kernel_example.py`](examples/semantic_kernel_example.py) | Microsoft Semantic Kernel | Python |
| [`llama_stack_example.py`](examples/llama_stack_example.py) | Meta Llama Stack | Python |
| [`dspy_example.py`](examples/dspy_example.py) | DSPy | Python |
| [`camel_ai_example.py`](examples/camel_ai_example.py) | CAMEL AI | Python |
| [`browser_use_example.py`](examples/browser_use_example.py) | Browser Use | Python |
| [`basic_memory.py`](examples/basic_memory.py) | Direct PyO3 | Python |
| [`mastra_example.ts`](examples/mastra_example.ts) | Mastra | TypeScript |
| [`vercel_ai_sdk_example.ts`](examples/vercel_ai_sdk_example.ts) | Vercel AI SDK | TypeScript |

## CLI Options

```
mnemo [OPTIONS]

Options:
  --db-path <PATH>              Database file path [default: mnemo.db] [env: MNEMO_DB_PATH]
  --openai-api-key <KEY>        OpenAI API key [env: OPENAI_API_KEY]
  --embedding-model <MODEL>     Embedding model [default: text-embedding-3-small] [env: MNEMO_EMBEDDING_MODEL]
  --dimensions <DIM>            Embedding dimensions [default: 1536] [env: MNEMO_DIMENSIONS]
  --agent-id <ID>               Default agent ID [default: default] [env: MNEMO_AGENT_ID]
  --org-id <ID>                 Organization ID [env: MNEMO_ORG_ID]
  --onnx-model-path <PATH>     ONNX embedding model path (local inference) [env: MNEMO_ONNX_MODEL_PATH]
  --rest-port <PORT>            Enable REST API on this port [env: MNEMO_REST_PORT]
  --postgres-url <URL>          Use PostgreSQL backend [env: MNEMO_POSTGRES_URL]
  --encryption-key <HEX>        AES-256-GCM encryption key (64 hex chars) [env: MNEMO_ENCRYPTION_KEY]
  --idle-timeout-seconds <SECS> Auto-shutdown after idle period (0 = disabled) [default: 0] [env: MNEMO_IDLE_TIMEOUT]
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

## Deployment

### Docker

```bash
docker build -t mnemo .
docker run -p 8080:8080 -e OPENAI_API_KEY=sk-... mnemo --rest-port 8080
```

### Kubernetes (Helm)

```bash
helm install mnemo deploy/helm/mnemo \
  --set env.OPENAI_API_KEY=sk-... \
  --set env.MNEMO_REST_PORT=8080
```

The Helm chart includes: Deployment, Service, ConfigMap, Secret, PVC, HPA, and Ingress templates.

## Development

```bash
# Run all tests (132 tests: unit + integration + MCP + pgwire + REST + admin + gRPC + doctests)
cargo test --all

# Run tests for a specific crate
cargo test -p mnemo-core
cargo test -p mnemo-mcp

# Run integration tests only
cargo test -p mnemo-core --test integration_test

# Lint and format
cargo clippy --all-targets --all-features
cargo fmt --all

# Run benchmarks
cargo bench -p mnemo-core

# Build with optional features
cargo build -p mnemo-core --features onnx     # ONNX local embeddings
cargo build -p mnemo-core --features s3        # S3 cold storage
cargo build -p mnemo-cli --features postgres   # PostgreSQL backend

# Build Python SDK (requires maturin, NOT cargo build)
cd python && maturin develop

# TypeScript SDK
cd sdks/typescript && npm install && npm test

# Go SDK
cd sdks/go && go test ./...
```

## Documentation

- **mdBook**: `docs/` directory — run `mdbook serve docs` for local browsing
- **Compliance**: SOC 2 controls mapping and HIPAA safeguards at `docs/src/compliance/`
- **REST API**: `docs/src/rest-api.md`
- **Tool reference**: `docs/src/tools/` (one page per MCP tool)

## License

Apache-2.0
