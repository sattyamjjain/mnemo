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
pip install mnemo-db
```

> **Why `mnemo-db` and not `mnemo`?** A 2021 notebook project (last release 2021-07-06, unrelated) holds the unqualified `mnemo` name on PyPI. Our distribution publishes as `mnemo-db`; the import path stays `from mnemo import …` so existing code is unaffected.

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

#### Memory-tool servers and shared-memory adapters (v0.3.4 → v0.4.1)

| Surface | Class | What it does |
|---|---|---|
| [Anthropic memory tool `memory_20250818`](docs/src/integrations/anthropic-memory-tool.md) | `MnemoMemoryToolServer` | Client-side handler for the 6-op `view`/`create`/`str_replace`/`insert`/`delete`/`rename` surface — every "file" lands as a Mnemo memory with hash-chain + ACL coverage. `pip install 'mnemo-db[anthropic-memory-tool]'`. |
| [Letta Conversations-style shared memory](docs/src/integrations/letta-conversations.md) | `MnemoLettaShared` | Multiple agents sharing a single audit-replayable memory stream. `attach`/`detach`/`read`/`write`/`list_participants` over Mnemo memories tagged `conversation:<id>` + `participant:<agent_id>`. |
| [Cloudflare R2 workspace](docs/src/integrations/r2-workspace.md) | `CloudflareR2Workspace` | Drop-in R2 backend for `MnemoSnapshotStore` — same signed-manifest contract as the AWS S3 path; `pip install 'mnemo-db[openai-sandbox-r2]'`. |
| [Letta-protocol-compat REST surface](crates/mnemo-letta/) (`mnemo-letta` crate) | `mnemo_letta::router(engine)` | `POST /v1/agents`, `POST /v1/agents/{id}/messages`, `GET /v1/agents/{id}/memory` — drop in front of any `MnemoEngine` so a Letta-Code-shaped benchmark or notebook can talk to Mnemo without code changes. New in v0.4.0-rc3 (B5). |
| [Mannsetu DPDPA consent manager](crates/mnemo-compliance/src/mannsetu.rs) (`mnemo-compliance` crate) | `MannsetuConsentSource` + `ConsentTokenGuard` | DPB-registered consent-manager binding plus a per-write guard with expiry / scope / revocation checks. Refuses any `remember` whose consent token is missing, expired, wrong-scope, or revoked. New in v0.4.0-rc3 (B4). |
| [DPDPA "data passport" PDF](python/mnemo/dpdpa_passport.py) | `mnemo.dpdpa_passport.build_passport_pdf` | One-page PDF showing every personal data point Mnemo holds for a subject, suitable for Section 11 / 12 access requests. Hand-rolled PDF — zero third-party deps, byte-for-byte reproducible. New in v0.4.0-rc3 (Q3). |
| [Provenance SDK](python/mnemo/provenance.py) | `mnemo.provenance.verify_read_provenance` | Pure-Python verifier for the HMAC-SHA256 receipts that Mnemo returns alongside `recall(..., with_provenance=True)`. Auditors verify offline without compiling Rust. New in v0.4.0-rc3 (Q1). |
| [Claude Code installer](python/mnemo/install_claude_code.py) | `python -m mnemo install claude-code [--hardened <manifest>]` | Idempotently registers Mnemo as an MCP server in `~/.claude.json`. The `--hardened` flag switches the registered launcher to the v0.4.0-rc3 hardened mode. New in v0.4.0-rc3 (Q2). |
| [Anthropic CMA-Memory compat shim](crates/mnemo-cma/) (`mnemo-cma` crate) | `CmaTreeRoot` + `import_cma_tree` + `audit_bridge` | Drop-in for the Anthropic Context-Managed Agent Memory beta announced 2026-04-23. Mounts an existing CMA `.memory/` tree, mirrors writes through to mnemo's HMAC chain, and exports back byte-identical so users can leave cleanly. New in v0.4.1 (P0-2). |
| [Agent behavioural-baseline exporter](crates/mnemo-baseline/) (`mnemo-baseline` crate) | `AgentBaseline` + `JsonExporter` (OTel + OCSF) | Per-agent rolling profile (recall/write rates, namespace fanout, tool mix, HMAC continuity) emitted to OpenTelemetry semconv 1.31 + OCSF 1.4 Application-Activity envelopes with z-score+EWMA drift detection. Anti-leak invariant: emitted payloads never carry memory contents. Plugs into the agentic-SOC telemetry gap RSAC 2026 flagged. New in v0.4.1 (P0-3). |
| [1M-context recall budget planner](crates/mnemo-core/src/budget/) (`mnemo-core::budget`) | `ContextBudget::for_model` + `plan_recall` | First OSS memory store with an explicit per-model `ContextBudget → RecallPlan` planner. Per-model table covers `deepseek-v4-1m`, `claude-3.7-sonnet-1m`, `gpt-5.1-400k`, `gemini-2.5-pro-2m` plus their smaller siblings. Typed `FallbackStrategy` (TruncateOldest / SummarizeOldestK / DropDuplicates / None). Property test: never overflows total context. New in v0.4.1 (P1-4). |
| [Project-Deal counterparty discovery + reputation](crates/mnemo-deal/) (`mnemo-deal::discovery` + `::reputation`) | `AgentAdvertisement` + `compute_reputation` | `/.well-known/mnemo-deal-agent.json` advertisement (Ed25519-keyed, capability-tagged) plus an advisory reputation score with 90-day half-life decay and per-dispute 10% penalty. mnemo becomes not just the deal ledger but the directory of the agent-deal substrate. **Advisory only** — see `docs/deal-reputation-threats.md`. New in v0.4.1 (P1-5). |

### TypeScript

```typescript
import { MnemoClient } from "@mndfreek/mnemo-sdk";

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
- **Bitemporal graph layer** ([`mnemo-graph`](docs/src/concepts/temporal-edges.md)) — Graphiti-inspired temporal edges with `valid_from` / `valid_to` (fact validity) plus `recorded_at` (system clock). `graph_expand(seed, depth, as_of)` walks the graph at any point in time without losing later corrections. New in v0.4.0-rc1.
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
- **Memory-provenance signing on reads** — every `recall(..., with_provenance=True)` returns an HMAC-SHA256 receipt binding the cited records to a server-side key; supports key rotation. Verify offline from Python via `mnemo.provenance.verify_read_provenance`. New in v0.4.0-rc3.
- **Hardened MCP launcher** — `mnemo mcp-server --manifest <path>` runs a safe-spawn gauntlet (refuse inherited secrets, refuse `--config` argv injection, refuse untrusted parents) BEFORE engine state is constructed. Direct response to the OX-MCP "exfiltrate-then-act" disclosure (2026-04-24). All privileged knobs come from a chmod-restricted TOML manifest. New in v0.4.0-rc3.
- **DPDPA consent-token-per-write** — Mannsetu adapter + `ConsentTokenGuard` (expiry / scope / revocation) refuses every `remember` that is not authorized by a fresh consent token. Per-subject "data passport" PDF for Section 11 / 12 access requests. New in v0.4.0-rc3.
- **MCP tool-catalog attestation** — `mnemo mcp-server` pins the advertised MCP tool list, refuses to start when the catalog gains an unauthorized tool or any tool's `inputSchema` mutates, and emits `MNEMO_MCP_TOOL_DRIFT` audit rows. Direct response to arXiv 2604.20994 (function-hijacking via tool-list poisoning). New in v0.4.0.
- **Cloudflare Mesh runtime adapter** — SPIFFE-style `MeshIdentity` + per-namespace `MemOp` ACL + `MeshAuditEnvelope` chained into the existing HMAC ledger. First OSS embedded memory DB to speak Cloudflare Mesh attestation natively. New in v0.4.0.
- **Code-mode WIT recall** — `mnemo:memory@0.4` WIT world plus a wasmtime-friendly host runner. Agents call `recall` as a sandboxed WASM function instead of a JSON tool envelope, dropping per-turn token cost ~96% on 200-turn LongMemEval_S samples. New in v0.4.0.
- **Decay-curve score lane** — `DecayLane` (Ebbinghaus + reinforcement) fuses with vector + BM25 + recency in the default recall path. `letta_mode` flag bypasses it for parity with Letta's published numbers. New in v0.4.0.
- **Agent-Deal ledger** — `mnemo-deal` crate ships a chained-HMAC `DealEnvelope` log with `verify_chain → DisputeReport`. v0.4.1 adds advertisement (`/.well-known/mnemo-deal-agent.json`) + advisory reputation (90-day half-life, per-dispute 10% penalty).
- **Markdown+Git working-set adapter** — `mnemo-md-sync` parses YAML-style frontmatter (`mnemo_id`, `tags`, `expires_at`) and provides `MdSyncSpec` + `SyncFlushPolicy` (PreferEngine / PreferDisk / NewerWins). New in v0.4.0.
- **Anthropic CMA-Memory compat shim** — `mnemo-cma` crate mounts, mirrors, and exports the Anthropic CMA-Memory beta filesystem (announced 2026-04-23). Every CMA write is bridged into the mnemo HMAC chain via `CmaSource::CmaBeta` markers. New in v0.4.1.
- **Agent behavioural-baseline exporter** — `mnemo-baseline` crate emits per-agent profiles in OpenTelemetry semconv 1.31 + OCSF 1.4 Application-Activity formats with z-score+EWMA drift detection; anti-leak regex test ensures payloads never carry memory contents. Plugs into the RSAC 2026 SOC telemetry gap. New in v0.4.1.
- **1M-context recall budget planner** — `mnemo-core::budget` adds `ContextBudget::for_model` + `plan_recall` covering `deepseek-v4-1m`, `claude-3.7-sonnet-1m`, `gpt-5.1-400k`, `gemini-2.5-pro-2m`; typed `FallbackStrategy`; property test asserts no model overflows. New in v0.4.1.
- **mnemo doctor + Grafana dashboard** — typed `DoctorReport` + `DoctorFix` recommendations and a committed `dashboards/mnemo-grafana.json` (schemaVersion 39) covering recall p50/p99, tool-catalog drift, HMAC continuity, code-mode token reduction. New in v0.4.1.

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
mnemo [OPTIONS] [COMMAND]

Options:
  --db-path <PATH>              Database file path [default: mnemo.db] [env: MNEMO_DB_PATH]
  --openai-api-key <KEY>        OpenAI API key [env: OPENAI_API_KEY]
  --embedding-model <MODEL>     Embedding model [default: text-embedding-3-small] [env: MNEMO_EMBEDDING_MODEL]
  --dimensions <DIM>            Embedding dimensions [default: 1536] [env: MNEMO_DIMENSIONS]
  --agent-id <ID>               Default agent ID [default: default] [env: MNEMO_AGENT_ID]
  --org-id <ID>                 Organization ID [env: MNEMO_ORG_ID]
  --onnx-model-path <PATH>      ONNX embedding model path (local inference) [env: MNEMO_ONNX_MODEL_PATH]
  --rest-port <PORT>            Enable REST API on this port [env: MNEMO_REST_PORT]
  --postgres-url <URL>          Use PostgreSQL backend [env: MNEMO_POSTGRES_URL]
  --encryption-key <HEX>        AES-256-GCM encryption key (64 hex chars) [env: MNEMO_ENCRYPTION_KEY]
  --idle-timeout-seconds <SECS> Auto-shutdown after idle period (0 = disabled) [default: 0] [env: MNEMO_IDLE_TIMEOUT]

Commands:
  baseline    Train the per-agent embedding-space baseline used by the z-score
              outlier detector (v0.3.3, Task A).
  mcp-server  Start the MCP STDIO server in hardened mode using a TOML manifest
              (v0.4.0-rc3, Task B2). Refuses inherited secrets / argv injection /
              untrusted parents BEFORE engine state is constructed. Privileged
              knobs come from the manifest; key material reaches the binary via
              a chmod-restricted keystore file. See
              `examples/mcp-server/manifest.toml` for an annotated reference.
  eval        Replay a JSONL dataset of {query, expected} rows against an
              in-memory engine and emit a per-row latency / top-k JSONL report
              (v0.4.0-rc3, Task B6). Defaults to the bundled LongMemEval_M
              sample under `crates/mnemo-core/benches/data/longmemeval_m.jsonl`.
              Pass `--with-provenance` + `--provenance-key-hex <hex>` to also
              measure the HMAC-receipt overhead.
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

## Benchmarks

We run LoCoMo-MC10 and LongMemEval on every release. The canonical
results page is
[`docs/benchmarks/2026-04-25-mnemo-v0.3.4.md`](docs/benchmarks/2026-04-25-mnemo-v0.3.4.md)
— it carries reference rows for Hindsight (91.4% LongMemEval / 89.61%
LoCoMo, [source](https://benchmarks.hindsight.vectorize.io)) and
Letta-Filesystem (74.0%) plus the four mnemo retrieval strategies
side-by-side. The mnemo rows populate from the first authenticated
[nightly run](.github/workflows/benchmarks-nightly.yml) — ungated CI
forks read the empty rows and the workflow's first-run exception
keeps the regression gate honest. Earlier reports under
[`docs/benchmarks/`](docs/benchmarks/) carry the v0.3.0 / v0.3.1 floor
numbers from before the v0.3.3 Tantivy-default + LLM-judge fixes.

**First public LoCoMo number (v0.4.1, P0-1)** — full report at
[`docs/benchmarks/locomo-2026-04-28.md`](docs/benchmarks/locomo-2026-04-28.md).
mnemo joins the public LoCoMo board alongside MemMachine (84.87%,
2026-04-24) and Memori (81.95%, 2026-04-24); the harness ships at
[`bench/locomo`](bench/locomo) with a dual-judge variance gate
(GPT-5.1 + Claude-3.7 Sonnet) and runs nightly via
[`.github/workflows/locomo-nightly.yml`](.github/workflows/locomo-nightly.yml).
mnemo trades raw overall score for **temporal-slice strength + ~96% per-turn token cost** —
see the report for the honest pitch.

**LongMemEval_M provenance overhead bench (v0.4.0-rc3, B3).** A
self-contained 45-record synthesized dataset ships at
[`crates/mnemo-core/benches/data/longmemeval_m.jsonl`](crates/mnemo-core/benches/data/longmemeval_m.jsonl)
(override with `MNEMO_LONGMEMEVAL_PATH=<path>` for the published
gated dataset). The
[`longmemeval_bench`](crates/mnemo-core/benches/longmemeval_bench.rs)
criterion target runs two arms — `recall_no_provenance` and
`recall_with_provenance` — so the per-recall HMAC-receipt overhead
is measurable in CI:

```bash
cargo bench -p mnemo-core --bench longmemeval_bench
```

## Documentation

- **mdBook**: `docs/` directory — run `mdbook serve docs` for local browsing
- **Compliance**: SOC 2 controls mapping and HIPAA safeguards at `docs/src/compliance/`
- **REST API**: `docs/src/rest-api.md`
- **Tool reference**: `docs/src/tools/` (one page per MCP tool)
- **Hardened MCP launcher**: [`docs/src/integrations/mcp-server.md`](docs/src/integrations/mcp-server.md) — manifest schema, threat model, systemd unit example
- **Time-travel debugger**: [`examples/time-travel-debugger/index.html`](examples/time-travel-debugger/index.html) — vanilla-JS UI that diffs recall results between two `as_of` timestamps; serve any way you like (`python3 -m http.server`)
- **LoCoMo report**: [`docs/benchmarks/locomo-2026-04-28.md`](docs/benchmarks/locomo-2026-04-28.md) — first public mnemo number alongside MemMachine + Memori with the honest temporal-slice + per-turn-token pitch
- **Grafana dashboard**: [`dashboards/mnemo-grafana.json`](dashboards/mnemo-grafana.json) — schemaVersion 39, drop straight into Grafana 11.5; covers recall p50/p99, tool-catalog drift, HMAC continuity, code-mode token reduction, baseline anomalies
- **Benchmarks**: `docs/benchmarks/`

## License

Apache-2.0
