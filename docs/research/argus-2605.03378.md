# ARGUS 2605.03378 — research-anchor note

> Recorded 2026-05-09. **Composition anchor, not a compliance claim.**
> ARGUS is an external research artifact, not a spec and not a
> product. mnemo claims neither implementation of ARGUS nor any
> form of "ARGUS-compliance" — those phrasings are explicitly
> banned by `tests/readme_no_marketing_phrases.rs`. This note exists
> so a future contributor can find the layering rationale fast
> without re-reading the paper.

## Citation

- **Paper:** "Provenance-Aware Decision Auditing for Context-Aware
  Prompt Injection" (full title in this paragraph only).
- **arXiv:** [2605.03378](https://arxiv.org/abs/2605.03378)
- **Submitted:** 2026-05-05 (4 days old at the time of this anchor)

## What ARGUS does

ARGUS is a *read-side* decision-auditing model. Given:

1. A recorded LLM decision (the model's output for some prompt /
   tool-call / response).
2. The provenance of every context item that fed the prompt
   (where the item came from, who wrote it, when it was retrieved).

ARGUS reconstructs the influence graph from context items to the
decision and flags decisions where the influence shape is consistent
with **context-aware prompt-injection adversaries**: an attacker who
knew the agent's retrieval policy and seeded the substrate with items
designed to be retrieved at the right moment to nudge a specific
decision.

The paper's contribution is the audit *model* — the formal influence
shape of context-aware injection — and a methodology for applying it
post-hoc to recorded decisions. It does not propose a new wire
protocol or a new substrate; it assumes the substrate already records
provenance for every context item.

## Where mnemo fits

mnemo is the substrate the audit reads from. The pairing is natural
because mnemo's existing surfaces already produce the inputs ARGUS-style
auditing needs:

- **Per-record provenance.** Every memory record carries
  `source_type` + `created_by` + `consolidation_state`, and every
  RECALL returns an HMAC-SHA256 provenance receipt naming the cited
  records.
- **Bitemporal context replay.** `as_of` point-in-time queries +
  bitemporal graph traversal reconstruct the substrate state at the
  exact moment of the audited decision.
- **Quarantine state.** `quarantined` + `quarantine_reason` separate
  flagged-but-retained context from healthy context — useful when
  ARGUS is replaying a decision and needs to know which retrieved
  items were already suspect at retrieval time.
- **Append-only audit log.** PostgreSQL trigger
  (`prevent_event_modification`) enforces the substrate's immutability
  at the schema level, which is what makes the audit trustworthy
  months later.
- **Offline verification.** `mnemo verify` reconstructs the envelope
  chain without contacting any cloud account — the substrate is
  audit-trustworthy even when the original deployment is gone.

## What this note is NOT

- **Not an ARGUS implementation.** mnemo does not implement the
  ARGUS audit model. The read-side auditing logic lives elsewhere
  (a future ARGUS reference implementation, an internal SOC tool,
  or whatever the operator chooses).
- **Not a compliance claim.** Compositional security claims
  ("prompt-injection-proof", "provenance-guaranteed",
  "injection-resistant by construction") are explicitly banned by
  the README marketing-phrase test extension that landed alongside
  this note (2026-05-09 U2). mnemo's job is the substrate; ARGUS's
  job is the audit; together they cover both sides — neither
  guarantees end-to-end safety alone.
- **Not a benchmark.** No bench harness is implied. If a future
  v0.5.x ships an ARGUS-replay reference test corpus, that's a
  separate row.

## Cross-references

- Substrate-side comparison: [`../comparisons/cloudflare-agent-memory.md`](../comparisons/cloudflare-agent-memory.md)
  § *Read-side composition: ARGUS provenance auditing (2026-05-09)*
- Memory-curation substrate anchor: [`../comparisons/anthropic-dreams.md`](../comparisons/anthropic-dreams.md)
  (companion 2026-05-09 row — both anchor mnemo as substrate
  layer beneath external read/write models)
- v0.4.4 carry list: [`../../CHANGELOG.md`](../../CHANGELOG.md)
  `[Unreleased]` block (no `mnemo-argus` entry — ARGUS-aware audit
  tooling is not on the v0.4.4 backlog)

## Sources

- arXiv 2605.03378 — https://arxiv.org/abs/2605.03378 (submitted 2026-05-05)
