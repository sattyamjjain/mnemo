# mnemo vs Cloudflare Agent Memory — long-form comparison

> Living doc, last updated 2026-05-03 for the v0.4.2 cut. The full
> bench numbers ship in v0.4.3 as the `mnemo-bench-cf` crate. Until
> then, this file is the **contract** that the bench will produce
> against — every "TBD" placeholder below corresponds to a metric the
> v0.4.3 harness will fill in.

## Why this comparison exists

Cloudflare Agent Memory went GA during [Agents Week 2026](https://www.cloudflare.com/agents-week/updates/)
on 2026-04-30 (Workers AI inference layer, Email Service beta, Agents
SDK preview followed in the same week). It is the closest hosted
competitor to mnemo's embedded memory database. Operators evaluating
both have a fair question: "given Cloudflare's edge runtime, why pay
the embedded-DB tax?"

The answer is that they optimise different axes. We document those
axes here so the trade-off is explicit before any benchmark is run.

## Honest concession up front

On per-recall p50 against Cloudflare Workers KV + Vectorize, edge
recall is **likely faster** than mnemo's local DuckDB path. mnemo
will not "beat" Cloudflare on raw recall throughput at the edge — it
isn't trying to.

What mnemo provides instead is a memory whose **every write is
HMAC-chained, every read is provenance-signed, every retraction is
auditable, and whose storage survives any specific cloud account**.

## Differentiation scenarios

### S1 — Recall p50 / p99

| System | Recall p50 | Recall p99 | Notes |
|---|---|---|---|
| Cloudflare Agent Memory (Workers KV + Vectorize) | TBD (v0.4.3 bench) | TBD | Edge-cached recall path; hosted vector index |
| Cloudflare DO Facets (SQLite-per-DO) | TBD (v0.4.3 bench) | TBD | Per-tenant embedded SQLite — closest analogue to mnemo's per-agent DuckDB; open beta 2026-04-30 ([source](https://blog.cloudflare.com/durable-object-facets-dynamic-workers/)) |
| mnemo (DuckDB embedded) | ~4ms (LoCoMo nightly) | TBD | Local DuckDB + USearch |

**Honest call:** Cloudflare KV+Vectorize likely wins on edge throughput. DO Facets SQLite vs DuckDB-per-agent is the substrate-level comparison — same per-tenant embedded shape, different engine.

### S1.5 — DO Facets SQLite-per-DO vs mnemo DuckDB-per-agent (substrate-level)

[Cloudflare Durable Object Facets](https://blog.cloudflare.com/durable-object-facets-dynamic-workers/) (open beta, 2026-04-30) is the closest substrate analogue to mnemo today: each Facet is dynamically loaded with a private SQLite database, the same per-tenant-embedded shape mnemo runs on DuckDB. The comparison sharpens to four concrete axes:

| Axis | DO Facets SQLite-per-DO | mnemo DuckDB-per-agent |
|---|---|---|
| Per-tenant footprint | One SQLite file per Facet (managed by Cloudflare) | One `*.mnemo.db` DuckDB file per agent (operator-held) |
| Cold-start | Facet instantiation is dynamic (paper claim: "load and instantiate dynamically") — TBD measured | DuckDB `Connection::open` against a fresh file: ~1-3ms locally; pre-warmed in mnemo's MCP server | 
| Persistence boundary | Cloudflare-account-bound — survives Worker version upgrades, but exit requires Cloudflare's export tooling | Operator-held DuckDB file — copy-the-file is the entire export, no platform-specific tooling required |
| Audit-replay shape | Workers Audit Log + DO event metadata; cryptographic receipt only if the operator builds it | First-class: every `recall(..., with_provenance=true)` returns an HMAC-SHA256 receipt verifiable with `mnemo.provenance.verify_read_provenance` purely offline |

Bench numbers: TBD pending the `mnemo-bench-cf` crate (parked for v0.4.3). The crate will run identical workload against both substrates and quantify cold-start + per-tenant footprint + cross-Facet leak probe + sovereignty round-trip. This row's contract is what the bench harness produces against.

### S2 — FORGET residual probe

After issuing a `forget` for a memory, can a follow-up `recall` from a
different agent (or the same agent on a fresh session) still surface
the deleted content via vector hit, BM25 hit, graph traversal, or
admin export?

| System | Vector residual | BM25 residual | Graph residual | Admin export residual |
|---|---|---|---|---|
| Cloudflare Agent Memory | TBD | TBD | TBD | TBD |
| mnemo | None (audit hash chain preserved on `redact`) | None | None | None — `forget_subject` cascades through ACL+graph; persistence-version stamp guards against rollback |

### S3 — Chain audit replay

Given a 90-day-old write, can the operator reconstruct the exact
content the agent saw, the exact provenance signature, and the exact
ACL state at the time the call was made — *offline*, without contacting
the original cloud account?

| System | Offline replay possible | Cryptographic receipt verifiable offline | Replay at point-in-time `as_of` |
|---|---|---|---|
| Cloudflare Agent Memory | TBD (depends on Workers Audit Log retention + access) | TBD | TBD |
| mnemo | Yes — `mnemo replay --as-of` against the local DB + audit log | Yes — `mnemo.provenance.verify_read_provenance` is pure offline | Yes — `RecallRequest::as_of` is a first-class API field |

### S4 — Cross-agent leak probe

Can agent A's memory leak to agent B via shared infrastructure (cache
keys, embedding-index collisions, audit-log cross-reads, retention-
policy-bypass) — independent of correct ACL programming?

| System | Cache-key collision | Vector-index collision | Audit log cross-read | Retention bypass |
|---|---|---|---|---|
| Cloudflare Agent Memory | TBD | TBD | TBD | TBD |
| mnemo | None — per-agent namespace, permission-safe ANN with iterative oversampling + post-filtering | None — UUID v7 ID space, no cache keying on agent-namespaced UUIDs | None — append-only audit log enforced by DB triggers (PostgreSQL backend) | None — `forget_subject` honours `redact` strategy that preserves the chain |

### S5 — Sovereignty round-trip

Can the operator exit the platform and continue operating against a
self-hosted instance carrying every signed memory, every chain link,
and every ACL — without re-issuing keys, re-signing history, or
losing the chain-of-custody?

| System | Export with chain | Re-import at another instance | HMAC chain survives transit | Receipts remain verifiable |
|---|---|---|---|---|
| Cloudflare Agent Memory | TBD | TBD | TBD | TBD |
| mnemo | Yes — DuckDB file is the entire database; `mnemo verify` runs on any host | Yes — copy the file, point `mnemo` at it | Yes — chain is content-hash; transport-agnostic | Yes — provenance keystore is operator-held, rotation supported |

### S6 — Role-aware tool exposure (v0.4.2 — A1)

Can the platform's MCP server hide the destructive tools (`forget`,
`forget_subject`, `delegate`) from a read-only auditor caller while
keeping `recall` and `verify` reachable, with every blocked call
producing a tamper-evident audit row?

| System | Per-tool role gate | Audit-on-deny | Spec-aligned (MCP authz 2025-11-25) |
|---|---|---|---|
| Cloudflare Agent Memory | TBD (depends on Workers binding-level auth) | TBD | TBD |
| mnemo | Yes — manifest `[role_filter]` block with `allow` / `deny` maps | Yes — `McpRoleDenied { caller_id, tool_name, attempted_at, reason }` to `audit_log_path` | Yes — see [2025-11-25 spec](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization) |

## When Cloudflare is the right choice

- The workload is dominated by **edge-region recall throughput**.
- The agent runs **inside Workers** and Cloudflare-hosted memory is on
  the same data plane.
- The compliance posture is **"trust the cloud's audit log"** rather
  than "operator-held cryptographic receipt."

## When mnemo is the right choice

- The compliance posture requires **offline replay** of any agent's
  decision against an HMAC-signed history.
- The deployment surface includes **on-prem, sovereign, or air-gapped**
  environments where the storage cannot be a hosted service.
- The agent must **survive the cloud-account boundary** (org changes,
  billing changes, region changes, vendor exits).
- The threat model includes **DPDPA / GDPR subject erasure with audit
  trail preservation** (`forget_subject --strategy redact`).

## v0.4.3 plan

The `mnemo-bench-cf` crate (deferred from the 2026-05-02 prompt) will:

1. Spin up a real Cloudflare Agent Memory tenant.
2. Run the same write/recall workload against both backends.
3. Fill in the **TBD** rows in S1-S6 above.
4. Publish the trace SHA-256s alongside the report so anyone can
   recompute.

Until then, **this document is the contract**. Cloudflare wins on
edge-recall p50; mnemo wins on every audit / sovereignty / chain row.
The bench will quantify the magnitude.

## Sources

- Cloudflare Agents Week wrap — https://www.cloudflare.com/agents-week/updates/ (2026-04-29)
- Cloudflare Agent Memory GA blog — https://blog.cloudflare.com/agents-week-in-review-2026-04-30/ (carry context, 2026-04-30)
- MCP Authorization spec (2025-11-25) — https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization
- LoCoMo nightly methodology — [`docs/benchmarks/locomo-2026-04-28.md`](../benchmarks/locomo-2026-04-28.md)
