<!-- AUTO-MANAGED: project-description -->
## Overview

**Mnemo** — MCP-native memory database for AI agents, built in Rust.

Provides persistent, searchable, versioned memory with vector similarity, full-text search, graph relations, ACLs, encryption, and multi-agent support. Accessible via MCP (stdio), REST, gRPC, pgwire, and language SDKs (Python/TypeScript/Go).

<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: build-commands -->
## Build & Development Commands

```bash
# Build entire workspace
cargo build --all

# Build release binary
cargo build --release -p mnemo-cli

# Run all tests (132 tests: unit + integration + MCP + pgwire + REST + admin + gRPC + doctests)
cargo test --all

# Run tests for a specific crate
cargo test -p mnemo-core
cargo test -p mnemo-mcp

# Run a single test by name
cargo test -p mnemo-core test_name

# Lint
cargo clippy --all-targets --all-features

# Format check / apply
cargo fmt --all -- --check
cargo fmt --all

# Run benchmarks
cargo bench -p mnemo-core

# Build with optional features
cargo build -p mnemo-core --features onnx        # ONNX local embeddings
cargo build -p mnemo-core --features s3           # S3 cold storage
cargo build -p mnemo-cli --features postgres      # PostgreSQL backend

# Docker
docker build -t mnemo .

# Python SDK (PyO3 — DO NOT use cargo build, must use maturin)
cd python && maturin develop

# TypeScript SDK
cd sdks/typescript && npm install && npm test

# Go SDK
cd sdks/go && go test ./...
```

**Environment variables for CLI**:
- `MNEMO_DB_PATH` — database file path (default: `mnemo.db`)
- `OPENAI_API_KEY` — enables OpenAI embeddings
- `MNEMO_ONNX_MODEL_PATH` — enables local ONNX embeddings (takes priority over OpenAI)
- `MNEMO_POSTGRES_URL` — enables PostgreSQL backend instead of DuckDB
- `MNEMO_REST_PORT` — starts REST API alongside MCP stdio
- `MNEMO_ENCRYPTION_KEY` — AES-256-GCM key (64-char hex)
- `MNEMO_IDLE_TIMEOUT` — auto-shutdown after N seconds idle (0 = disabled)

<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: architecture -->
## Architecture

```
mnemo/
├── crates/
│   ├── mnemo-core/          # Core engine: storage, models, queries, indexing, search, encryption
│   │   └── src/
│   │       ├── model/       # Data types: MemoryRecord, AgentEvent, Relation, ACL, Checkpoint, Delegation
│   │       ├── query/       # Engine operations: remember, recall, forget, share, branch, merge, replay, conflict, causality
│   │       ├── storage/     # StorageBackend trait + DuckDB impl + cold storage (S3)
│   │       ├── index/       # VectorIndex trait + USearch HNSW impl
│   │       ├── search/      # FullTextIndex trait + Tantivy impl
│   │       ├── embedding/   # EmbeddingProvider trait + OpenAI, ONNX, Noop impls
│   │       ├── sync/        # Multi-node sync engine with watermarks
│   │       ├── cache.rs     # In-memory LRU cache
│   │       ├── encryption.rs# AES-256-GCM at-rest encryption
│   │       ├── hash.rs      # Content hash chains + verification
│   │       ├── config.rs    # Decay/consolidation config
│   │       └── error.rs     # Error enum + Result type alias
│   ├── mnemo-mcp/           # MCP server (rmcp 0.14, stdio transport)
│   │   └── src/
│   │       ├── server.rs    # ServerHandler + tool_router + tool_handler
│   │       └── tools/       # One file per MCP tool: remember, recall, forget, share, checkpoint, branch, merge, replay, delegate, verify
│   ├── mnemo-cli/           # CLI binary (clap) — entry point
│   ├── mnemo-postgres/      # PostgreSQL storage + pgvector index backend
│   ├── mnemo-rest/          # Axum 0.8 REST API (feature-gated)
│   ├── mnemo-admin/         # Admin dashboard API handlers
│   ├── mnemo-pgwire/        # PostgreSQL wire protocol (SQL-over-pgwire)
│   └── mnemo-grpc/          # tonic 0.12 gRPC service (11 RPCs)
├── python/                  # PyO3 bindings + OpenAI Agents + CrewAI + Mem0 compat
├── sdks/
│   ├── typescript/          # TypeScript REST client SDK
│   └── go/                  # Go REST client SDK
├── examples/                # Python usage examples
├── deploy/helm/             # Helm chart for Kubernetes
├── docs/                    # mdBook documentation
└── .github/workflows/       # CI: fmt, clippy, test, build
```

**Key architectural patterns**:
- `MnemoEngine` is the central query coordinator — holds `Arc<dyn StorageBackend>`, `Arc<dyn VectorIndex>`, `Arc<dyn EmbeddingProvider>`, and optional components (full-text, encryption, cold storage, cache)
- Builder pattern: `engine.with_full_text(ft).with_encryption(enc).with_cache(c)`
- Each query operation lives in its own file under `query/` with an `execute(engine, request) -> Result<Response>` function
- Storage is trait-based (`StorageBackend`) — DuckDB and PostgreSQL implement it
- DuckDB connection: `Arc<Mutex<Connection>>` with `spawn_blocking` (not Send)
- `#[async_trait]` required for all async trait impls (Rust 2024 dyn-compat limitation)
- Error handling: `thiserror` enum in `error.rs`, `Result<T> = std::result::Result<T, Error>`

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

- Single initial release commit — monorepo structure established from day one
- CI enforces: `cargo fmt --check`, `cargo clippy --all-features`, `cargo test --all`, `cargo build --all`
- Apache-2.0 license

<!-- END AUTO-MANAGED -->

<!-- MANUAL -->
## Custom Notes

Add project-specific notes here. This section is never auto-modified.

<!-- END MANUAL -->
