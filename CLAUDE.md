# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

<!-- AUTO-MANAGED: project-description -->
## Overview

**Mnemo** — MCP-native memory database for AI agents, built in Rust.

Provides persistent, searchable, versioned memory with vector similarity, full-text search, graph relations, ACLs, encryption, and multi-agent support. Accessible via MCP (stdio), REST, gRPC, pgwire, and language SDKs (Python/TypeScript/Go).

<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: build-commands -->
## Build & Development Commands

```bash
# Build the workspace (excludes the maturin-built PyO3 crate — same as ci.yml)
cargo build --workspace --exclude mnemo-python

# Build release binary (crate dir is crates/mnemo-cli, published as `mnemo-mcp-server`)
cargo build --release -p mnemo-mcp-server

# Run the full test suite (excludes mnemo-python; run it to see the current count)
cargo test --workspace --exclude mnemo-python

# Run tests for a specific crate
cargo test -p mnemo-core
cargo test -p mnemo-mcp

# Run a single test by name
cargo test -p mnemo-core test_name

# Lint — mirrors CI. mnemo-grpc's build script needs `protoc` on PATH.
# Do NOT add --all-features. The mnemo-core `onnx` feature is NOT broken (it
# builds and tests against the pinned ort 2.0.0-rc.11 / ndarray 0.17 /
# tokenizers 0.23 API — issue #125 was the fix, not an open defect). CI keeps it
# out of the workspace-wide jobs purely for speed: `ort` compiles ONNX Runtime
# and downloads a prebuilt native lib. It has its own `onnx-feature` job so it
# cannot silently rot.
cargo clippy --all-targets --workspace --exclude mnemo-python

# Format check / apply
cargo fmt --all -- --check
cargo fmt --all

# Benchmarks (criterion, harness = false): engine_bench + longmemeval_bench
cargo bench -p mnemo-core

# Optional features
cargo build -p mnemo-core --features onnx            # ONNX local embeddings
cargo build -p mnemo-core --features s3              # S3 cold storage
cargo build -p mnemo-mcp-server --features postgres  # PostgreSQL backend
# mnemo-cli feature flags: rest (default), admin, pgwire, grpc, postgres

# mnemo-golem-wit lives in crates/ but is in `[workspace] exclude`, NOT members:
# its WASM-only host imports have no native definition, so a native cdylib link
# fails ("Undefined symbols … _cabi_post_mnemo:golem-vector/…"). Build it
# standalone — it carries its own [workspace].
cargo component build --release \
  --manifest-path crates/mnemo-golem-wit/Cargo.toml --target wasm32-wasip2

# Docker
docker build -t mnemo .

# Python SDK (PyO3 — DO NOT use cargo build, must use maturin)
cd python && maturin develop

# TypeScript SDK
cd sdks/typescript && npm install && npm test

# Go SDK
cd sdks/go && go test ./...

# Release / publish helpers (see scripts/)
./scripts/check_version_drift.sh     # crates.io vs workspace version guard
./scripts/registry_parity.sh --mode preflight  # registry triple + lag gate
python3 scripts/check_docs_links.py  # mdBook link check
```

**Environment variables for CLI** (each also has a `--flag` equivalent unless noted):
- `MNEMO_DB_PATH` — database file path (default: `mnemo.db`)
- `MNEMO_AGENT_ID` — default agent ID (default: `default`)
- `MNEMO_ORG_ID` — default organization ID
- `MNEMO_EMBEDDING_MODEL` — embedding model name (default: `text-embedding-3-small`)
- `MNEMO_DIMENSIONS` — embedding dimensions (default: `1536`)
- `OPENAI_API_KEY` — enables OpenAI embeddings
- `MNEMO_ONNX_MODEL_PATH` — enables local ONNX embeddings (takes priority over OpenAI)
- `MNEMO_POSTGRES_URL` — enables PostgreSQL backend instead of DuckDB
- `MNEMO_REST_PORT` — starts REST API alongside MCP stdio
- `MNEMO_ENCRYPTION_KEY` — AES-256-GCM key (64-char hex)
- `MNEMO_IDLE_TIMEOUT` — auto-shutdown after N seconds idle (0 = disabled)
- `MNEMO_TTL_SWEEP_INTERVAL` — seconds between TTL sweeps; a sweep hard-deletes expired memories and emits `MemoryExpired` audit events (0 = disabled)
- `MNEMO_EXPERIENCE_MEMORY` — env-only; `1`/`true` enables the experience-memory path
- `MNEMO_REJECT_INHERITED_SECRETS` — env-only; `0` opts out of the inherited-secret guard in `safe_spawn` (testing only)
- `MNEMO_PARENT_BASENAME` — env-only; parent process basename used for spawn attestation

**Environment variables for other surfaces**:
- `MNEMO_AUTH_TOKEN` — gRPC bearer secret; auth is enforced when set non-empty (`mnemo-grpc`)
- `MNEMO_CORS_ORIGINS` — REST CORS allowlist; defaults to localhost only (`mnemo-rest`)
- `MNEMO_TEST_POSTGRES_URL` — gates the live pgvector test; it skips (still green) when unset
- `MNEMO_LONGMEMEVAL_PATH` — swaps the fixture file for `longmemeval_bench`

<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: architecture -->
## Architecture

Cargo workspace with **31 members**: 21 crates under `crates/`, 9 bench crates under
`bench/`, and `python/`. A 22nd directory, `crates/mnemo-golem-wit`, sits in
`[workspace] exclude` and builds standalone (see Build Commands).

```
mnemo/
├── crates/
│   ├── mnemo-core/          # Core engine: storage, models, queries, indexing, search, encryption
│   │   └── src/
│   │       ├── model/       # MemoryRecord, AgentEvent, Relation, ACL, Checkpoint, Delegation,
│   │       │                #   AgentProfile, Capability, EmbeddingBaseline, WriteProvenance
│   │       ├── query/       # One file per engine op: remember, recall, forget, share, branch,
│   │       │                #   merge, replay, checkpoint, conflict, causality, consolidate,
│   │       │                #   evidence, experience, lifecycle, maturity, poisoning, reflection,
│   │       │                #   retained, retrieval, write_provenance, current_fact_resolver,
│   │       │                #   orientation_cache, event_builder
│   │       ├── storage/     # StorageBackend trait + DuckDB impl + migrations + cold storage (S3)
│   │       ├── index/       # VectorIndex trait + USearch HNSW impl (search/filtered_search are async)
│   │       ├── search/      # FullTextIndex trait + Tantivy impl
│   │       ├── embedding/   # EmbeddingProvider trait + OpenAI, ONNX, Noop impls
│   │       ├── sync/        # Multi-node sync engine with watermarks
│   │       ├── score/       # Recency/decay scoring
│   │       ├── anomaly/     # Outlier detection over embeddings
│   │       ├── budget/      # Token/context budget planner
│   │       ├── eval/        # In-tree eval harness (memfail)
│   │       ├── provenance.rs        # Write-provenance chain (principal/capability/session)
│   │       ├── opaque_reasoning.rs  # Opaque-reasoning-payload shape flag
│   │       ├── retrieval.rs         # ReasoningTrustPolicy + read-time trust filtering
│   │       ├── auth.rs      # RBAC / principal checks
│   │       ├── cache.rs     # In-memory LRU cache
│   │       ├── encryption.rs# AES-256-GCM at-rest encryption
│   │       ├── hash.rs      # Content hash chains + verification
│   │       └── error.rs     # Error enum + Result type alias
│   ├── mnemo-mcp/           # MCP server (rmcp 3.0, stdio transport) — 23 tools
│   │   └── src/
│   │       ├── server.rs        # ServerHandler + tool_router + tool_handler
│   │       ├── role_filter.rs   # Per-role tool gating (denied tool → -32601)
│   │       └── tools/           # One file per tool: remember, recall, forget, forget_subject,
│   │                            #   share, checkpoint, branch, merge, replay, delegate, verify,
│   │                            #   consolidate, experience, provenance, agent_managed,
│   │                            #   attention_state, trajectory_audit
│   ├── mnemo-cli/           # CLI binary (clap) — entry point; published as `mnemo-mcp-server`
│   ├── mnemo-postgres/      # PostgreSQL storage + pgvector index backend
│   ├── mnemo-rest/          # Axum 0.8 REST API (feature-gated, default-on in CLI)
│   ├── mnemo-admin/         # Admin dashboard API handlers
│   ├── mnemo-pgwire/        # PostgreSQL wire protocol (SQL-over-pgwire)
│   ├── mnemo-grpc/          # tonic gRPC service (14 RPCs; build script needs protoc)
│   ├── mnemo-graph/         # Bitemporal graph layer (Graphiti-style temporal edges)
│   ├── mnemo-compliance/    # DPDPA consent-manager + EU AI Act audit-log primitives
│   ├── mnemo-mesh/          # SPIFFE-style identity + per-namespace ACL
│   ├── mnemo-baseline/      # Per-agent rolling behavioural baseline (z-score + EWMA) → OTel/OCSF
│   ├── mnemo-attention-state/ # Attention-state-memory storage substrate
│   ├── mnemo-codemode/      # Code-mode recall — recall as a sandboxed WIT fn, not a JSON tool
│   ├── mnemo-deal/          # Chained-HMAC ledger of agent-on-agent deal envelopes
│   ├── mnemo-md-sync/       # Bidirectional sync: git-tracked Markdown wiki ↔ Mnemo
│   ├── mnemo-cma/           # Anthropic CMA-Memory compat shim (Markdown filesystem-of-memory)
│   ├── mnemo-amp/           # AMP / memorywire JSON-Schema 2020-12 interop adapter
│   ├── mnemo-letta/         # Letta-protocol-compatible REST surface
│   ├── mnemo-golem-host/    # Wasmtime host runner for the golem:vector WASM component
│   ├── mnemo-db/            # Name-reservation pointer crate — intentionally empty
│   └── mnemo-golem-wit/     # EXCLUDED from workspace — WASM component (cargo-component)
├── bench/                   # 9 cargo bench crates (most `publish = false`)
│   ├── locomo/              # LoCoMo authenticated nightly runner + cross-judge variance bands
│   ├── embeddings/          # Embedding-backend selection: recall@10/nDCG@10 + p50/p95 latency
│   ├── audit_conformance/   # Deterministic proof of the SHA-256 hash-chained write log
│   ├── audit_tamper/        # Adversarial tamper-evidence: delete/reorder/forge/truncate
│   ├── retention_conformance/ # Retention-floor harness over the append-only event log
│   ├── poisoning/           # Defense-delta ASR: quarantine ON vs OFF (MINJA/AgentPoison)
│   ├── asi06_poisoning/     # ASI06 cover-up/forgery resistance of the auditable layer
│   ├── salami_poisoning/    # Compositional ("Salami") poisoning — save + assembly rates (#37)
│   ├── forged_reasoning/    # Forged-chain-of-thought injection ASR, trust filter OFF vs ON
│   ├── state_bench/         # Python harness (NOT a cargo crate)
│   └── results/             # Committed dated JSON bench results
├── python/                  # PyO3 bindings + ~20 framework adapters (OpenAI Agents, CrewAI,
│                            #   LangGraph, AutoGen, DSPy, Agno, CAMEL, Letta, Mem0, ADK, …)
├── sdks/
│   ├── typescript/          # TypeScript REST client SDK
│   └── go/                  # Go REST client SDK
├── scripts/                 # Version-drift, release-parity, docs-link, generated-versions gates
├── examples/                # Python usage examples
├── deploy/helm/mnemo/       # Helm chart for Kubernetes
├── dashboards/              # Observability dashboards
├── docs/                    # mdBook (docs/book, docs/src) + adr, benchmarks, compliance,
│                            #   security, research, roadmap, comparisons, compat, release
└── .github/workflows/       # ci, security, docs, cargo-publish, release-crate, npm-publish,
                             #   pypi-publish, benchmarks-nightly, locomo-nightly, dco
```

**Key architectural patterns**:
- `MnemoEngine` is the central query coordinator — holds `Arc<dyn StorageBackend>`, `Arc<dyn VectorIndex>`, `Arc<dyn EmbeddingProvider>`, and optional components (full-text, encryption, cold storage, cache)
- Builder pattern: `engine.with_full_text(ft).with_encryption(enc).with_cache(c)`
- Each query operation lives in its own file under `query/` with an `execute(engine, request) -> Result<Response>` function
- Storage is trait-based (`StorageBackend`) — DuckDB and PostgreSQL implement it
- DuckDB connection: `Arc<Mutex<Connection>>` with `spawn_blocking` (not Send)
- `VectorIndex::search` / `filtered_search` are `async` (no `block_on` bridge — it panicked on a current_thread runtime); filter bound is `+ Send + Sync`
- `#[async_trait]` required for all async trait impls (Rust 2024 dyn-compat limitation)
- Error handling: `thiserror` enum in `error.rs`, `Result<T> = std::result::Result<T, Error>`
- Satellite crates (`mnemo-graph`, `mnemo-mesh`, `mnemo-cma`, `mnemo-amp`, `mnemo-letta`, …) layer over a `MnemoEngine` rather than reaching into storage directly

<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: conventions -->
## Code Conventions

- **Edition**: Rust 2024, stable toolchain
- **Naming**: snake_case for functions/variables, PascalCase for types/enums, SCREAMING_SNAKE for constants
- **Modules**: Flat `pub mod` re-exports in `mod.rs`, one file per logical unit
- **Traits**: Defined in `mod.rs`, implementations in dedicated files (e.g., `index/mod.rs` defines `VectorIndex`, `usearch.rs` implements it)
- **Async**: All storage/embedding/query functions are async, use `#[async_trait]` for trait defs
- **Error handling**: Return `crate::error::Result<T>`, convert external errors via `From` impls
- **Feature gates**: `#[cfg(feature = "onnx")]`, `#[cfg(feature = "s3")]` for optional deps
- **Dependencies**: Workspace-level dep management in root `Cargo.toml`, crates reference with `{ workspace = true }`
- **Testing**: `#[tokio::test]` with `tempfile` for isolated DB instances, tests at bottom of source files
- **MCP tools**: Use `Parameters<T>` wrapper for rmcp 0.14 inputs, `#[tool_handler]` on bare impl, `#[tool_router]` on method impl
- **CI**: `RUSTFLAGS="-Dwarnings"` — all warnings are errors

<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: patterns -->
## Detected Patterns

- **Request/Response pattern**: Every engine operation uses `FooRequest` → `execute(engine, req)` → `FooResponse`
- **Arc-wrapped traits**: All pluggable components held as `Arc<dyn Trait>` for shared ownership across async tasks
- **Builder composition**: `MnemoEngine::new(...)` returns a base engine; optional features added via `.with_*()` chaining
- **Soft delete**: Records use `deleted_at` field, `soft_delete_memory` vs `hard_delete_memory`
- **Hash chains**: Both memories and events maintain prev_hash chains for integrity verification
- **UUID v7**: All IDs use UUID v7 (time-sortable)
- **Permission-safe search**: ANN queries use iterative oversampling (3x → double) with post-filtering for ACL compliance
- **Feature-gated crates**: `mnemo-rest`, `mnemo-postgres` are optional deps in CLI; ONNX/S3 are features in core

<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: git-insights -->
## Git Insights

- Monorepo structure established from initial release
- Recent: dependency update (tonic 0.14, pyo3 0.28, reqwest 0.13, rand 0.9) + security hardening (30 fixes)
- CI enforces (see `.github/workflows/ci.yml`): `cargo fmt --all -- --check`, `cargo clippy --all-targets --workspace --exclude mnemo-python`, `cargo test --workspace --exclude mnemo-python`, `cargo build --workspace --exclude mnemo-python`, `cargo audit`, plus a live-pgvector Postgres job and a crates.io version-drift guard
- Apache-2.0 license

<!-- END AUTO-MANAGED -->

<!-- MANUAL -->
## Custom Notes

Add project-specific notes here. This section is never auto-modified.

<!-- END MANUAL -->
