# Example RECALL fixtures — outcome-diff substrate (v0.4.4)

> **Purpose:** documentation-only fixtures that exercise the
> reconstruction-from-events path described in
> [`../research/delegate52-2604.15597.md`](../research/delegate52-2604.15597.md).
> These rows specify the substrate calls an outcome-diff replay tool
> would issue against mnemo for a single delegated workflow — they
> are NOT bench numbers and they do NOT add new behaviour to the
> binary. The purpose is operator-facing: given a delegation
> `thread_id`, what does the substrate-pull look like end-to-end?
>
> Each row below is a fixture in the same shape the
> `crates/mnemo-cli/tests/release_status_doc_present.rs` family
> enforces: a stable string-grep test pins the doc against
> accidental deletion or rewrite (see
> `tests/example_recalls_doc_present.rs`).

## Fixture row 1 — primary-agent plan capture

**Scenario.** A primary agent intends to delegate "rewrite section 3
of the design doc to remove the deprecated `mnemo-cma` reference."
The plan is captured as a first-class memory before any sub-agent
execution begins.

**Substrate call.**

```text
REMEMBER {
  thread_id     = "delegation-2026-05-10-abc123",
  agent_id      = "primary-claude",
  source_type   = "agent",
  importance    = 0.95,                 // explicit, do not let
                                        // decay-curve consolidate
  scope         = "private",
  consolidation_state = "raw",
  metadata      = { "role": "plan",
                    "delegate_target": "sub-agent-rewriter",
                    "plan_version": "v1" },
  content       = "rewrite §3 of the design doc; preserve the
                   bench-harness pointer; remove the mnemo-cma
                   line; do not touch §4 or later"
}
```

**Why each field matters.**

- `thread_id` is the join key. Every input / trace event / output
  in this delegation carries the same `thread_id` so a later RECALL
  scoped to it pulls the entire tetrad in one query.
- `importance=0.95` + `consolidation_state="raw"` together prevent
  the default decay-curve and the consolidation pass from quietly
  eating the plan record before the audit replay needs it.
- `metadata.role="plan"` is the literal label the diffing tool
  filters on. The convention is operator-chosen — mnemo does not
  enforce a particular role taxonomy.

**What the substrate guarantees.** This REMEMBER is now bound into
the per-agent envelope chain via HMAC-SHA256; an offline `mnemo
verify` against the chain will fail if the plan record is mutated,
deleted, or has its `prev_hash` altered after the fact.

## Fixture row 2 — full-tetrad reconstruction RECALL

**Scenario.** Twenty-four hours after the delegation completed, an
auditor (or an outcome-diff replay tool) reconstructs the full
delegation history.

**Substrate call.**

```text
RECALL {
  thread_id          = "delegation-2026-05-10-abc123",
  agent_id           = null,                   // pull across both
                                               // primary and sub-agent
  as_of              = "2026-05-10T18:00:00Z", // the moment the
                                               // delegation completed
  with_provenance    = true,                   // HMAC-SHA256 receipts
                                               // for offline verify
  hybrid_weights     = [1.0, 0.0, 0.0, 0.0],   // pure recency over
                                               // the thread
  limit              = 1024                    // pull the whole chain
}
```

**Expected response shape.**

The response is a list of records partitioned by `metadata.role`:
exactly one record with `role="plan"` (the row 1 fixture), N records
with `source_type="tool_output"` (every tool response the sub-agent
saw), every event-log row with `event_type ∈ {recall, branch,
merge, reflection}` for the thread, and one or more records with
`metadata.role="output"` (the final artifact). Plus a single
`provenance_receipt` covering the HMAC-SHA256 hash of every cited
record's `content_hash`.

**What an outcome-diff tool does next.** Reads the plan record's
content, walks the trace events in causal order
(`prev_event_id` chain), reconstructs the artifact-as-built from the
`role="output"` records, and diffs against the plan. mnemo provides
the substrate; the diff policy is the auditor's.

**What mnemo does NOT do.** mnemo does not run the diff. mnemo does
not flag silent corruption. mnemo does not guarantee any class of
DELEGATE-52 outcome by construction — those overclaim phrasings
("DELEGATE-52-resistant", "outcome-corruption-proof", "delegation-safe
by construction") are blocked by the README marketing-phrase test.
mnemo's job is the substrate honestly recorded; the audit's job is
the policy applied to it.

## Cross-references

- [`../research/delegate52-2604.15597.md`](../research/delegate52-2604.15597.md) — full operator recipe + the plan / input / trace / output tetrad diagram + the explicit non-overlap callout.
- [`../comparisons/anthropic-dreams.md`](../comparisons/anthropic-dreams.md) — Curation (Dreams) and outcome diffing (DELEGATE-52) are separable substrate questions; this doc covers the latter.
- [`../comparisons/cloudflare-agent-memory.md`](../comparisons/cloudflare-agent-memory.md) — The S3 (chain audit replay) row in the Cloudflare comparison is the same substrate axis these fixtures exercise.
