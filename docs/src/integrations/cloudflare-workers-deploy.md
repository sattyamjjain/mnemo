# Cloudflare Workers deploy template — design note

> **Status:** *design anchor*, not a shipped template. The `deploy/cloudflare/` scaffold is parked for v0.4.3 follow-up. This page is the contract that scaffold will produce against.
>
> **Primary anchor:** [Cloudflare Durable Object Facets — open beta (2026-04-30)](https://blog.cloudflare.com/durable-object-facets-dynamic-workers/).

## Why Workers, why now

Cloudflare Durable Object Facets shipped as open beta on 2026-04-30. Each Facet "load[s] and instantiate[s] a Durable Object class dynamically, while providing it with a SQLite database to use for storage" (per the announcement). That is exactly the per-tenant embedded-substrate shape mnemo already runs on DuckDB-per-agent — making Workers the natural managed runtime for an mnemo MCP server when an operator wants Cloudflare's edge boundary instead of running their own box.

The trade-off is honest: mnemo on Workers stops being a Rust-native binary and starts crossing the JS boundary (the Worker entrypoint is JS; the DO class is JS; mnemo's Rust core has to be exposed via WASM or via an HTTP shim). The bench numbers from `mnemo-bench-cf` (parked for v0.4.3) will quantify the gap. This page is the design contract those numbers run against.

## Intended layout (single Worker, one DO Facet per tenant)

```toml
# wrangler.toml (sketch — not yet shipped under deploy/cloudflare/)
name = "mnemo-mcp-worker"
main = "dist/worker.js"
compatibility_date = "2026-05-04"

[[durable_objects.bindings]]
name = "MNEMO_TENANT"
class_name = "MnemoTenantFacet"
# DO Facet — each instance gets its own SQLite-backed storage.
# One Facet ≈ one mnemo agent_id namespace.

[vars]
# HMAC keystore is operator-held — never lives in vars or secrets that
# the Cloudflare account boundary can read. See "Operator-held material"
# below.
MNEMO_DEFAULT_AGENT = "claude-prod"
```

## What stays Rust-native vs. crosses the JS boundary

| Surface | Where it lives | Notes |
|---|---|---|
| MCP transport (stdio → HTTP) | JS Worker | Worker entrypoint converts incoming HTTP to MCP JSON-RPC framing |
| Tool-router (`tools/list`, `tools/call`) | JS Worker | Pure routing; no compute — can stay JS |
| Recall + Remember + Forget query engine | Rust core (WASM) | Compiled with `wasm32-unknown-unknown`; today this requires a USearch-WASM port that doesn't exist yet — see "Open questions" |
| HMAC chain + provenance signing | Rust core (WASM) | Pure compute, no I/O; clean WASM boundary |
| Storage (DuckDB → SQLite) | JS Worker (DO Facet API) | DuckDB doesn't run in Workers; mnemo's storage layer would route to the Facet's SQLite via a `StorageBackend` trait impl |
| Vector index (USearch) | Rust WASM, Facet-stored | Today USearch is C++ via `usearch` crate — the WASM build is **the load-bearing missing piece** for v0.4.3 |
| Full-text (Tantivy) | Rust WASM | Tantivy compiles to WASM; should be straightforward |

## File-format compatibility

mnemo writes DuckDB; the Workers Facet exposes SQLite. They are **not** wire-compatible. The MCP server contract (REMEMBER / RECALL / FORGET / SHARE) is preserved, but **a memory written to a Workers-backed mnemo cannot be read by a self-hosted DuckDB-backed mnemo without an explicit `mnemo export` → `mnemo import` round-trip**. Document this loudly in the eventual scaffold's README.

`mnemo-bench-cf` will quantify whether the SQLite-per-DO recall p50 is within the 2× envelope of DuckDB-per-agent on the same workload. If not, the scaffold ships with explicit "do not use for latency-sensitive workloads" warnings.

## Operator-held material — what NEVER goes into Cloudflare

- **HMAC keystore** (the `keystore_path` in `examples/mcp-server/manifest.toml`). The provenance receipt's whole point is that it's verifiable offline by an auditor who does not have a Cloudflare account. If the keystore lives in Workers Secrets, the receipt's audit value collapses — Cloudflare can rotate or read the key. The keystore MUST stay on operator-held infrastructure (a separate signing service the Worker calls via mTLS, or pre-signed receipts uploaded by an operator-side batch job).
- **Audit log destination** — the `audit_log_path` should write to operator-held R2 / S3 / GCS, not to Workers Logs (which expire on Cloudflare's cadence and aren't HMAC-chained).

## What `mnemo-bench-cf` must measure

Once the scaffold lands, the bench crate (parked v0.4.3) must produce numbers for:

1. **Cold-start latency** — first recall against a freshly-instantiated Facet vs first recall against a freshly-opened DuckDB.
2. **Per-tenant footprint** — disk + memory at 10k / 100k / 1M memories.
3. **Cross-Facet leak probe** — the same probe `mnemo-bench-cf` runs against KV+Vectorize: does Facet A's vector index ever surface a hit from Facet B's writes? Workers' isolation guarantees say "no", but the harness measures.
4. **Cross-engine round-trip** — write through the Worker, export, re-import to self-hosted DuckDB; does `mnemo verify` re-confirm the chain?
5. **Sovereignty round-trip** — operator exits Cloudflare; does the Facet's SQLite export carry the HMAC chain into a fresh self-hosted instance with `mnemo verify` still passing?

These are the rows that today sit as `TBD (v0.4.3 bench)` in [`docs/comparisons/cloudflare-agent-memory.md`](../../comparisons/cloudflare-agent-memory.md).

## Open questions (parked for v0.4.3 owner-of-record)

1. **USearch on WASM.** Today no published USearch WASM build. Either (a) port the HNSW step to a pure-Rust crate that compiles to WASM, (b) ship without ANN on Workers and let the bench show what BM25-only recall costs, or (c) call back to a Rust-native sidecar service for vector search. The scaffold owner picks one.
2. **Tantivy on WASM.** Should compile, but compile time + bundle size matter. Measure before committing.
3. **DuckDB-on-WASM (`@duckdb/duckdb-wasm`).** An alternative to "Rust core writes to Facet SQLite via a `StorageBackend` impl" — the Worker could host a DuckDB-WASM instance per tenant and persist its file to the Facet's SQLite blob storage. Higher perf parity, but two storage engines and a WASM-on-WASM stack. Trade-off worth measuring.

## Runtime layer (Project Think)

[Cloudflare Project Think](https://blog.cloudflare.com/project-think/)
(2026-05-04) is the *runtime* story for AI agents on Workers + DO
Facets — the durable agentic loop itself. Even when the operator
deploys mnemo onto a Workers + DO Facets substrate following this
template, **the audit-ledger contract is operator-held and survives
independent of any Worker's lifecycle.** The HMAC chain, the
provenance signer's keystore, and `mnemo verify` all run outside
the Project Think runtime boundary by design.

The full layering rationale (where Project Think wins, where mnemo
wins, how they compose) is in [`docs/comparisons/cloudflare-project-think.md`](../../comparisons/cloudflare-project-think.md).
Short version: Project Think owns the loop, mnemo owns the
audit-ledger, the bench harness above does *not* re-run for Project
Think because the answer is layering, not benchmarking.

## Cross-references

- Substrate-level comparison: [`docs/comparisons/cloudflare-agent-memory.md`](../../comparisons/cloudflare-agent-memory.md) — the S1.5 row tracks the DO Facets SQLite vs DuckDB substrate axis.
- Runtime-layer comparison: [`docs/comparisons/cloudflare-project-think.md`](../../comparisons/cloudflare-project-think.md) — loop vs. ledger, layering not substitution.
- Comparable embedded-memory pitch in README: see the [`## Why mnemo when Cloudflare Agent Memory exists?`](../../../README.md#why-mnemo-when-cloudflare-agent-memory-exists) section, which already concedes edge-recall p50.
- v0.4.4 carry list: [`CHANGELOG.md`](../../../CHANGELOG.md) — `mnemo-bench-cf` is on the v0.4.4 backlog with the dependency note attached.
