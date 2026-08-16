# Governed Persistent Memory (arXiv 2608.12476) — prior art on the release decision

> **Prior-art citation, not a parity claim.** Recorded 2026-08-16. GPM is the
> academic statement of a problem mnemo has only partly solved. mnemo ships two
> of the paper's five clauses in recognisable form (ledger integrity, and a
> weaker source binding), does **not** implement its bitemporal derived-lifecycle
> -state model, and has **no release gate at all** — mnemo's `RECALL` returns
> records and the agent does what it likes with them. Read the
> [gap table](#where-mnemo-actually-lands) before citing this page anywhere.

## Citation

- **Paper:** Guodong Xu — *"Governed Persistent Memory: Source-Bound State
  Semantics and Fail-Closed Release for Long-Horizon Agents"*
- **arXiv:** [2608.12476](https://arxiv.org/abs/2608.12476)
- **Submitted:** 2026-08-12
- **Surfaced:** 2026-08-16

## What the paper argues

Agent memory is conventionally modelled as **select–store–retrieve**. The paper's
central claim is that this framing is missing a step, and that the missing step
is where the failures live:

> retrieval does not decide whether contradictory, superseded, retracted,
> deleted, or stale records may support an outgoing claim

Retrieval answers *what is relevant*. It never answers *what is allowed to leave
the system*. Those are different questions, and a store that only answers the
first will happily hand an agent a retracted record that ranks well.

GPM's proposal is an auditable **bitemporal state-transition model** with:

- **source-bound admission** — a record's admission is tied to the source that
  produced it, rather than accepted as undifferentiated content
- **derived lifecycle state** — releasability is *computed* from transitions, not
  stored as a flag someone must remember to set
- **current public barriers** — an explicit boundary between held state and
  releasable state
- **fail-closed structured release** — the release step defaults to refusing

The model is expressed as **five executable clauses**: ledger integrity, source
binding, conflict isolation, non-revival after retraction or deletion, and exact
claim closure over a fresh view at one verified head.

## The numbers, with the paper's own caveats attached

On a prespecified, **hash-frozen 3,600-case GPM-ReleaseBench**:

| lane | result |
|---|---|
| GPM | matches all complete outcomes |
| strongest of three intentionally simple complete policies | **1,800 / 3,600**, and makes unmatched releases on **50% of violation cases** |

In a separate sealed end-to-end service evaluation (publicly disclosed V3 arm):

| lane | result |
|---|---|
| governed | **2,400 / 2,400** clusters correct |
| ungoverned local Qwen2.5-7B | **600 / 2,400** |

The governed lane repairs all 1,800 baseline failures with no regression
(one-sided 95% lower bounds 99.875% and 99.834%). A later V5 reseal over Chinese-
and English-command arms reproduces 2,400/2,400 per arm.

**The caveats are the paper's own, and dropping them would misreport it.** The
paper states that these are *"bounded contract and implementation results, not
open-world model accuracy or evidence of world truth"*, that governed answers in
the sealed evaluation are deterministic service outputs, and that the 7B figure
is the ungoverned comparison — **not** a claim that a language model became
accurate. The 2,400/2,400 is a contract-conformance number. It is not a recall
score and it does not belong in a leaderboard row.

Note also what the 1,800/3,600 result actually indicts. It is not a strawman: it
is the *strongest* of three **complete** policies — policies that always answer.
Being complete and simple is precisely what a normal memory store is. Half of the
violation cases got an unmatched release. That is the shape of the problem for
any store, including this one, that treats retrieval as the last checkpoint.

## Where mnemo actually lands

Clause by clause, honestly. `✅` = shipped and reproducible here; `➖` = a weaker
or partial form, with the weakness named; `❌` = not implemented.

| GPM clause | mnemo | what is actually there, and what is missing |
|---|---|---|
| **Ledger integrity** | ✅ | SHA-256 hash-chained `agent_events` with an offline verifier that needs no trust in the store. Measured: **100% single-byte-mutation detection over 256 trials** ([`bench/audit_conformance`](../../bench/audit_conformance)), and delete / reorder / forge-integrity-field each 100% over 200 trials ([`bench/audit_tamper`](../../bench/audit_tamper)), with two **disclosed 0% gaps** — payload-only forge and tail truncation. This is the one clause where mnemo is on comparable ground. |
| **Source binding** | ➖ | [`WriteProvenance`](../../crates/mnemo-core/src/provenance.rs) binds every `REMEMBER` / `SHARE` to a principal, capability and session over a SHA-256 chain, and `SourceType` records an origin. **But the binding is not admission-controlling and not tamper-proof in the way the clause requires**: `source_type` is optional, defaults to `Agent`, is not covered by the content hash, and does not appear in the read receipt — so a tool return is **not provably separable from user input**. That limitation is already written down in [`docs/security/read-time-provenance.md`](../security/read-time-provenance.md); GPM is the paper that says why it matters. |
| **Conflict isolation** | ➖ | [`query/conflict.rs`](../../crates/mnemo-core/src/query/conflict.rs) resolves conflicts (including `EvidenceWeighted`) and [`current_fact_resolver`](../../crates/mnemo-core/src/query/current_fact_resolver.rs) drops superseded versions with an optional supersession chain. This is **resolution at read time, not isolation as a state barrier** — the loser is de-ranked, not made unreleasable. |
| **Non-revival after retraction or deletion** | ❌ | mnemo has soft delete, `forget_subject` redact / hard-delete, and TTL sweeps. It has **no non-revival guarantee**: point-in-time `as_of` recall deliberately skips the deleted check (`passes_filters` in [`query/recall.rs`](../../crates/mnemo-core/src/query/recall.rs) reads `if request.as_of.is_none() && record.is_deleted()`), so a deleted record is reachable again by design. That is a defensible audit feature and a direct violation of this clause. |
| **Exact claim closure over a fresh view at one verified head** | ❌ | There is no release step to close over. `RECALL` returns ranked records; nothing decides whether a record may support an outgoing claim, and nothing pins the read to a verified head. |

And the structural gap underneath all five:

**mnemo does not implement GPM's bitemporal derived-lifecycle-state model.**
There is bitemporal machinery in the tree — [`mnemo-graph`](../../crates/mnemo-graph)
carries Graphiti-style temporal edges, and core has `as_of` point-in-time
queries — but no state machine *derives* a per-record releasability from
transitions. mnemo's lifecycle fields (`ConsolidationState`, `deleted_at`,
maturity) are stored attributes that retrieval reads, not a computed barrier that
release must pass.

Most importantly: **mnemo's release posture is fail-open.** `RECALL` returning a
record is the end of mnemo's involvement. GPM's argument is that this is the
wrong place to stop, and mnemo does not currently have a counter-argument — only
a different scope.

## Why this is cited rather than adopted

mnemo's axis is *auditability of what was written and read* — a regulator or
auditor verifying the log offline, without trusting the store (see
[POSITIONING](../POSITIONING.md)). GPM's axis is *governance of what may be
asserted* — a gate between the store and the agent's outgoing claim. They compose
well and they are not the same thing: an unforgeable ledger of a bad release is
still a bad release, which is exactly the point the paper is making.

Adopting the model is a real design commitment, not a patch:

- a **release surface** distinct from `RECALL`, because retrofitting fail-closed
  semantics onto `RECALL` would break every shipped client of a docs-drift-tested
  MCP tool
- **derived** rather than stored lifecycle state, which cuts across
  `ConsolidationState`, `deleted_at`, supersession and TTL
- a **verified-head** notion for reads, which mnemo's `as_of` deliberately does
  not provide
- reconciling **non-revival** with mnemo's intentional `as_of` visibility of
  deleted records — these two requirements are in direct conflict and one of them
  has to give

None of that is scheduled. This page exists so that the gap is written down and
dated, in the same way [ADR 0001](../adr/0001-capability-leased-reads.md) recorded
an unbuilt design rather than letting a capability matrix imply it shipped.

## What this page is NOT

- **NOT a claim that mnemo implements GPM**, or any of its five clauses in full.
  Two are partial and named as partial; two are absent.
- **NOT a benchmark comparison.** mnemo has not been run against
  GPM-ReleaseBench, and no mnemo number on this page is comparable to a GPM
  number. The audit-chain figures measure tamper detection, not release
  correctness.
- **NOT a reproduction.** GPM-ReleaseBench and the sealed service evaluation were
  not re-run here; the numbers above are the paper's own, quoted with its own
  scoping caveats.
- **NOT a roadmap commitment.** See the section above for what adoption would
  cost. Nothing is scheduled.

## Cross-references

- Audit-chain evidence (the one comparable clause):
  [`bench/audit_conformance`](../../bench/audit_conformance),
  [`bench/audit_tamper`](../../bench/audit_tamper),
  [tamper-evidence narrative](../benchmarks/audit-log-tamper-evidence.md)
- Source-binding limitation, already disclosed:
  [`docs/security/read-time-provenance.md`](../security/read-time-provenance.md)
- Write-provenance chain: [`crates/mnemo-core/src/provenance.rs`](../../crates/mnemo-core/src/provenance.rs)
- Positioning axis this sits beside: [`docs/POSITIONING.md`](../POSITIONING.md)

## Sources

- arXiv 2608.12476 — https://arxiv.org/abs/2608.12476 — Guodong Xu, *"Governed
  Persistent Memory: Source-Bound State Semantics and Fail-Closed Release for
  Long-Horizon Agents"* (submitted 2026-08-12). Title, date, authorship and every
  figure quoted above were verified against the arXiv API on 2026-08-16.
