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

Your AI agent now has persistent memory with 12 MCP tools:

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
| `mnemo.attention_state.put` | v0.4.5 — Store an opaque attention-state blob keyed by `(agent_id, prefix_hash)` (anchored on [arXiv:2605.18226](https://arxiv.org/abs/2605.18226); only registered when the server is built with `MnemoServer::with_attention_state(...)`) |
| `mnemo.attention_state.get` | v0.4.5 — Look up an attention-state blob by `(agent_id, prefix_hash)`; returns `null` on miss |

## Access Protocols

| Protocol | Crate | Use Case |
|----------|-------|----------|
| **MCP** (stdio) | `mnemo-mcp` | AI agent integration via rmcp 1.3 |
| **REST** (HTTP) | `mnemo-rest` | Web clients, dashboards, OTLP ingest |
| **gRPC** | `mnemo-grpc` | High-performance service-to-service (11 RPCs) |
| **pgwire** | `mnemo-pgwire` | Connect with any PostgreSQL client (`psql`) |

### Attention-state-memory substrate (v0.4.5)

mnemo v0.4.5 ships an [attention-state-memory substrate](docs/research/context-memorization-2605.18226.md) anchored on [arXiv:2605.18226](https://arxiv.org/abs/2605.18226) (Context Memorization). Two new MCP tools — `mnemo.attention_state.put` and `mnemo.attention_state.get` — store and retrieve opaque attention-state blobs keyed by `(agent_id, prefix_hash)`. The substrate is implemented in [`crates/mnemo-attention-state`](crates/mnemo-attention-state) with a typed `AttentionStateStore` trait + an `InMemoryAttentionStateStore` reference impl.

**Honest scope:** mnemo ships the *store*. The producer (inference runtime that extracts prefix states) and the consumer (re-injection on the next generation) are out of scope; the substrate's blob format, model compatibility, and quantization sensitivity are the producer's responsibility. Tools are registered only when `MnemoServer::with_attention_state(...)` is configured at startup; unconfigured calls return a spec-shaped error result, not a panic. See the [research anchor](docs/research/context-memorization-2605.18226.md) for the operator recipe + the explicit non-overclaim disclaimers.

### Memory under interference — current-fact resolver (v0.4.7)

[arXiv:2605.18565](https://arxiv.org/abs/2605.18565) (MINTEval) measures how often memory systems return a *superseded* value of a fact after the same fact has been revised K times. mnemo v0.4.7 ships an **opt-in current-fact resolver** that post-processes the standard recall result set: candidates sharing the same value under a caller-chosen `fact_key` (typical convention: `"fact_id"`) are grouped, and only the most-recent write per group is kept. When `include_supersession_chain = true`, older fact-versions are returned in the response's new `superseded` field for audit.

Enable via the MCP `recall` tool param `current_fact_resolver: { fact_key, include_supersession_chain }`, the REST `?current_fact_key=…&current_fact_include_chain=true` query params, or the Rust `RecallRequest.current_fact_resolver = Some(...)` field directly. **The default read path is unchanged** — the resolver is purely additive and opt-in. The MINTEval-shaped bench at [`bench/locomo/src/bin/interference.rs`](bench/locomo/src/bin/interference.rs) compares default vs resolver arms across `K ∈ {1, 3, 5, 10}` revisions; see the resolver module doc at [`crates/mnemo-core/src/query/current_fact_resolver.rs`](crates/mnemo-core/src/query/current_fact_resolver.rs) for the contract + the explicit "not a contradiction detector / not a write-side guard" disclaimers.

### Repeated-context recall — orientation cache (v0.4.8)

[arXiv:2605.19932](https://arxiv.org/abs/2605.19932) (PEEK — Prefix-Encoded Episodic Knowledge) shows that a small, token-budgeted "orientation map" maintained alongside an agent's retrieval surface (key entities, `UPPER_SNAKE` constants, fenced schema fragments that have been useful) lets agents re-enter long-running contexts with a fraction of the recall payload. mnemo v0.4.8 ships an **opt-in orientation cache** that post-processes the standard recall result set: a heuristic Distiller extracts transferable knowledge from each hit and a priority Evictor enforces a fixed token budget (default 512). The recall response carries the bounded rendered map alongside `top-k` so the caller has both *what is in scope* and *what is relevant right now* in one payload.

Enable via the MCP `recall` tool param `orientation_cache: { namespace?, token_budget?, include_in_response?, distill? }`, the REST `?orientation_cache=true&orientation_namespace=…&orientation_token_budget=…` query params, the gRPC `OrientationCacheRequest` message, the pgwire `/*+ orientation_cache */` SQL hint comment, or the Rust `RecallRequest.orientation_cache = Some(OrientationCacheConfig::new())` field directly. The store is in-process, namespace-scoped (`(org_id, agent_id)` by default), and lost on restart — persistence is a v0.5.x knob. **The default read path is unchanged** and orientation rendering only fires when both the caller passes a config AND the engine has an `OrientationCacheStore` attached via `MnemoEngine::with_orientation_cache_store()`. See the PEEK-anchored bench at [`bench/locomo/src/bin/orientation.rs`](bench/locomo/src/bin/orientation.rs) for the repeated-context scenario (`K ∈ {3, 6, 10, 15}` calls per trial) and the module doc at [`crates/mnemo-core/src/query/orientation_cache.rs`](crates/mnemo-core/src/query/orientation_cache.rs) for the contract + the explicit "not a learned summariser / not a context-window extender / not persisted" disclaimers.

### Offline consolidation — Auto-Dreamer-shaped active-bank shrink (v0.4.8)

Anthropic's Auto-Dreamer consolidation runs offline, away from the agent's interactive loop, and produces a smaller *active bank* of semantic summaries that should serve subsequent recall at least as well as the raw episodic trace it replaced. mnemo's existing `run_decay_pass` + `run_consolidation` path ([`crates/mnemo-core/src/query/lifecycle.rs`](crates/mnemo-core/src/query/lifecycle.rs), plus the reflection module at [`crates/mnemo-core/src/query/reflection.rs`](crates/mnemo-core/src/query/reflection.rs) that mirrors the same offline-housekeeping shape) is the engine-side equivalent: the decay pass marks low effective-importance records as `Archived` / `Forgotten`, the consolidation pass replaces tag-overlap clusters of episodic memories with structured `[Consolidated from N memories] …` semantic bundles, and the originals are flipped to `Consolidated` rather than deleted (so the chain stays auditable). The new Auto-Dreamer-shaped scenario at [`bench/locomo/src/bin/auto_dreamer_consolidation.rs`](bench/locomo/src/bin/auto_dreamer_consolidation.rs) exercises both passes end-to-end on a synthetic multi-session trajectory and reports the two axes Auto-Dreamer headlines as its claim: `active_bank_ratio = post / pre` (expects `< 1.0`) and held-out `recall_post >= recall_pre`. A JSON summary lands beside the Markdown report so the headline number is citable here.

Run via `cargo run --release --bin auto_dreamer_consolidation -p mnemo-locomo-bench` — defaults to 8 sessions × 25 facts × 5 trials with archive/forget thresholds of 0.40 / 0.10 and `min_cluster_size = 3`; all tunable via CLI flags. **The default read path is unchanged** — the bench only consumes existing `mnemo_core::query::lifecycle::*` APIs and adds no public surface. See the bin module rustdoc for the full "what this bin is NOT" block (not a faithful Auto-Dreamer reproduction; not the `criterion` crate; `NoopEmbedding` makes the vector lane degenerate by design; single-agent, single-scope).

### Embedding-backend selection — SLA-aware recommender (v0.4.9)

[arXiv:2605.23618](https://arxiv.org/abs/2605.23618) (GE2 vs local encoders — quality + latency) motivates choosing an embedding backend by *measured* quality and tail-latency on the operator's workload, not by reputation. mnemo v0.4.9 ships [`bench/embeddings`](bench/embeddings), a criterion-driven bench + SLA-aware recommender that runs each *configured* backend (Noop and a bench-local hashing baseline always; `OpenAiEmbedding` when `OPENAI_API_KEY` is set; `OnnxEmbedding` when `MNEMO_ONNX_MODEL_PATH` is set and `mnemo-core` was built with the `onnx` feature) against a 50-document / 10-query labeled fixture and reports nDCG@10, recall@10, p50/p95 single-vector embed latency, and throughput at batch sizes 1/8/32. The recommender then picks the **highest-nDCG backend whose p95 ≤ the SLO** and reports the explicit nDCG gap vs the absolute best-quality backend (so the operator sees "you give up 0.003 nDCG for 7x lower p95 latency" rather than a black-box choice).

Run via `mnemo bench embeddings --slo-ms <N>` (built into the `mnemo` binary) or `cargo bench -p mnemo-embeddings-bench` (criterion HTML reports at `target/criterion/embed_single/`). **The default read path is unchanged** — no retrieval defaults, no RRF weights, no `EmbeddingProvider` impls are touched. The embedded-first wedge stays: default builds run without `OPENAI_API_KEY` and the recommender picks a local backend. See [`bench/embeddings/README.md`](bench/embeddings/README.md) for the full "what this bench is NOT" block (not a faithful arXiv:2605.23618 reproduction; not a managed-cloud default; `hashing-baseline` is a bench-local lexical sanity floor, not a production backend).

### mnemo as a golem:vector provider (v0.4.6)

mnemo v0.4.6 ships a vertical-slice WASM-component implementation of the [`golem:vector@1.0.0`](https://github.com/golemcloud/golem-ai/issues/21) WIT interface — three load-bearing functions (`upsert-vector` / `search-vectors` / `delete-vectors`) — split across two crates: [`crates/mnemo-golem-wit`](crates/mnemo-golem-wit) (the WASM component, compiled to `wasm32-wasip2` via `cargo component build`) and [`crates/mnemo-golem-host`](crates/mnemo-golem-host) (the Rust host that owns an `Arc<MnemoEngine>` and supplies the WIT host imports). The two-crate split is forced by mnemo-core's C++ deps (DuckDB + USearch) which cannot compile to WASM — see [`docs/research/golem-vector-wit-provider.md`](docs/research/golem-vector-wit-provider.md) for the layering rationale, the per-function gap list (27 of 30 deferred to v0.5.x), and the wasmtime-component-loader wiring step explicitly deferred. The vertical-slice integration is functionally complete as a Rust trait surface today (`MnemoGolemProvider` + `MnemoGolemHost`) with 5 integration tests + an end-to-end example showing REMEMBER → RECALL → DELETE through a real `MnemoEngine`.

### mnemo and the MCP 2026 Roadmap

The [MCP 2026 Roadmap](https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/) (published 2026-03-09 by lead maintainer David Soria Parra) reorganises the protocol's direction around four priority areas: **Transport Evolution and Scalability**, **Agent Communication**, **Governance Maturation**, and **Enterprise Readiness**. mnemo's existing surfaces — operator-held HMAC keystore, AES-256-GCM at-rest content encryption, dual DuckDB / PostgreSQL backends, and the `mnemo-compliance` crate — sit under the **Enterprise Readiness** priority area as an *attestable memory* layer regulated-workflow buyers can defend today.

This is a spec-context anchor, not a compliance claim. The roadmap's Transport Evolution work (stateless Streamable HTTP + `.well-known` server discovery) is upstream of mnemo and tracked via the `rmcp = "1.3"` workspace dep — mnemo follows `rmcp`'s SEP implementation as it lands rather than racing the spec. See [`docs/src/integrations/mcp-server.md`](docs/src/integrations/mcp-server.md) §"MCP 2026 Roadmap alignment" for the four-priority-area mapping table.

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
await client.share({ memory_id: id, target_agent_id: "auditor-agent" });

await client.close();
```

### Go

```go
import "github.com/sattyamjjain/mnemo/sdks/go"

client, err := mnemo.NewClient(mnemo.ClientOptions{DbPath: "agent.mnemo.db"})
defer client.Close()

result, _ := client.Remember(mnemo.RememberInput{Content: "User prefers dark mode"})
memories, _ := client.Recall(mnemo.RecallInput{Query: "user preferences"})
_, _ = client.Share(mnemo.ShareInput{MemoryID: result.ID, TargetAgentID: "auditor-agent"})
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
- **Feedback-driven consolidation trigger** — opt-in `ConsolidationPolicy::MaturityDriven` gates `run_consolidation` on a per-cluster maturity score (recency × hit-success × edge-degree × redundancy) instead of firing on a fixed schedule. Inherited by `forget` and `checkpoint` automatically across MCP / REST / gRPC / pgwire; the default `FixedSize` policy preserves the v0.4.x behaviour byte-for-byte. New in v0.4.10. <!-- prior art: FluxMem, arXiv:2605.28773 — structural cousin only, not a reproduction. -->
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
- **GEM trajectory-correctness audit** — `mnemo-compliance::trajectory_audit` replays the hash-chained event log and reports four trajectory-level signals: (a) unregulated-growth (active-bank vs ceiling), (b) missing-semantic-revision (facts superseded but never revised), (c) capacity-driven-forgetting (deletes outside the 5 named strategies), (d) read-only-retrieval (scopes that only RECALL). Surfaced via `mnemo.trajectory_audit` (MCP), `POST /v1/compliance/trajectory_audit` (REST), and the `TrajectoryAudit` gRPC RPC — same `(agent_id, thread_id)` shape as `mnemo.verify`, on the orthogonal trajectory axis. <!-- anchor: GEM, arXiv:2605.26252 — prior art only, structural cousin. -->
- **MemFail per-operation fault-isolation suite** — `mnemo_core::eval::memfail` decomposes each end-to-end recall into the three operation seams mnemo exposes (`remember` = store, `run_consolidation` = summarize, `recall` = retrieve) and ships three adversarial probe sets plus a canonical stale-context fixture that attributes a stale-recall failure to retrieve when store + summarize check out. Designed for `cargo test` and as a reusable library for downstream eval harnesses. New in v0.4.11.
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
- **MCP role-aware tool filter** — manifest `[role_filter]` block with `caller_roles`, `default = "allow_all" | "deny_all"`, per-tool `allow` / `deny` maps (deny wins), and an `McpRoleDenied` audit row on every blocked call. Aligned with the [2025-11-25 MCP authorization spec](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization). Omitting the block keeps pre-v0.4.2 behaviour byte-for-byte. New in v0.4.2.

### Memory curation interop (Dreams, Routines, and substrate primitives)

Anthropic's [Dreams Research Preview](https://platform.claude.com/docs/en/managed-agents/dreams) (surfaced 2026-05-06 at Code w/ Claude SF) is a managed-agent feature that "lets Claude reflect on past sessions to curate an agent's memory and surface new insights." Its companion [Routines doc](https://code.claude.com/docs/en/routines) describes the long-horizon agents that *consume* curated memory. mnemo's REMEMBER / RECALL / FORGET / SHARE primitives, envelope provenance, and AES-256-GCM at-rest encryption are the substrate any such curator reads from and writes through — Dreams owns *what to curate*, mnemo owns *how to durably store with audit trail*. The two surfaces are complementary, not substitute.

**Honest framing:** the Dreams API itself is a Research Preview behind a Request-access form, and **mnemo does NOT today ship an Anthropic-API adapter**. Today's anchor is substrate-level interop documentation, not integration. A `mnemo-dreams` adapter crate is plausible if/when the API exits Research Preview, but is explicitly NOT in scope for v0.4.x. See [`docs/comparisons/anthropic-dreams.md`](docs/comparisons/anthropic-dreams.md) for the curator-action ↔ mnemo-primitive layering table.

## Why mnemo when Cloudflare Agent Memory exists?

Cloudflare announced Agent Memory GA during [Agents Week (2026-04-30)](https://www.cloudflare.com/agents-week/updates/),
followed by Workers AI inference, Email Service beta, and an Agents
SDK preview. It is the closest hosted competitor to mnemo.

mnemo is an embedded, cryptographically-audited, replayable memory
the regulator can inspect offline. Cloudflare optimises recall
throughput on the edge runtime; mnemo optimises a memory whose every
write is HMAC-chained, every read is provenance-signed, and whose
storage layer survives outside any cloud's audit boundary.

Honest concession: on per-recall p50 against the Workers KV+Vectorize
backend, edge-recall throughput likely favours Cloudflare. mnemo's
axis is provenance, chain replay, point-in-time `as_of`, evidence-
weighted conflict resolution, DPDPA / GDPR subject erasure with audit
preservation, and the v0.4.2 MCP role-aware tool filter — surfaces
that matter when an auditor or regulator must reconstruct exactly what
an agent saw and decided, three months later, without depending on a
cloud account staying live.

The full bench harness against Cloudflare Agent Memory ships in v0.4.3
as a `mnemo-bench-cf` crate. Until then,
[`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md)
documents the differentiation scenario list with empty-bench
placeholders so the comparison's contract is explicit before the
numbers land.

Retrieval-strategy framing matters here too: [arXiv 2605.15184](https://arxiv.org/abs/2605.15184)
(Sen et al., May 2026) measured BM25 keyword retrieval outperforming
pure vector retrieval on its experiment-1 corpus inside an agent
harness. mnemo's documented default — hybrid RRF over BM25 + vector
+ graph + recency — is already hedged against the vector-first
default the paper questioned. v0.4.4 adds a typed
`RetrievalMode::HarnessAware { harness, format }` variant that lets
the response envelope be reshaped per agent harness (Claude Code,
Codex, Gemini CLI, Chronos, generic) without changing which records
the substrate retrieves. See
[`docs/research/grep-vs-vector-2605.15184.md`](docs/research/grep-vs-vector-2605.15184.md)
for the composition anchor + the explicit non-overclaim disclaimer.

Outcome diffing — reconstructing the artifact's full provenance from
append-only events — is the third trust wall in production agent
systems (alongside aligned-by-training intent and policy-mediated
action). The DELEGATE-52 delegation-corruption result
([arXiv 2604.15597](https://arxiv.org/abs/2604.15597), surfaced on
Hacker News 2026-05-09) puts a 25% baseline on the corruption rate
this layer needs to detect. mnemo's append-only event log + snapshot
substrate is the layer that lets a downstream auditor reconstruct any
artifact's full plan / input / trace / output tetrad and diff against
what the primary agent's plan asked for —
see [`docs/research/delegate52-2604.15597.md`](docs/research/delegate52-2604.15597.md)
for the operator recipe and the explicit non-overlap callout.

### Project Think — loop vs. ledger

Cloudflare extended this story on [2026-05-04](https://blog.cloudflare.com/project-think/)
with **Project Think**, a runtime story for AI agents built on Workers
+ DO Facets — the *durable agentic loop* itself. Project Think is
upstream of mnemo's surface: it owns where the agent runs and how the
loop survives a Worker restart. mnemo owns whether the writes that loop
emits are cryptographically chained, replayable months later, and
inspectable without a Cloudflare account.

These are **complementary, not substitute, surfaces.** An operator can
run their durable loop on Project Think + DO Facets and chain every
memory write into mnemo's HMAC ledger; the bench crate that compares
*Cloudflare Agent Memory vs mnemo as a memory store* does not redo
itself for *Project Think as a runtime vs mnemo as a memory ledger* —
the latter is a layering question, not a benchmark. See
[`docs/comparisons/cloudflare-project-think.md`](docs/comparisons/cloudflare-project-think.md)
for the full layering table and where each side wins.

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

### Cloudflare Workers deploy template (design anchor)

> **Status:** *design anchor*, not a shipped template. The `deploy/cloudflare/` scaffold is parked for v0.4.3 follow-up. This section documents the contract that scaffold will produce against — see [`docs/src/integrations/cloudflare-workers-deploy.md`](docs/src/integrations/cloudflare-workers-deploy.md) for the full design note.

[Cloudflare Durable Object Facets](https://blog.cloudflare.com/durable-object-facets-dynamic-workers/) (open beta, 2026-04-30) lets a single Worker dynamically load Durable Object classes, each with its own SQLite database. That's the per-tenant embedded-substrate shape mnemo already runs on DuckDB-per-agent — making Workers the natural managed runtime for an mnemo MCP server when you don't want to operate the box yourself.

The intended layout (single Worker, one DO Facet per tenant, mnemo as the MCP-over-HTTP entrypoint):

```toml
# wrangler.toml (sketch — not yet shipped under deploy/cloudflare/)
name = "mnemo-mcp-worker"
main = "dist/worker.js"

[[durable_objects.bindings]]
name = "MNEMO_TENANT"
class_name = "MnemoTenantFacet"
# DO Facet — each instance gets its own SQLite-backed storage
# matching mnemo's embedded DuckDB-per-agent contract.
```

What stays Rust-native vs. crosses the JS boundary, the file-format compatibility story (mnemo writes DuckDB; the Workers Facet exposes SQLite — the bench crate quantifies the gap), and which mnemo surfaces require the operator-held HMAC keystore vs. which can run inside the Worker — all in [`docs/src/integrations/cloudflare-workers-deploy.md`](docs/src/integrations/cloudflare-workers-deploy.md). The bench numbers land with the v0.4.3 `mnemo-bench-cf` crate.

## Development

```bash
# Run all tests (376 tests at v0.4.5: unit + integration + MCP + pgwire + REST + admin + gRPC + doctests)
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

**Embedding-backend selection bench + SLA-aware recommender (v0.4.9).** Anchored on [arXiv:2605.23618](https://arxiv.org/abs/2605.23618) (GE2 vs local encoders — quality + latency). New crate [`bench/embeddings`](bench/embeddings) measures every configured backend (Noop + bench-local hashing baseline always; OpenAI when keyed; ONNX when configured + feature-gated) for nDCG@10, recall@10, p50/p95 embed latency, and throughput at batch 1/8/32 on a 50-doc / 10-query labeled fixture; the recommender picks the highest-nDCG backend whose p95 ≤ the SLO and reports the nDCG gap vs the best-quality backend. Run with `mnemo bench embeddings --slo-ms <N>` or `cargo bench -p mnemo-embeddings-bench`. See [`bench/embeddings/README.md`](bench/embeddings/README.md) for the full "what this bench is NOT" block.

**First public LoCoMo number (v0.4.1, P0-1)** — full report at
[`docs/benchmarks/locomo-2026-04-28.md`](docs/benchmarks/locomo-2026-04-28.md).
mnemo joins the public LoCoMo board alongside MemMachine (84.87%,
2026-04-24) and Memori (81.95%, 2026-04-24); the harness ships at
[`bench/locomo`](bench/locomo) with a dual-judge variance gate
(GPT-5.1 + Claude-3.7 Sonnet) and runs nightly via
[`.github/workflows/locomo-nightly.yml`](.github/workflows/locomo-nightly.yml).
mnemo trades raw overall score for **temporal-slice strength + ~96% per-turn token cost** —
see the report for the honest pitch.

**Auto-Dreamer-shaped offline consolidation bench (v0.4.8).** Added 2026-05-25 at [`bench/locomo/src/bin/auto_dreamer_consolidation.rs`](bench/locomo/src/bin/auto_dreamer_consolidation.rs). Mirrors Auto-Dreamer's "smaller active bank, equal-or-better recall" axis against mnemo's existing `run_decay_pass` + `run_consolidation` path. Emits a Markdown report + a JSON summary (`active_bank_ratio`, `recall_pre`, `recall_post`) so the headline is citable. Defaults: 8 sessions × 25 facts × 5 trials. Run via `cargo run --release --bin auto_dreamer_consolidation -p mnemo-locomo-bench`. See [`bench/locomo/README.md`](bench/locomo/README.md#auto-dreamer-offline-consolidation--auto_dreamer_consolidation) for the full "what this bin is NOT" block.

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
