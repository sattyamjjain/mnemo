# mnemo vs Anthropic Dreams — substrate vs curator

> Living doc, last updated 2026-05-09. Companion to
> [`cloudflare-agent-memory.md`](cloudflare-agent-memory.md) (memory store comparison)
> and [`cloudflare-project-think.md`](cloudflare-project-think.md) (runtime layer
> comparison). Where those docs ask "where does memory live?" and "where does
> the agent loop run?", this doc asks the upstream question: "who decides what
> to keep, what to forget, and what to consolidate — and where does the
> chosen state durably land?"

## Status — Research Preview

[Anthropic Dreams](https://platform.claude.com/docs/en/managed-agents/dreams)
is a **Research Preview** feature behind a Request-access form (verified
2026-05-09). The companion [Routines](https://code.claude.com/docs/en/routines)
doc describes the long-horizon agents that consume curated memory. The
public docs page is the only authoritative interface today.

**This doc is a substrate-anchor, not an integration claim.** mnemo does
not today ship an Anthropic-API adapter. A `mnemo-dreams` adapter crate
is plausible if/when the API exits Research Preview, but is explicitly
NOT in scope for v0.4.x. Today's surface is documentation that records
the layering rationale so an operator evaluating both can see they are
complementary, not substitute.

## What Dreams does

Per the Anthropic docs page tagline: "Let Claude reflect on past
sessions to curate an agent's memory and surface new insights." The
Research Preview describes Dreams as a *curator* — a managed feature
that reflects over an existing memory store and produces curated
items: consolidated, forgotten, re-ranked, or annotated. The page does
not define a new wire protocol; it describes a curator that reads and
writes memory items through whatever substrate the agent already uses.

The companion Routines doc describes the consumers: long-horizon agents
whose continued operation benefits from curated memory across many
sessions.

## What mnemo does

mnemo is the substrate layer beneath such a curator. Its surfaces:

- **REMEMBER / RECALL / FORGET / SHARE** — the four primitives a curator
  reads and writes through. REMEMBER carries explicit metadata
  (importance, decay function, source, scope); RECALL carries hybrid
  scoring + provenance receipts; FORGET carries five strategies
  (`SoftDelete` / `HardDelete` / `Decay` / `Consolidation` / `Archive`)
  matching curator-action shapes; SHARE carries cross-agent ACL +
  delegation chains.
- **HMAC envelope chain** — every read returns an HMAC-SHA256
  provenance receipt verifiable offline; every write extends a
  per-agent chain whose `verify` tool reconstructs the full lineage.
- **AES-256-GCM at-rest content encryption** via `MNEMO_ENCRYPTION_KEY`,
  with operator-held keystore.
- **Bitemporal graph layer** (`mnemo-graph`) — `valid_from` /
  `valid_to` (fact validity) plus `recorded_at` (system clock).
  `graph_expand(seed, depth, as_of)` walks the graph at any point in
  time, which is exactly the shape a curator's "what did the agent
  know on day N?" reflection needs.
- **Decay-curve score lane** + cognitive-forgetting strategies — the
  primitives a curator's forget-policy chooses among, rather than
  hard-coding any one schedule.

## Layering — curator action ↔ mnemo primitive

| Dreams (curator action) | mnemo (substrate primitive) | Notes |
|---|---|---|
| Reflect over past N sessions | RECALL with `as_of` + thread filters | Bitemporal graph + point-in-time recall is what makes this efficient at depth. |
| Promote / consolidate insights | REMEMBER with `consolidation_state = "consolidated"` + decay-function override | Consolidation is a first-class state in mnemo's record model, not a side-effect of write order. |
| Demote / forget noise | FORGET with strategy `Decay` or `SoftDelete` (chain preserved) or `HardDelete` (PII redaction with audit-trail) | Five forget strategies cover the curator-policy space without bias. |
| Re-rank by re-evaluation | RECALL with `hybrid_weights` + custom `rrf_k` | Curator's re-rank policy maps to a tuned RRF across vector + BM25 + decay + recency. |
| Surface new insights via cross-session linkage | mnemo-graph relations + `graph_expand` traversal | Cross-session linkage lives in the typed-edge layer, not buried in unstructured text. |
| Audit what was curated and when | Envelope chain + `mnemo verify` | Every curator action chains into the HMAC ledger; an auditor can reconstruct the full curation history months later, offline. |

## Explicit non-overlap

Dreams owns the *what to curate* policy: which sessions to reflect
over, which insights to surface, what consolidation rule to apply,
how often to run. mnemo owns the *how to durably store with audit
trail*: HMAC chain integrity, AES-256-GCM at-rest encryption,
bitemporal graph, point-in-time recall, offline provenance receipts,
DPDPA / GDPR subject erasure with audit preservation.

If Dreams chooses to consolidate three memories into one, mnemo records
that consolidation as a chained event with the curator identity in
the envelope, so a future auditor can reconstruct the decision.
mnemo's job is not to decide *whether* the consolidation was correct;
that's the curator's job. mnemo's job is to make the audit trail
verifiable offline.

## Future — `mnemo-dreams` adapter

If/when the Dreams API exits Research Preview and ships a public
read/write surface, a `mnemo-dreams` adapter crate becomes plausible.
The adapter would expose mnemo's RECALL / REMEMBER / FORGET to a
Dreams curator the way `mnemo-cma` (v0.4.1) bridges the Anthropic
CMA-Memory beta filesystem into the mnemo HMAC chain. **This crate is
explicitly NOT on the v0.4.4 backlog today** — the API surface to bind
against is not yet public.

Operators wanting to experiment ahead of GA can pre-stage by:

1. Running mnemo with the existing four primitives + envelope chain.
2. Manually issuing curator-style operations (consolidations, decay
   tweaks, re-ranks) through the existing CLI / SDK while Dreams is
   in Research Preview.
3. When the API ships, a thin adapter crate translates Dreams calls
   into the operations they were already manually issuing.

## Outcome-diffing primitive (cross-reference)

Curation chooses *what* to keep, forget, or consolidate; outcome diffing
asks *whether* the artifact the agent finally produced matches the
agent's stated plan. The latter is its own substrate question — see
[`../research/delegate52-2604.15597.md`](../research/delegate52-2604.15597.md)
for the plan / input / trace / output tetrad mnemo's append-only log
captures, and the operator recipe for getting outcome-diff-ready today.

## Sources

- Anthropic Dreams Research Preview docs — https://platform.claude.com/docs/en/managed-agents/dreams (surfaced at Code w/ Claude SF, 2026-05-06; verified live 2026-05-09)
- Anthropic Routines doc — https://code.claude.com/docs/en/routines (companion long-horizon agent context, 2026-05-06)
- mnemo memory-store comparison: [`cloudflare-agent-memory.md`](cloudflare-agent-memory.md)
- mnemo runtime-layer comparison: [`cloudflare-project-think.md`](cloudflare-project-think.md)
- mnemo outcome-diffing primitive: [`../research/delegate52-2604.15597.md`](../research/delegate52-2604.15597.md)
- v0.4.4 carry list: [`../../CHANGELOG.md`](../../CHANGELOG.md) `[Unreleased]` § *Parked for v0.4.4 backlog* (no `mnemo-dreams` entry — explicitly out of scope)
