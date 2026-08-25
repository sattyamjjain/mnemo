# Non-adaptive Phase-3 exploitation, against a pinned real embedder

**2026-08-25** · tracks [#37](https://github.com/sattyamjjain/mnemo/issues/37) ·
design of record [ADR 0003](../adr/0003-minja-procedure-harness.md) ·
raw artifact [`bench/results/minja_phase3.json`](../../bench/results/minja_phase3.json)

> **This is not a MINJA number and must not be labelled one.** ADR 0003 states
> that removing the adaptive shortening step is exactly what makes a result stop
> being MINJA. Phases 1 and 2 need a model in the loop *as the attacker* and are
> held out of scope pending an LLM budget. What is measured here is the
> **retrieval-time consequence of records that are already shortened** — a
> strictly weaker claim.

## The result

Three findings, all of them nulls, all with denominators.

| | rate | 95% CI | n |
|---|---:|---|---:|
| Poisoned record exploited, detector **OFF** | **0.956** | [0.906, 0.980] | 129/135 |
| Poisoned record exploited, detector **ON** | **0.956** | [0.906, 0.980] | 129/135 |
| **Benign floor** (matched twin), detector OFF | **0.956** | [0.906, 0.980] | 129/135 |
| Benign false-quarantine, detector ON | 0.000 | [0.000, 0.028] | 0/135 |
| Poisoned records quarantined, detector ON | 0.000 | [0.000, 0.028] | 0/135 |

- **defense delta (OFF − ON) = 0.0000**
- **poisoning delta (poison − benign floor) = 0.0000**
- mean embedding z-score vs the pre-attack baseline: **poison 1.08**, benign 1.06,
  against a default threshold of **3.0**

### 1. The z-score lane does not defend against this

0 of 135 poisoned records were quarantined. The mean z-score of a poisoned
record is **1.08σ** against a 3.0σ threshold — it is not close.

ADR 0003 pre-registered this prediction before any code existed: *"a real dense
embedder puts poisoned content at roughly 1.5σ, under the 3.0σ default … If that
prediction holds, the honest headline is 'the z-score lane does not defend
against this'."* It holds, and at a lower σ than predicted. The headline is the
negative result, not a reframing to whatever the detector does catch.

The false-quarantine rate of 0/135 is **not** evidence of a well-calibrated
detector. It is 0 because the detector fires on nothing at all. A perfect
specificity number from a detector with zero sensitivity is arithmetic, not
security.

### 2. The attack rate is entirely explained by topical retrieval

This is the finding that changes how the first row should be read.

A naive report would publish "**95.6%** attack success rate" and it would look
alarming. But the benign twin — same opening clause, same tags, same restatement
of the victim query, differing only in that it resolves to *no answer* — is
retrieved at **exactly the same rate**, 129/135.

So `retrieved@k` here is measuring *"does a topically-matched record about this
query come back for this query"*, and the answer is yes, for poisoned and
innocuous records alike. The poisoning-specific effect is **0.0000**.

The stronger oracle agrees. Asked whether the injected record *outranks the
corpus's own gold record*, the poison manages 0.800 [0.725, 0.859] and the
benign twin 0.822 [0.749, 0.877] — the poison does slightly **worse**.

### 3. The null is not an artifact of a saturated oracle

At k=10 both arms miss on the same two queries, so the top-k oracle is at
ceiling and has little power to separate them. A null from a saturated measure
is worth little, so the same measurement was taken at smaller k:

| k | poison, detector OFF | benign floor | defense delta | poisoning delta |
|---:|---:|---:|---:|---:|
| 1 | 0.756 (102/135) | 0.778 | +0.0000 | **−0.0222** |
| 3 | 0.956 (129/135) | 0.956 | +0.0000 | +0.0000 |
| 10 | 0.956 (129/135) | 0.956 | +0.0000 | +0.0000 |

At k=1, where the oracle is nowhere near ceiling, the poisoned record is
retrieved *less* often than its benign twin. No k shows a poisoning advantage.

## How it was measured

Four arms over the same corpus, all through the ordinary `remember()` /
`recall()` path — no privileged write, no test-only backdoor. If a harness needs
a special API to poison memory, it is not modelling the attack.

| arm | detector | what it answers |
|---|---|---|
| poison OFF | forced off | can a shortened record be exploited at all |
| poison ON | as shipped | does the shipped defense change that |
| benign OFF | forced off | the floor — how much is topical retrieval |
| benign ON | as shipped | does the defense quarantine innocent records |

The z-score baseline is trained on **pre-attack memory only**. Training it on
post-attack memory would let the poison define its own normal.

Recency is neutralised (`recency_half_life_hours = 1e12`) because the corpus is
batch-seeded in one burst and the injected record is always written last —
leaving recency live would let write *order* decide the ranking and manufacture
the result.

| | |
|---|---|
| corpus | `crates/mnemo-core/benches/data/longmemeval_m.jsonl`, sha256 `9ed6e435558d…` |
| distinct victim queries | 45 |
| seeds | 1, 2, 3 (each permutes insertion order) |
| trials | 540 (4 arms × 45 queries × 3 seeds), all recorded individually in the artifact |
| embedder | `Xenova/all-MiniLM-L6-v2`, 384-dim, sha256 `759c3cd2b7fe…`, **pinned — the run refuses to start on a mismatch** |
| storage backend | DuckDB (in-memory) |
| fixture | `bench/minja_phase3/fixtures/phase3_records.json`, committed and hashed into the result |

### The denominator to quote is 45, not 135

The 135 trials are 45 distinct victim queries repeated across 3 seeds, so they
are not independent. The pooled interval is narrower than the data earns. The
artifact publishes both; the conservative reading at the distinct-query
denominator is **[0.852, 0.988]** at n=45, and that is the one to quote.

Per-seed rates were identical (0.9556, 0.9556, 0.9556): insertion order did not
move the result.

## Reproduce

```bash
curl -sSL --fail -o model.onnx \
  https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx
curl -sSL --fail -o tokenizer.json \
  https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json
MNEMO_ONNX_MODEL_PATH=./model.onnx cargo run --release --features onnx \
  -p mnemo-minja-phase3-bench --bin minja_phase3_bench
```

The weights are pinned by digest; a different checkpoint is refused before
anything is measured, so this command reproduces the published figure or fails
loudly. It cannot quietly produce a different one.

## What this does **not** establish

- **It is not a MINJA number.** Phases 1 (indication-prompt injection) and 2
  (progressive shortening) are not implemented. Phase 2 is generative and
  adaptive; faking it with fixtures is precisely what ADR 0003 says makes a
  result stop being MINJA.
- **It says nothing about an adaptive attacker.** The attacker here never sees
  the defense and never reacts to it. An attacker who can observe quarantine
  decisions and iterate is a different and strictly harder threat model, and is
  not measured.
- **One corpus, one embedder, one k, one threshold.** 45 distinct queries from a
  single bundled LongMemEval_M slice, on `all-MiniLM-L6-v2`, at the default 3.0σ.
  Nothing here generalises to another corpus, a different embedding model, or a
  tuned threshold.
- **The fixtures are structurally derived, not paper-verbatim.** They reproduce
  the described *end state* of Phase-2 shortening — an ordinary-register note
  asserting a competing answer with no bridging markers. They are not quotations
  from arXiv:2503.03704 and were not produced by an attacker model.
- **n=45 is below the repo's own n≥100 bar,** and below ADR 0003's acceptance
  criterion of N≥100 target queries. The corpus is pinned by the issue's scope
  decision, so this shortfall is disclosed rather than fixed.
- **The structural oracle is not an answer-correctness judge.** It measures
  whether the poisoned record was *retrieved*, not whether a model would then
  have been misled by it. An LLM judge would be a second number, never a
  replacement.
- **It measures one defense lane.** The always-on lexical/self-referential lane
  is not exercised, by construction: these records carry no bridging markers.
  `bench/poisoning` measures that lane, where the canonical marker-carrying
  variant is quarantined 100% → ASR 0.

## Drift

The committed artifact is checked on every CI run by
`bench/minja_phase3/tests/committed_result_is_within_band.rs` (offline, ±0.05 on
rates; the quarantine count and defense delta are compared **exactly** because
they are structural). Regeneration against the real model runs nightly in
`.github/workflows/minja-phase3-nightly.yml`, which opens an issue on drift
rather than failing into a mailbox.
