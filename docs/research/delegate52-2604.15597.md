# DELEGATE-52 (arXiv 2604.15597) — outcome-diffing primitive anchor

> Recorded 2026-05-10. **Composition anchor, not a compliance claim.**
> DELEGATE-52 is an external research artifact, not a spec and not a
> product. mnemo claims neither implementation of any DELEGATE-52
> mitigation nor any form of "DELEGATE-52-resistant" framing — the
> overclaim phrasings are blocked by `tests/readme_no_marketing_phrases.rs`.
> This note exists so a future contributor can find the layering
> rationale fast without re-reading the paper.

## Citation

- **Paper:** the DELEGATE-52 delegation-corruption result.
- **arXiv:** [2604.15597](https://arxiv.org/abs/2604.15597)
- **Front-page on Hacker News:** 2026-05-09
- **Released:** May 2026

(The literal paper title appears in the §Sources block at the bottom
of this doc only, per the citation phrasing rule the README's
marketing-phrase test enforces.)

## What DELEGATE-52 measures

DELEGATE-52 is an empirical study of long delegated workflows on
frontier LLMs. The setup: a primary agent delegates a multi-step
document-editing task to a sub-agent, the sub-agent works through it
across many tool calls and intermediate reflection steps, and the
final artifact is compared against the primary agent's original plan.
The headline finding — **a 25% baseline rate at which the final
artifact silently diverges from the plan, with no error surfaced to
either agent** — establishes the corruption rate this audit layer
needs to detect.

The paper's contribution is the *measurement*: a reproducible test
harness, a benchmark corpus across multiple frontier models, and a
taxonomy of the failure modes (instruction drift, intermediate-state
loss, tool-output mis-merge, reflection-step rewriting, summary-step
elision). The paper does not propose a new wire protocol or a new
substrate; it assumes the substrate already records enough trace data
to support the measurement.

## The three trust walls

DELEGATE-52 lands in a moment when the agent-trust conversation is
splitting into three separable layers:

1. **Wall 1 — aligned-by-training intent.** Did the model want to
   produce the right artifact at all? Owned by the model lab and
   the alignment-research community. Out of scope for mnemo.
2. **Wall 2 — policy-mediated action.** Did the agent's policy /
   guardrails / tool catalog let it take the actions it intended to
   take? Owned by the runtime layer (MCP authorization spec,
   role-aware tool filter, ARGUS-style read-side audit). mnemo
   participates as the substrate but does not own the policy
   evaluation.
3. **Wall 3 — outcome-diffing.** Did the artifact the sub-agent
   actually produced match the artifact the primary agent's plan
   asked for? This is where DELEGATE-52's 25% baseline lives, and
   this is where mnemo's append-only event log + snapshots are the
   right substrate.

## Where mnemo fits — the plan / input / trace / output tetrad

```text
   ┌──────────────────────────────────────────────────────────────┐
   │                   Outcome-diff reconstruction                │
   │                                                              │
   │   Plan       ──REMEMBER──▶  ┌─────────────┐                  │
   │   (primary    consolidation │             │                  │
   │   agent's    importance=    │  Append-    │                  │
   │   stated     "plan", scope= │  only       │  ──RECALL──▶     │
   │   intent)    "thread")      │  event      │   (auditor /     │
   │                             │  log        │    downstream    │
   │   Input      ──REMEMBER──▶  │             │    consumer      │
   │   (sub-agent  source_type=  │  + snapshot │    reconstructs  │
   │   inputs,    "tool_output"  │  table      │    full chain    │
   │   tool       or "agent")    │             │    at any        │
   │   responses)                │  HMAC-      │    `as_of`)      │
   │                             │  chained    │                  │
   │   Trace      ──Event──▶     │  envelopes  │                  │
   │   (every     each tool      │             │                  │
   │   reflection call, each     │             │                  │
   │   step,      reflection,    │             │                  │
   │   each       each branch    │             │                  │
   │   tool call) merge)         │             │                  │
   │                             │             │                  │
   │   Output     ──REMEMBER──▶  │             │                  │
   │   (final     content_hash + │             │                  │
   │   artifact)  prev_hash      └─────────────┘                  │
   │                                                              │
   └──────────────────────────────────────────────────────────────┘
```

Concretely, the four substrate operations DELEGATE-52-style outcome
diffing reaches for:

| Audit need | mnemo surface |
|---|---|
| Capture the primary agent's plan as a first-class memory | REMEMBER with `importance="plan"` (or whatever convention the operator picks) + `consolidation_state="raw"` so the plan record cannot be silently consolidated away |
| Capture every input the sub-agent saw | REMEMBER with `source_type="tool_output"` per tool response; envelope chain links to the agent that issued the tool call |
| Capture every reflection / branch / merge step | First-class `Event` rows in the append-only log; `prev_event_id` chains preserve causal order; PostgreSQL `prevent_event_modification` trigger enforces append-only at the schema level |
| Capture the final artifact and chain it back | REMEMBER with a `content_hash` whose `prev_hash` walks back through the trace to the plan |
| Reconstruct the full chain post-hoc | RECALL with `with_provenance=true` returns HMAC-SHA256 receipts naming every cited record; `mnemo verify` re-validates the chain offline |
| Diff actual vs planned outcome | Out of scope for mnemo — this is the *audit policy* a downstream consumer (e.g. a DELEGATE-52 reference replay tool) chooses; mnemo provides the substrate the audit reads from |

## Layering — what mnemo does NOT do

mnemo does not implement the DELEGATE-52 audit. mnemo's job is to
make the substrate auditable; the diffing policy (what counts as a
"silent corruption", how aggressively to flag, what threshold to
escalate at) is a separate layer. The pairing is the same shape as
the [ARGUS composition anchor](argus-2605.03378.md): mnemo is the
*write-side* substrate, an external audit tool is the *read-side*
diffing model, neither replaces the other.

## Operator recipe — getting outcome-diff-ready today

An operator wanting to start collecting the substrate for
DELEGATE-52-style outcome diffing can do so against mnemo today
without any new code:

1. **Mark the plan.** Issue a REMEMBER for the primary agent's plan
   with explicit metadata: `source_type="agent"`, `importance` set
   high enough that the default decay-curve does not consolidate it,
   and a stable `thread_id` for the delegation.
2. **Mark every tool-output input.** Issue a REMEMBER per tool
   response with `source_type="tool_output"`. Use the envelope
   chain to bind the tool response to the agent that issued the
   call.
3. **Let the event log do its job.** No special action — every
   `recall`, `forget`, `share`, `branch`, `merge` is already an
   append-only event with `prev_event_id` chaining.
4. **Mark the artifact.** Issue a REMEMBER for the final artifact
   with the same `thread_id`.
5. **Replay later.** A downstream consumer issues `recall(...,
   thread_id=<id>, as_of=<delegation_end_ts>)` to retrieve the full
   trace, runs `mnemo verify` on the chain, and feeds the
   plan/input/trace/output tetrad into whatever DELEGATE-52-style
   diffing tool the operator chooses.

The operator does NOT need a `mnemo-delegate52` adapter crate; the
substrate is already complete. If a future v0.5.x ships a reference
replay tool that bundles the diffing policy + the substrate-pull,
that's a separate row.

## What this note is NOT

- **Not a DELEGATE-52 implementation.** mnemo records the substrate;
  the diffing audit is a downstream layer.
- **Not a compliance claim.** Compositional audit guarantees
  ("DELEGATE-52-resistant", "outcome-corruption-proof",
  "delegation-safe by construction") are blocked by the README
  marketing-phrase test. mnemo's job is the substrate honestly
  recorded; the audit's job is the policy applied to it.
- **Not a benchmark.** No bench harness is implied. If a future
  v0.5.x ships a DELEGATE-52-replay reference test corpus, that's
  a separate row.
- **Not an integration.** The DELEGATE-52 paper does not ship an API.
  There is nothing to integrate against today.

## Cross-references

- ARGUS read-side composition: [`argus-2605.03378.md`](argus-2605.03378.md) — same layering shape (mnemo = substrate, external audit = policy).
- Dreams curator-side composition: [`../comparisons/anthropic-dreams.md`](../comparisons/anthropic-dreams.md) — what to keep / forget / consolidate is the curator's job; what the substrate honestly recorded for the auditor is mnemo's job. Today's update adds a one-line cross-reference there back to this doc.
- README "Why mnemo when Cloudflare Agent Memory exists?" — gains a paragraph anchoring the outcome-diffing primitive in v0.4.4.
- v0.4.4 carry list: [`../../CHANGELOG.md`](../../CHANGELOG.md) `[Unreleased]` block — no `mnemo-delegate52` entry; outcome-diff replay tooling is not on the v0.4.4 backlog.

## Sources

- arXiv 2604.15597 — https://arxiv.org/abs/2604.15597 — *"LLMs Corrupt Your Documents When You Delegate"* (literal title, May 2026).
- Hacker News front-page surface: 2026-05-09.
- Companion read-side composition: arXiv 2605.03378 (ARGUS, 2026-05-05).
