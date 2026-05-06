# mnemo vs Cloudflare Project Think — runtime layer vs audit-ledger layer

> Living doc, last updated 2026-05-05. Companion to
> [`cloudflare-agent-memory.md`](cloudflare-agent-memory.md) (the *memory
> store* comparison) and [`../src/integrations/cloudflare-workers-deploy.md`](../src/integrations/cloudflare-workers-deploy.md)
> (the *deploy template* design note). Where the agent-memory doc asks
> "should I store agent memory here or there?", this doc asks the more
> upstream question: "where does the agent loop run, and where does
> its audit trail live?"

## Why this comparison exists

Cloudflare announced **Project Think** on [2026-05-04](https://blog.cloudflare.com/project-think/),
positioned as "building the next generation of AI agents on Cloudflare."
The thesis is a *durable agentic loop* on Workers + DO Facets — a
runtime that survives Worker restarts, region failovers, and tenant
isolation boundaries. It is upstream of where mnemo sits.

The temptation is to read Project Think as a competitor. **It isn't.**
Project Think owns the runtime layer; mnemo owns the audit/replay
layer. Operators evaluating both have a layering question, not a
substitution question.

## Honest layering up front

| Layer | What lives there | Who owns it well today |
|---|---|---|
| Durable agent loop runtime | Where the agent's tick-by-tick execution survives a process restart | **Project Think** (Cloudflare) — first-class story; mnemo does not compete here |
| Per-tenant embedded storage substrate | The disk the agent's working memory writes to | **DO Facets SQLite** (Cloudflare) ↔ **DuckDB-per-agent** (mnemo). See `cloudflare-agent-memory.md` S1.5 for the substrate-level comparison |
| Hosted memory service | Edge-cached recall over a managed vector index | **Cloudflare Agent Memory** (KV+Vectorize). See `cloudflare-agent-memory.md` S1 |
| Cryptographic audit ledger | HMAC-chained writes + offline-verifiable provenance receipts + point-in-time `as_of` replay | **mnemo** — first-class story; Cloudflare's Workers Audit Log is *not* a chained-receipt substrate |

The honest mnemo positioning: an operator running their durable loop
on Project Think can **also** chain every memory write into mnemo's
HMAC ledger. The two are stacked, not exclusive.

## Where each side wins

### Project Think wins

- **Durable execution under Worker churn.** A 14-day agent loop that
  survives Worker version upgrades, region failovers, and tenant
  isolation boundaries — Cloudflare's runtime story. mnemo has no
  equivalent and isn't trying to.
- **Edge-region tick latency.** The loop ticks where the user is.
- **Hosted observability of the loop itself.** Cloudflare's tenancy
  and lifecycle metadata around the loop are first-class.
- **Zero-ops runtime.** No box for the operator to run.

### mnemo wins

- **Offline-verifiable provenance.** Every `recall(..., with_provenance=true)`
  returns an HMAC-SHA256 receipt verifiable with
  `mnemo.provenance.verify_read_provenance` purely offline — no
  Cloudflare account needed, three months later, by an auditor who
  was not on the original deployment.
- **Chain replay at point-in-time.** `mnemo replay --as-of <ts>` against
  the local DB + audit log reconstructs the exact memory the agent
  saw at any past moment. Cloudflare's Workers Audit Log retention
  cadence and access tooling do not produce this shape.
- **Sovereignty round-trip.** Operator exits Cloudflare; the DuckDB
  file is the entire database; `mnemo verify` runs on any host.
  Project Think's runtime state is Cloudflare-account-bound.
- **Cross-engine memory ledger.** A loop running on Project Think can
  chain into mnemo; a loop running on Temporal can also chain into
  mnemo; a loop running on AWS Step Functions can also chain into
  mnemo. mnemo's audit contract is runtime-agnostic by design.

## How they compose

The cleanest stack for an operator who wants both:

1. **Durable loop on Project Think + DO Facets.** Worker entrypoint
   converts incoming HTTP to MCP JSON-RPC framing; the DO Facet's
   SQLite stores per-tenant working memory.
2. **Audit-ledger writes go to operator-held mnemo.** Either via an
   HTTP shim that the Worker calls on every memory write, or as a
   batch job at the end of each loop tick. The HMAC keystore stays
   on operator-held infrastructure (Workers Secrets is *not* the
   right place — see [`cloudflare-workers-deploy.md`](../src/integrations/cloudflare-workers-deploy.md)
   §"Operator-held material").
3. **`mnemo verify`** runs against the chained-write ledger as a
   separate audit step — no Cloudflare account required.

The bench harness for *Cloudflare Agent Memory vs mnemo as a memory
store* (parked v0.4.4 backlog `mnemo-bench-cf`) **does not redo itself
for Project Think.** The latter is a layering question — solved by
the stacking pattern above — not a benchmark.

## When Cloudflare alone is the right choice

- The compliance posture is *"trust the cloud's audit log"* — Cloudflare's
  Workers Audit Log is sufficient for the operator's auditors.
- The agent's durable loop never needs to outlast a Cloudflare account
  boundary (org change, region migration, vendor exit).
- The agent's memory is fully ephemeral / per-session — no
  cross-session reconstruction requirement, no `as_of` replay, no
  provenance receipt that survives offline.

## When mnemo (stacked under Project Think) is the right choice

- The compliance posture requires offline replay of any agent's
  decision against an HMAC-signed history.
- The agent's audit trail must survive any specific cloud's
  retention cadence — including the cloud's own outages or
  account-level access changes.
- The agent's memory is part of a **multi-runtime** strategy (loop
  on Cloudflare today, loop on Temporal tomorrow, audit on a single
  operator-held mnemo throughout).
- The threat model includes DPDPA / GDPR subject erasure with audit
  trail preservation (`forget_subject --strategy redact` chain
  preserved).

## v0.4.4+ plan

- The `mnemo-bench-cf` crate (parked) **does not bench Project Think
  as a runtime** — that's a layering question, not a perf question.
  The bench targets remain: hosted Agent Memory KV+Vectorize and DO
  Facets SQLite as memory-store substrates.
- A future `deploy/cloudflare/` scaffold (parked v0.4.4 backlog) will
  ship the stacking pattern above as a reference implementation: a
  Worker + DO Facet running the durable loop, with a `wrangler.toml`
  binding to an operator-held mnemo audit endpoint.
- The `cloudflare-workers-deploy.md` design note's "Runtime layer
  (Project Think)" sub-section links here for the layering rationale.

Project Think and the [MCP 2026 Roadmap](https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/)
together describe the *runtime + protocol* picture; mnemo sits below
both as the offline-auditable storage substrate. See the four-priority
mapping in [`../src/integrations/mcp-server.md`](../src/integrations/mcp-server.md)
§"MCP 2026 Roadmap alignment".

## Sources

- Cloudflare Project Think announcement — https://blog.cloudflare.com/project-think/ (2026-05-04)
- Cloudflare Durable Object Facets open beta — https://blog.cloudflare.com/durable-object-facets-dynamic-workers/ (2026-04-30, the storage substrate Project Think builds on)
- Cloudflare Agents Week wrap — https://www.cloudflare.com/agents-week/updates/ (2026-04-29, broader runtime context)
- mnemo memory-store comparison: [`cloudflare-agent-memory.md`](cloudflare-agent-memory.md)
- mnemo deploy-template design: [`../src/integrations/cloudflare-workers-deploy.md`](../src/integrations/cloudflare-workers-deploy.md)
