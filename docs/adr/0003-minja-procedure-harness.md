# ADR 0003 — MINJA procedure harness design

- Status: **Proposed, not built** (this is the design doc [#37](https://github.com/sattyamjjain/mnemo/issues/37) requires before any harness code)
- Date: 2026-08-15
- Tracking: [#37](https://github.com/sattyamjjain/mnemo/issues/37) (labelled `needs-design`)
- Source: MINJA, [arXiv:2503.03704](https://arxiv.org/abs/2503.03704)

## Why this is a design doc and not a harness

#37 says so itself: *"Out of scope today: this is >500 LoC of attacker
logic + an LLM budget to drive the attacker agent. Not a drop-in; needs
its own design doc first."* This is that doc. It is written so the
implementation PR is mechanical and its honesty properties are fixed in
advance rather than negotiated while the numbers are being generated.

## What already exists, and what it is not

mnemo has four poisoning benches. None of them is MINJA, and the gap is
specific:

| Bench | What it measures | Why it is not MINJA |
|---|---|---|
| `bench/poisoning` | defense-delta ASR, quarantine ON vs OFF | Static MINJA-*style* lexical attacks, not the interactive procedure |
| `bench/salami_poisoning` | compositional save + assembly rate | Individually-benign slices; a different attack shape entirely |
| `bench/asi06_poisoning` | cover-up/forgery resistance of the audit layer | Detects tampering; does not model an attacker corrupting memory |
| `bench/forged_reasoning` | forged-CoT injection ASR | Trust-filter evaluation, single-shot |

MINJA is a **procedure**, not a corpus: the attacker only ever issues
*ordinary queries* to the victim agent, and corrupts its memory through
that legitimate channel. Nothing in the four benches models an
interactive, multi-turn, query-only attacker. That is the whole gap.

## The three phases to implement

Faithful to the paper, and the reason the LoC estimate is what it is.

### Phase 1 — indication-prompt injection

Attacker submits queries carrying a bridging instruction that associates
a *victim query* with a *target (malicious) answer*. The agent stores the
interaction as an ordinary memory. No privileged access; no direct write.

### Phase 2 — progressive shortening

The attacker re-submits, iteratively deleting the explicit bridging text
while keeping the association intact. This is the step that makes the
resulting memory look benign: by the end, the stored record contains no
visible injection, only an innocuous-looking Q/A pair whose *retrieval
behaviour* is poisoned. **Any detector evaluated only against Phase-1
text will overstate its own performance** — this is the crux of why the
existing lexical bench is not a MINJA number.

### Phase 3 — victim-query exploitation

A *different* user (or session) issues the target query. Success = the
poisoned record is retrieved and the malicious answer is produced.

## Measurement protocol (fixed now, not later)

- **Primary metric:** attack success rate over N target queries, detector
  **disabled vs enabled** — a defense-delta, matching the shape
  `bench/poisoning` already publishes. A bare ASR without the OFF arm is
  not publishable.
- **Threshold sweep:** `PoisoningPolicy` threshold ∈ {2.0, 2.5, 3.0, 3.5,
  4.0}, baseline trained on pre-attack memory only. Publish the ROC.
- **Benign control arm, mandatory.** A topic-matched benign workload run
  through the identical pipeline, reporting false-quarantine rate. Every
  other bench in this repo carries one; a defense number without it is a
  half-number.
- **Wilson 95% intervals** on every rate, per house convention.
- **Success oracle:** structural (did the target answer surface in the
  top-k / in the response), not LLM-judged, so the headline is
  reproducible without a judge. If an LLM judge is added later it is a
  *second* reported number, never a replacement.

## The two hard costs, stated plainly

### 1. It needs a real LLM budget

Phases 1–2 require a model in the loop *as the attacker* — the shortening
step is generative and adaptive. This cannot be faked with fixtures
without ceasing to be MINJA. Budget must be explicit before the PR opens,
and the harness must record the exact model id in the result file (as
`bench/locomo` already does for judges).

### 2. The z-score lane is likely to catch nothing

This is the uncomfortable prediction, and recording it in advance is the
point of a design doc. mnemo's own prior finding — a real dense embedder
puts poisoned content at roughly 1.5σ, under the 3.0σ default — means the
outlier detector probably does **not** fire on a Phase-2-shortened record,
which is engineered to look ordinary. The lexical lane is the one with a
plausible chance.

If that prediction holds, the honest headline is *"the z-score lane does
not defend against MINJA; here is the measured ASR with it enabled"* —
not a quiet reframing to whatever the detector does catch. Writing the
expected-negative down now removes the temptation to redefine success
after seeing the data.

## Implementation sketch

```
bench/minja_procedure/
  src/
    victim.rs      # agent over MnemoEngine; ordinary remember/recall only
    attacker.rs    # phases 1-3; the LLM-driven loop
    oracle.rs      # structural success test + benign control
    sweep.rs       # threshold sweep -> ROC
  results/         # dated JSON + MD, per house convention
```

Victim wiring goes through the ordinary `remember`/`recall` path — no
privileged write, no test-only backdoor. If the harness needs a special
API to poison memory, it is not modelling MINJA.

## Acceptance criteria

1. ASR with detector OFF and ON, N ≥ 100 target queries, Wilson-95.
2. Benign control false-quarantine rate on a topic-matched workload.
3. ROC across the five thresholds.
4. Dated result file naming the attacker model, embedder, and machine.
5. `docs/benchmarks/index.md` row + a "what this does not show" section.
6. Published at `docs/benchmarks/2026-*-minja-procedure-harness.md`.

## What this design does not cover

Semantic-paraphrase, adaptive-attacker, and cross-session-drip variants
remain open under #37 after this lands. This ADR covers the MINJA
procedure as published, nothing wider.
