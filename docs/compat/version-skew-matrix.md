# Mnemo Version Skew Matrix

> Updated 2026-07-24 for the **v0.5.16** cut — a **benchmark/docs** change: the
> ASI06 auditable memory-poisoning **resistance** benchmark
> (`mnemo-asi06-poisoning-bench`) over the shipped `hash::verify_chain` +
> `provenance::verify_read_provenance` primitives. **No public API, protocol, or
> storage change; no dependency change** to the published crates — version pins
> move 0.5.15 → 0.5.16 in lockstep.
>
> Updated 2026-07-21 for the **v0.5.15** cut — a **benchmark/docs** change: the
> first **real-embedder** LoCoMo retrieval bench (`locomo_v1_bench`, default local
> ONNX embedder + a hard anti-no-op guard) plus the `ort` 2.0.0-rc.11 migration of
> the ONNX embedder. **No public API, protocol, or storage change; no dependency
> change** to the published crates — version pins move 0.5.14 → 0.5.15 in lockstep.
>
> Updated 2026-07-19 for the **v0.5.14** cut — a **feature** change: a
> processing-log **retention-conformance profile** in `mnemo-compliance`
> (`RetentionProfile` — DPDP Rules 2025 / EU AI Act Art.19 / HIPAA §164.312(b)),
> a `StorageBackend::events_are_append_only()` capability, a `mnemo compliance
> retention` CLI command, and a `bench/retention_conformance` harness. **No
> dependency change from v0.5.13**; version pins move 0.5.13 → 0.5.14 in lockstep.
>
> Updated 2026-07-18 for the **v0.5.13** cut — a **correctness/safety** change:
> semantic / hybrid / `auto` / graph / domain-scoped recall now **fail loud**
> with `Error::EmbedderNotConfigured` when no real embedder is configured (the
> no-op embedder returns all-zero vectors), instead of silently returning empty.
> Lexical / exact recall are unchanged. **No dependency change from v0.5.12**; the
> version pins move 0.5.12 → 0.5.13 in lockstep.
>
> Updated 2026-07-13 for the **v0.5.12** cut — a **distribution-only** change
> (crates.io publishing metadata for the `mnemo-core` / `mnemo-attention-state` /
> `mnemo-compliance` / `mnemo-mcp` compliance line + a tag-triggered
> `release-crate.yml` publish workflow). **No dependency or API change from
> v0.5.11**, so every dependency column below is unchanged; the version pins move
> 0.5.11 → 0.5.12 in lockstep.
>
> Updated 2026-07-07 for the v0.5.11 cut (memory-poisoning **defense-delta**
> benchmark — a new `bench/poisoning` crate measuring ASR with the shipped
> poisoning-quarantine defense ON vs OFF for MINJA + AgentPoison-style attacks;
> bench-and-docs only, no dependency or API change from v0.5.10, no managed-cloud
> dep). The v0.5.10 cut added the LoCoMo **claimed-vs-observed**
> reproduction bench — a new `reproduction_bench` bin + a shared `dataset` loader
> + a byte-stable test + docs; bench-and-docs only, no dependency or API change
> from v0.5.9, no managed-cloud dep). The v0.5.9 cut added the regulated-memory
> **audit-conformance**
> artifact — a new offline `mnemo-audit-conformance-bench` crate + `docs/compliance/`
> Art.12/DPDP mappings + README positioning; bench-and-docs only, no dependency
> or API change from v0.5.8, no managed-cloud dependency added to core). The
> v0.5.8 cut added the reproducible **BEAM-style** multi-hop / open-domain
> retrieval bench (a new `beam_bench` bin + a shared `stats::wilson_95` helper +
> docs; bench-and-docs only, no dependency or API change from v0.5.7).
> The v0.5.7 cut added real **pgvector ANN search** on the
> PostgreSQL backend — semantic/hybrid/graph/domain-scoped recall now returns
> results via the HNSW cosine index; #99. Implements a previously-stubbed
> capability; no `VectorIndex` trait API change, no dependency change from
> v0.5.6). The v0.5.6 cut was the first memory-poisoning **resistance**
> micro-bench + OWASP **ASI06** mapping — a new bench bin + `docs/security/ASI06.md`
> + one README row; no new detector, no dependency or API change from v0.5.5).
> The v0.5.5 cut was the workspace-member drift fix (#74 — phantom crate
> references removed / relabelled Planned, a `README.md` crate-claim CI fence
> added; docs-and-tests only, no dependency or API change from v0.5.4). The
> v0.5.4 cut added bearer-token auth on REST + gRPC via
> `MNEMO_AUTH_TOKEN` → `401`/`UNAUTHENTICATED`, else open + warn; README
> security claims aligned to wired behavior + a "what is/isn't enforced today"
> table). No engine/protocol API break — additive `router_with_auth`. The
> v0.5.3 cut returned a typed `Error::BackendUnsupported` for Postgres semantic
> recall. The v0.5.2 cut added the
> real-embedder memory-quality result in
> [`bench/RESULTS.md`](../../bench/RESULTS.md). The
> v0.4.5 → v0.4.9 cuts are not reproduced here; consult the
> [CHANGELOG](../../CHANGELOG.md) for the per-cut substrate / SDK matrix in
> those windows.

The matrix below pins which downstream and upstream versions are tested
together for each `mnemo` workspace release. Bumping any cell requires a
new workspace cut (the Cargo `workspace.package.version` is the
source-of-truth — see [CHANGELOG](../../CHANGELOG.md)).

## Server-side (Rust workspace + storage)

| `mnemo` (Cargo workspace) | `rmcp` | `tantivy` | `usearch` | `duckdb` | `pgvector` | `sqlx` | Cloudflare substrate ³ |
|---|---|---|---|---|---|---|---|
| **0.5.11** (2026-07-07) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.10 (2026-07-06) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.9 (2026-07-05) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.8 (2026-07-04) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.7 (2026-07-04) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.6 (2026-07-02) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.5 (2026-07-03) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.4 (2026-06-27) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.3 (2026-06-23) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.2 (2026-06-22) | 1.3 | 0.26 | 2.21 | 1.10504.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.1 (2026-06-21) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.5.0 (2026-06-21) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.15 (2026-06-13) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.14 (2026-06-11) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.13 (2026-06-04) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.12 (2026-06-02) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.11 (2026-06-02) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.10 (2026-05-29) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.4 (2026-05-17) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.3 (2026-05-04) | 1.3 | 0.26 | 2.21 | 1.10502.0 ⁴ | 0.8.2 | 0.8 | Workers KV+Vectorize + DO Facets SQLite ³ |
| 0.4.2 (2026-05-03) | 1.3 | 0.26 | 2.21 | 1.4 | 0.8.2 | 0.8 | Workers KV+Vectorize (anchor only) |

## SDK side (Python / TypeScript / Go + MCP SDKs)

| `mnemo` | Python SDK (`mnemo-db`) | TS SDK (`@mndfreek/mnemo-sdk`) | Go SDK (`mnemo.Version`) | `mcp-python` ⁵ | `mcp-go` ⁵ | `mcp-ruby` ⁵ | `mcp-csharp` ⁵ |
|---|---|---|---|---|---|---|---|
| **0.5.11** (2026-07-07) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.10 (2026-07-06) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.9 (2026-07-05) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.8 (2026-07-04) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.7 (2026-07-04) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.6 (2026-07-02) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.5 (2026-07-03) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.4 (2026-06-27) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.3 (2026-06-23) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.2 (2026-06-22) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.1 (2026-06-21) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.5.0 (2026-06-21) ⁶ | (unchanged) | (unchanged) | (unchanged) | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.4.15 (2026-06-13) | 0.4.15 | 0.4.15 | 0.4.15 | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.4.14 (2026-06-11) | 0.4.14 | 0.4.14 | 0.4.14 | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.4.13 (2026-06-04) | 0.4.13 | 0.4.13 | 0.4.13 | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.4.12 (2026-06-02) | 0.4.12 | 0.4.12 | 0.4.12 | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.4.11 (2026-06-02) | 0.4.11 | 0.4.11 | 0.4.11 | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.4.10 (2026-05-29) | 0.4.10 | 0.4.10 | 0.4.10 | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
| 0.4.4 (2026-05-17) | 0.4.4 | 0.4.4 | 0.4.4 | 1.13.x | 0.31.x | 0.5.x | 0.4.x |
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

⁶ **v0.5.x are server-side cuts.** v0.5.0 adds the `consolidate`
  topic-document primitive; v0.5.1 adds the `reconstruct` recall
  strategy (active reconstruction, MRAgent arXiv:2606.06036); v0.5.2 is
  a bench + docs cut (real-embedder memory-quality result in
  `bench/RESULTS.md`, Postgres ANN stub hard-errors); v0.5.3 makes that
  Postgres error a typed `BackendUnsupported` variant + adds the README
  backend capability matrix; v0.5.4 adds bearer-token auth on REST/gRPC
  (`MNEMO_AUTH_TOKEN`) + aligns the README security claims with wired
  behavior — all with no API change
  — both v0.5.0/v0.5.1 land in the Rust workspace (crates.io) and the
  MCP/REST/gRPC(/pgwire) surfaces.
  The language SDKs (`mnemo-db` Python, `@mndfreek/mnemo-sdk` TS, Go) are
  **unchanged** and ship on their own cadence; `pip install mnemo-db`
  continues to report its existing version. SDK marshalling is unaffected
  because both changes are additive (a new primitive; a new opt-in
  `strategy` value).

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
