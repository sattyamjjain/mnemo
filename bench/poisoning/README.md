# `poisoning_bench` — memory-poisoning defense delta

A **deterministic, offline, byte-stable** benchmark of mnemo's shipped
poisoning-quarantine defense against two named, published memory-poisoning
attacks. The headline is the **defense delta** — Attack Success Rate (ASR) with
the defense **OFF** vs **ON**. Every number here is **observed** in this harness;
none is a vendor-claimed figure.

```bash
cargo run --release -p mnemo-poisoning-bench
# writes bench/poisoning/results/poisoning_<date>.{md,json}
```

No network, no LLM, no API key. Two runs `diff` identically (see
`bench/poisoning/tests/`).

## The defense being toggled (verified in-repo)

There is **no "provenance-trust-filtered retrieval"** in mnemo — `provenance.rs`
is per-*read* HMAC receipts. The real defense is the **poisoning detector +
quarantine**:

```text
// crates/mnemo-core/src/query/remember.rs:61  (write path)
let anomaly_result = super::poisoning::check_for_anomaly(engine, &record).await?;
if anomaly_result.is_anomalous { super::poisoning::quarantine_memory(engine, id, ...).await?; }
// crates/mnemo-core/src/query/recall.rs:1138   (read path)
if record.quarantined { /* skipped — never returned from recall */ }
// crates/mnemo-core/src/query/poisoning.rs:50   (opt-in z-score lane)
pub fn with_outlier_threshold(mut self, threshold: f32) -> Self  // on PoisoningPolicy
```

**ON** = the store as shipped. **OFF** = the same, byte-identical poison record
with `quarantined` forced back to `false` (a store with no detector). The delta
isolates exactly the quarantine bit.

## Results (observed, seed `0x901504202607`, 200 trials/attack, top-5)

| attack | defense lane | ASR_off [95%] | ASR_on [95%] | **delta** |
|---|---|---:|---:|---:|
| MINJA (canonical) | lexical / self-referential | 100.0% [98.1, 100.0] | 0.0% [0.0, 1.9] | **+100.0 pts** |
| MINJA (evasive, markers stripped) | lexical / self-referential | 100.0% [98.1, 100.0] | 100.0% [98.1, 100.0] | **+0.0 pts** |
| AgentPoison (low-rate trigger, 0.0998% of store) | embedding z-score gate | 100.0% [98.1, 100.0] | 3.5% [1.7, 7.0] | **+96.5 pts** |

**Benign control:** 0/200 false-quarantine (0.0%) on held-out clean,
in-distribution memories. The delta is only meaningful at ~0% false-positive.

*(CIs are Wilson 95% score intervals. Re-run to reproduce the exact bytes.)*

## Attacks

- **MINJA** ([arXiv:2503.03704](https://arxiv.org/abs/2503.03704)) — indirect
  prompt-injection that writes a poisoned "fact" into memory via an
  indirect-ingest path (`SourceType::Retrieval`, tagged `source:web`). Canonical
  carries the self-referential bridging phrasing ("from now on, always…",
  "whenever asked about…"); the always-on lexical lane quarantines it.
- **AgentPoison**-style low-rate trigger — a single novel-token trigger poison
  planted among 1001 benign memories (**< 0.1%** of the store). The z-score
  outlier gate quarantines the large majority.

## Honesty (read this)

- **These are observed numbers, never claimed.** No "best"/"first" claim; not a
  statement that mnemo is poisoning-proof.
- **Disclosed blind spots.** Evasive MINJA (markers stripped) evades the lexical
  lane → delta ≈ 0. The AgentPoison residual `ASR_on` is non-zero because a
  novel token occasionally hash-collides into an in-distribution dimension of the
  128-dim embedder and evades. A poison written entirely in in-distribution
  vocabulary would not trip the z-score gate at all (noted, not benchmarked —
  needs a generative judge).
- **Benign-control coverage caveat.** The 0% false-quarantine holds because the
  1001-record baseline populates the embedding space; a narrower corpus, or a
  clean write bearing a brand-new identifier, raises false positives.

## Determinism

Fixed corpus + a deterministic hashed-bag-of-tokens embedder + an **exact
brute-force** vector index (the deterministic reference mnemo's approximate HNSW
tracks) + a **neutralised recency lane** (batch-seeded corpus has no recency
signal). Full config runs in a few minutes; the CI tests use small configs.
