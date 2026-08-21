# Known security limitations

Measured claims live in [`docs/benchmarks/index.md`](../benchmarks/index.md).
This page is the other half: things a reader might reasonably assume mnemo
measures or enforces, and does not.

It exists because an unmeasured gap tracked only as an open issue reads as work
in progress, and after four months across five releases that stops being true. A
limitation someone can find and plan around is worth more than an issue that has
outlived its own milestone.

## The MINJA procedure is not measured

**Status: designed, not built. Tracked by
[#37](https://github.com/sattyamjjain/mnemo/issues/37), targeted before 1.0.**

mnemo publishes four memory-poisoning benchmarks. **None of them is MINJA**
([arXiv:2503.03704](https://arxiv.org/abs/2503.03704)), and the gap is specific
rather than incidental.

| Bench | What it measures | Why it is not MINJA |
|---|---|---|
| `bench/poisoning` | defense-delta ASR, quarantine on vs off | static MINJA-*style* lexical attacks, not the interactive procedure |
| `bench/salami_poisoning` | compositional save and assembly rates | individually-benign slices; a different attack shape |
| `bench/asi06_poisoning` | cover-up and forgery resistance of the audit layer | detects tampering; does not model an attacker corrupting memory |
| `bench/forged_reasoning` | forged chain-of-thought injection ASR | trust-filter evaluation, single-shot |

MINJA is a **procedure, not a corpus**: the attacker only ever issues *ordinary
queries* to the victim agent and corrupts its memory through that legitimate
channel. Nothing above models an interactive, multi-turn, query-only attacker.

### What a user should assume

- **mnemo has no measured resistance to MINJA-style progressive memory
  poisoning.** Do not read the four benches above as covering it. They measure
  different attacks and they say so individually.
- The nearest applicable finding is uncomfortable and is published rather than
  buried: on a real dense embedder, poisoned content sits at roughly **1.5 sigma**
  against a **3.0 sigma** default threshold, so **the z-score outlier lane does not
  catch it** (see [`docs/BENCH_POISONING.md`](../BENCH_POISONING.md)). The raw
  result shows it directly: on the evasive variant the z-score lane leaves ASR at
  **1.0 with the defense on**, unchanged from off
  ([`bench/results/poisoning_real.json`](../../bench/results/poisoning_real.json)).
  The lexical lane does defend against the canonical static variant, **100% to
  0%**, with 0/300 benign false positives.
- If MINJA-shaped attacks are in your threat model, the controls that do apply
  are the write-provenance chain and read-time trust filtering, both of which are
  measured, and neither of which is a poisoning *detector*.

### Why it has not been built

The blocker is a budget, not engineering, and saying so plainly is the point.
MINJA Phase 2 (progressive shortening) is **generative and adaptive**: a faithful
harness needs a model in the loop *as the attacker*. There has been no LLM budget
for that since the issue was filed on 2026-04-24.

An issue whose blocker is funding, filed as though its blocker were engineering,
does not move on its own, and #37 did not move for four months.

### The design is already fixed

[ADR 0003](../adr/0003-minja-procedure-harness.md) is the design of record and
pre-registers the honesty properties **before** any numbers exist: defense-delta
rather than a bare ASR, a mandatory topic-matched benign control arm, Wilson-95
intervals, and a structural success oracle so the headline reproduces without an
LLM judge. It also records the prediction that the z-score lane will probably
catch nothing, so a negative result cannot later be reframed into whatever the
detector does catch.

A reduced, non-adaptive Phase-3-only measurement is scoped and needs no budget,
only time. That is what #37 now tracks. It will **not** be a MINJA number and
must never be labelled one: removing the adaptive shortening step is precisely
what ADR 0003 says makes a result stop being MINJA.
