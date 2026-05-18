# Mnemo Version Skew Matrix

> Updated 2026-05-17 for the v0.4.4 cut.

The matrix below pins which downstream and upstream versions are tested
together for each `mnemo` workspace release. Bumping any cell requires a
new workspace cut (the Cargo `workspace.package.version` is the
source-of-truth — see [CHANGELOG](../../CHANGELOG.md)).

## Server-side (Rust workspace + storage)

| `mnemo` (Cargo workspace) | `rmcp` | `tantivy` | `usearch` | `duckdb` | `pgvector` | `sqlx` | Cloudflare substrate ³ |
|---|---|---|---|---|---|---|---|
| **0.4.4** (2026-05-17) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.3 (2026-05-04) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.2 (2026-05-03) | 1.3 | 0.26 | 2.21 | 1.4 | 0.8.2 | 0.8 | Workers KV+Vectorize (anchor only) |

## SDK side (Python / TypeScript / Go + MCP SDKs)

| `mnemo` | Python SDK (`mnemo-db`) | TS SDK (`@mndfreek/mnemo-sdk`) | Go SDK (`mnemo.Version`) | `mcp-python` ⁵ | `mcp-go` ⁵ | `mcp-ruby` ⁵ | `mcp-csharp` ⁵ |
|---|---|---|---|---|---|---|---|
| **0.4.4** (2026-05-17) | 0.4.4 | 0.4.4 | 0.4.4 | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.4.3 (2026-05-04) | 0.4.3 | 0.4.3 | 0.4.3 | 1.13.x (2026-05-01) | 0.31.x (2026-05-01) | 0.5.x (2026-05-02) | 0.4.x (2026-05-02) |
| 0.4.2 (2026-05-03) | 0.4.2 | 0.4.2 | 0.4.2 | 1.12.x | 0.30.x | 0.4.x | 0.3.x |

## v0.4.4 retrieval surface (new in this release)

| Feature | Where it lives | API shape |
|---|---|---|
| `RetrievalMode` typed enum | [`crates/mnemo-core/src/retrieval.rs`](../../crates/mnemo-core/src/retrieval.rs) | 5 variants: `VectorOnly` / `Bm25Only` / `HybridRrf` / `Graph` / `HarnessAware { harness, format }` |
| Backwards-compat dispatch | [`crates/mnemo-core/src/query/recall.rs`](../../crates/mnemo-core/src/query/recall.rs) — `execute()` prefers `mode` over legacy `strategy` string | `RecallRequest.mode: Option<RetrievalMode>` (additive); `RecallRequest.strategy: Option<String>` unchanged |
| 5 starter `HarnessAware` adapters | `retrieval.rs` — `ClaudeCodeEnvelope`, `CodexEnvelope`, `GeminiCliEnvelope`, `ChronosEnvelope`, `GenericEnvelope` | `trait HarnessEnvelope { fn shape(&self, hits: &[ScoredMemory]) -> String; }` |
| Research anchor | [`docs/research/grep-vs-vector-2605.15184.md`](../research/grep-vs-vector-2605.15184.md) | Composition anchor, not implementation claim |
| Bench scaffold | [`bench/locomo/src/bin/grep_vs_vector_replay.rs`](../../bench/locomo/src/bin/grep_vs_vector_replay.rs) | Routes LongMemEval-shaped slice through 3 modes; smoke metric only (gated full run = #44) |

**SDK callers are NOT affected by v0.4.4.** The Python / TypeScript /
Go SDKs continue to marshal `strategy: string` and continue to work
unchanged — the new `mode` field is purely additive on
`RecallRequest`. SDK migration to a typed `mode` field is a v0.5.x
follow-up.

## History

| `mnemo` | `rmcp` | `tantivy` | `usearch` | `pgvector` | `sqlx` | Python | TypeScript | Go |
|---|---|---|---|---|---|---|---|---|
| 0.4.1 (2026-04-28) | 1.3 | 0.26 | 2.21 | 0.8.2 | 0.8 | 0.4.1 | 0.4.1 | 0.1.0 ¹ |
| 0.4.0 (2026-04-27) | 1.3 | 0.26 | 2.21 | 0.8.2 | 0.8 | 0.4.0 | 0.4.0 | 0.1.0 ¹ |
| 0.3.2 (2026-04-23) | 0.14 → 1.3 ² | 0.26 | 2.21 | 0.8.0 | 0.8 | 0.3.2 | 0.3.2 | 0.1.0 ¹ |

¹ Pre-v0.4.2 the Go SDK reported `clientInfo.version = "0.1.0"` on MCP
  initialize. v0.4.2 introduces a `Version` const that tracks the
  workspace, so older Go binaries pinning `0.1.0` are still compatible
  with `mnemo` ≥ 0.4.2 servers (the field is informational; the MCP
  protocol version is unchanged at `2024-11-05`).

² `mnemo` 0.3.2 shipped the `rmcp` 0.14 → 1.3 upgrade as a single
  release; consumers building against 0.3.x should pin `rmcp = "1.3"`
  to match.

³ **Cloudflare substrate** — `mnemo-bench-cf` (parked for v0.4.3) will
  baseline mnemo against **two** Cloudflare substrates rather than
  one: (a) the hosted Agent Memory service (Workers KV + Vectorize)
  per the 2026-04-30 GA, and (b) the Durable Object Facets open beta
  (SQLite-per-DO) announced 2026-04-30 — see
  [`docs/comparisons/cloudflare-agent-memory.md`](../comparisons/cloudflare-agent-memory.md)
  S1 and S1.5. The "+" notation in the v0.4.3 row signals both
  substrates are baseline targets, not that mnemo runs on both
  natively today.

⁴ **DuckDB 1.5.2 file-format breaking change.** v0.4.3 bumps `duckdb`
  from `1.4` to `1.10502.0` (the upstream calendar-encoded version
  for DuckDB 1.5.2). On-disk files written by 0.4.3+ are not readable
  by 0.4.2 binaries — see the v0.4.3 BREAKING note in CHANGELOG for
  the upgrade procedure.

⁵ **MCP SDK matrix** — these are the **client-side** SDKs from
  `https://github.com/modelcontextprotocol`, distinct from the
  server-side `rmcp` Rust crate that mnemo's MCP server is built on.
  Each SDK consumes the same MCP wire protocol (currently
  `2024-11-05` with the 2025-11-25 authorization spec layered on
  top); a SDK-side bump rarely requires a mnemo-side rev unless the
  spec itself changes. The 2026-05-01 / 2026-05-02 SDK refresh is
  pure client-side; mnemo's `rmcp = "1.3"` pin is unaffected.

## How to verify

CI enforces the matrix via three regression tests:

- `crates/mnemo-core/tests/version_metadata.rs` — fails if
  `env!("CARGO_PKG_VERSION")` drifts from the matrix's "Current" row.
- `python/tests/test_version_alignment.py` — fails if
  `mnemo.__version__` does not match the Cargo workspace version
  parsed at test time.
- `crates/mnemo-mcp/tests/sdk_matrix_doc_present.rs` (v0.4.3) — fails
  if this matrix file is missing or loses the `mcp-python` /
  `mcp-go` / `mcp-ruby` / `mcp-csharp` columns. Catches accidental
  doc deletion ahead of an SDK-bump release.

The TypeScript SDK's `package.json` `version` and the Go SDK's
`mnemo.Version` are checked at release time by the GitHub Actions
publish workflows (`.github/workflows/npm-publish.yml`,
`.github/workflows/cargo-publish.yml`).
