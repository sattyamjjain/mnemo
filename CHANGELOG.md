# Changelog

All notable changes to Mnemo are documented in this file.

## [0.3.0] - 2026-04-20

### Highlights

Auto-Dream-aware consolidation, Letta-style memory tiers, DPDPA +
EU AI Act compliance primitives, pgvector CVE-2026-3172 fix, and a
public LongMemEval / LoCoMo benchmark harness. Rolled up on top of
v0.2.0 (which was merged to main the same day).

### Added

- **Letta-style memory tiers** (`MemoryTier` type alias for the existing
  `MemoryType` enum; Working / Procedural / Semantic / Episodic). The
  engine now applies tier-specific behaviours on write: Working memories
  auto-expire after `ttl_working_seconds` (default 3600s) when no explicit
  ttl is given, and Procedural memories are clamped to the
  `procedural_importance_floor` (default 0.8) so system prompts never
  fall below recall visibility. New builder knobs
  `with_ttl_working_seconds` and `with_procedural_importance_floor`.
- **Auto-Dream-compatible reflection pass** —
  `engine.run_reflection_pass(agent_id)` performs date absolutization
  (regex rewrites `"yesterday"`, `"last week"`, `"N days ago"`, etc. to
  ISO-8601 anchored on `created_at`), accepts external rewrites
  (`metadata.dreamed_at`) and re-embeds, consolidates semantically
  near-duplicate records (`cosine ≥ 0.92`) into the newer record with
  merged tags + summed access_count, auto-resolves low-importance
  conflicts via `KeepNewest`, and archives stale low-importance
  records. Emits `ReflectionReport` with per-phase counts.
- **OpenAI Agents SDK GA snapshot store** —
  `mnemo.openai_sessions_ga.MnemoSnapshotStore` implements
  `save_snapshot` / `load_snapshot` / `list_snapshots` / `resume` plus
  `SnapshotRef` with a `snapshot://<session>/<ts>` URI. Pluggable
  `WorkspaceStorage` supports local FS today and stubs S3/R2/GCS/Azure
  behind the matching `mnemo[openai-sandbox-<backend>]` extras. Payloads
  above `inline_threshold_bytes` (default 64 KiB) offload to workspace;
  Mnemo keeps pointer + SHA-256 and verifies integrity on load.
- **DPDPA consent manager adapter** in the new `mnemo-compliance` crate
  — `ConsentSource` trait, `HttpConsentManager` (generic HTTP binding
  with optional bearer auth), `StaticConsentSource` (tests / single-
  tenant self-hosting). `ConsentState` carries scope list, expiry, and
  consent-token hash. `ComplianceError::ConsentDenied` surfaces cleanly.
- **EU AI Act audit export** — `export_audit_log(events, format, signer)`
  with two formats: `NdjsonSigned` (one JSON line per event plus a
  detached Ed25519 signature chain covering `SHA256(index ∥ prev_hash
  ∥ event_json)`; canonicalised through `serde_json::Value` so signer
  and verifier agree on bytes) and `EuAiOfficeCsv` (the AI Office GPAI
  template columns with RFC4180 escaping). `verify_ndjson_signed`
  walks the chain and rejects tampered rows with the offending index.
- **Benchmark harness** — `mnemo.benches.locomo_runner` (with CLI)
  runs `recall@5`/`recall@10`/MRR/p50/p95/p99 across
  `auto`/`vector_only`/`hybrid_rrf`/`graph_boosted` strategies and
  emits a Markdown report + JSON sidecar under `docs/benchmarks/`.
  Real dataset loaders stubbed behind the `mnemo[benchmark]` extra;
  first live numbers published in v0.3.0-rc2.

### Changed

- `pgvector` upgraded from 0.4 → 0.8.2 to pick up the fix for
  **CVE-2026-3172** (buffer overflow in parallel HNSW builds). Also
  enables `hnsw.iterative_scan` for strict-order filtered recall — the
  migration SQL will adopt it once PostgreSQL backends regenerate
  indexes.

### Carried forward from the unreleased v0.2.0

The full T1–T6 v0.2.0 feature set is included (Claude Agent SDK
adapter, OpenAI preview `Session` store, TTL sweeper,
GDPR-safe `forget_subject`, `replay(as_of=...)`, recall
`ScoreBreakdown` / `explain`). v0.2.0 was merged to main earlier today
via admin merge; the tag itself is skipped.

### Deferred to v0.3.0-rc2

- **rmcp 0.14 → 1.3 + MCP resource exposure** (prior T7). PR #27 stays
  open; the API migration is its own release.
- **DuckDB 1.4 → 1.5.2 + DuckLake opt-in backend** (Task 12b). Ships
  behind the `storage-ducklake` feature flag once the sorted-table +
  bucket-partitioning API lands.
- **First published LongMemEval / LoCoMo numbers**. The harness is
  shippable today; the datasets come with the `mnemo[benchmark]` extra.

## [0.2.0] - 2026-04-20

### Highlights

Claude Opus 4.7 + OpenAI Agents SDK first-class support, GDPR-safe subject
erasure, time-travel replay, and retrieval provenance.

### Added

- **Claude Agent SDK adapter** (`mnemo.claude_agent_sdk.MnemoClaudeMemory`).
  Exposes the full Mnemo MCP tool surface to `ClaudeAgentOptions.mcp_servers`
  and optionally materializes recalled memories into Markdown files with YAML
  frontmatter. A `watchdog` observer mirrors file edits, deletes, and
  frontmatter changes back into Mnemo so Opus 4.7's Auto Memory workflow and
  the persistent database stay in sync.
- **OpenAI Agents SDK `Session` store** (`mnemo.openai_sessions.MnemoSessionStore`).
  Implements the `get_items`/`add_items`/`pop_item`/`clear_session` protocol
  introduced in the 2026-04-15 release, storing each turn as a
  session-tagged episodic memory with a monotonic index so conversations
  survive process restarts.
- **TTL sweeper** (`engine.run_ttl_sweep`). Hard-deletes every memory whose
  `expires_at` is in the past and emits a `MemoryExpired` audit event per
  deletion, with correct hash chain linkage. The `mnemo` CLI gains
  `--ttl-sweep-interval` / `MNEMO_TTL_SWEEP_INTERVAL` that drives the sweeper
  as a background tokio task.
- **GDPR / DPDPA-aligned subject erasure** (`engine.forget_subject`). Finds
  memories tagged `subject:<id>` and either redacts the content (default,
  preserves the hash chain for audit) or hard-deletes them. Exposed via
  MCP (`mnemo.forget_subject`), REST (`POST /v1/forget_subject`), and gRPC
  (`ForgetSubject`). A new `ForgetStrategy::Redact` variant is also
  accepted wherever the standard `mnemo.forget` strategy parsing runs.
- **Point-in-time replay** (`ReplayRequest.as_of`). When set, the engine
  synthesizes a virtual checkpoint from the memories and events that
  existed at that timestamp and returns the reconstructed state. Exposed
  via MCP, gRPC (`ReplayRequest.as_of`), REST, and a new `as_of` kwarg on
  the PyO3 `replay` method.
- **Ranking-signal provenance on recall** (`RecallRequest.explain`). When
  `true`, each `ScoredMemory` carries a `ScoreBreakdown` reporting the
  per-signal contributions (vector, BM25, graph, recency) and the final
  RRF rank. Wired through MCP, REST (`?explain=true`), gRPC (`ScoreBreakdown`
  message + `ScoredMemory.score_breakdown`), and the PyO3 `recall(..., explain=True)`
  kwarg.
- `EventType::MemoryExpired` and `EventType::MemoryRedact` variants with
  snake_case `Display`/`FromStr` support, so the audit trail can
  distinguish natural expiration and subject redaction from ordinary
  deletes.
- Examples: `examples/claude_agent_sdk_example.py`,
  `examples/openai_agents_snapshot_example.py`.

### Changed

- `RecallRequest` gains `explain: Option<bool>`.
- `ReplayRequest` gains `as_of: Option<String>`.
- `ForgetStrategy` gains a `Redact` variant.
- `ScoredMemory` gains `score_breakdown: Option<ScoreBreakdown>` (skipped
  during serialization when absent — existing JSON consumers unaffected).
- Python `mnemo/__init__.py` now tolerates a missing native `_mnemo`
  extension at import time so adapter modules can be imported before
  `maturin develop` runs.

### Tests

All 36 integration tests, 70 mnemo-core unit tests, and the MCP / pgwire /
REST / admin / gRPC suites pass. Four new tests cover TTL sweep semantics,
GDPR-safe redaction (hash chain preservation), point-in-time replay, and
score-breakdown provenance. The Python adapters ship with 21 tests
(pure-Python + integration-gated) that run under `pytest python/tests/`.

### Deferred to 0.2.0-rc2 / 0.3.0

- `mnemo.reflect` Auto Dream equivalent (reflection-pass consolidation).
- rmcp 0.14 → 1.3 upgrade (PR #27) and MCP resource exposure — the API
  migration warrants a dedicated release.

## [0.1.0] - 2026-02-07

### Initial Release

Mnemo is an MCP-native memory database that gives AI agents persistent, searchable, and secure long-term memory.

### Highlights

- **10 MCP tools** for AI agents: remember, recall, forget, share, checkpoint, branch, merge, replay, delegate, and verify
- **Hybrid search** combining semantic vectors, BM25 keyword matching, knowledge graph signals, and recency scoring via Reciprocal Rank Fusion
- **Two storage backends**: embedded DuckDB for single-agent use and PostgreSQL with pgvector for distributed multi-agent deployments
- **SDKs** for Python (with OpenAI Agents, Mem0, LangGraph, and CrewAI adapters), TypeScript, and Go
- **Multiple access protocols**: MCP (stdio), REST API, gRPC, and PostgreSQL wire protocol

### Features

- **Memory lifecycle management** -- five forgetting strategies (soft delete, hard delete, decay, consolidation, archive), TTL-based expiration, and automatic decay passes
- **Security and integrity** -- AES-256-GCM at-rest encryption, SHA-256 hash chain integrity verification, RBAC with ACL-based permission filtering, memory poisoning detection, and delegation with depth-limited transitive permissions
- **Conflict resolution** -- automatic detection of contradictory memories with newest-wins, highest-importance, manual, and evidence-weighted resolution strategies
- **Branching and replay** -- checkpoint agent state, branch timelines, merge branches, and replay event history with hash chain verification
- **Causal debugging** -- trace event causality chains with configurable direction (up/down/both) and event-type filtering
- **Point-in-time queries** -- recall memories as they existed at any historical timestamp using `as_of`
- **Observability** -- OTLP span ingestion with OpenTelemetry GenAI semantic conventions, admin dashboard with agent statistics

### Infrastructure

- 9-crate Rust workspace with full CI (format, clippy, test, build, security audit)
- Helm chart for Kubernetes deployment with S3 cold-storage support
- Docker and Docker Compose configurations
- mdBook documentation site

---

## [0.1.1] - 2026-02-07

### Security

- **Fix SQL injection in PostgreSQL backend** -- replaced string-interpolated embedding values with parameterized `pgvector::Vector` bindings via sqlx
- **Add authentication to pgwire server** -- cleartext password authentication before connection acceptance; default bind changed from `0.0.0.0` to `127.0.0.1`
- **Harden CORS configuration** -- replaced permissive CORS with configurable origin allowlist via `MNEMO_CORS_ORIGINS` environment variable, defaulting to localhost only
- **Fix delegation authorization bypass** -- delegation endpoint now verifies the caller has `Delegate` permission on each target memory before creating delegations
- **Upgrade pyo3 to 0.24** -- fixes buffer overflow in `PyString::from_object` (RUSTSEC-2025-0020)
- **Upgrade tantivy to 0.25** -- resolves transitive `lru` crate unsoundness
- **Add constant-time hash comparison** -- all hash verification now uses `subtle::ConstantTimeEq` to prevent timing side-channel attacks
- **Sanitize error responses** -- internal error details are logged server-side; clients receive generic error messages
- **Add request body size limits** -- REST API enforces a 2 MB maximum request body to prevent denial-of-service via oversized payloads
- **Add prompt injection detection** -- memory content is now scanned for 11 common prompt injection patterns during anomaly scoring

### Improvements

- **Add CI security scanning** -- new cargo-audit job in GitHub Actions plus Dependabot for Cargo, npm, and GitHub Actions dependencies
- **Add agent_id input validation** -- agent identifiers are now validated for length (max 256 characters) and allowed characters (alphanumeric, hyphens, underscores, dots)
- **Add sync_metadata table to PostgreSQL migrations** -- ensures sync watermark operations work correctly in distributed deployments
- **Generate TypeScript SDK lockfile** -- `package-lock.json` committed for reproducible builds and `npm audit` support

### Documentation

- Remove hardcoded passwords from deployment examples -- Docker, Kubernetes, and PostgreSQL docs now use environment variable references
- Add CONTRIBUTING.md with contribution guidelines
- Add project memory configuration for development tooling
