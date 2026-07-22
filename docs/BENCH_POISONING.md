# Memory-poisoning defense benchmark — real embedder

> **Honest measurement, not a marketing number.** This benchmark runs mnemo's
> **shipped** poisoning detector against MINJA- and consolidation-style memory
> poisoning through a **real semantic embedder** (ONNX `all-MiniLM-L6-v2`,
> 384-dim — no API key), and reports the detector's **Attack Success Rate (ASR)**
> with a **Wilson 95%** interval and the **benign false-positive rate**. It
> **refuses to score** under a no-op (zero-vector) embedder.

- Runner: [`bench/poisoning/src/bin/poisoning_real_bench.rs`](../bench/poisoning/src/bin/poisoning_real_bench.rs)
- Harness: [`bench/poisoning/src/real_embedder_bench.rs`](../bench/poisoning/src/real_embedder_bench.rs)
- Raw result (deterministic key order, no wall-clock in payload): [`bench/results/poisoning_real.json`](../bench/results/poisoning_real.json)
- Sibling (byte-stable, **hash** embedder, defense-*delta*): [`bench/poisoning/`](../bench/poisoning/)

## Why a *real-embedder* poisoning bench

mnemo already ships a byte-reproducible poisoning defense-delta bench — but it
runs a **hashed-bag-of-tokens** embedder for determinism, and openly defers the
hard case in its own report:

> *"a poison written entirely in in-distribution vocabulary (semantic poisoning
> with no novel tokens) would not trip the z-score gate at all — that blind spot
> is real but needs a generative judge … noted, not benchmarked here."*

This bench closes that gap by exercising the **same shipped defense** through a
**real** dense embedder. The result **corrects** an optimistic reading of the
hash bench (see [Headline finding](#headline-finding)).

## What is exercised (the real, shipped defense path)

Every poison is written through the real `remember()` path and read back through
`recall()` — no stubs:

| stage | call | file |
|---|---|---|
| write-time anomaly scan | `check_for_anomaly` | `crates/mnemo-core/src/query/poisoning.rs` |
| quarantine on anomaly | `quarantine_memory` | `crates/mnemo-core/src/query/poisoning.rs` |
| read-time skip | `recall` skips `quarantined` records | `crates/mnemo-core/src/query/recall.rs` |
| embedding z-score lane | `PoisoningPolicy::with_outlier_threshold` + `score_embedding_outlier` vs a trained `EmbeddingBaseline` | `crates/mnemo-core/src/anomaly/outlier.rs` |

The detector has two lanes relevant here: an **always-on lexical /
self-referential** lane (matches MINJA bridging phrasing on indirect-ingest
records) and an **opt-in embedding z-score** lane (flags writes whose embedding
is ≥ `threshold` normalised-Mahalanobis σ from the agent's benign baseline).

**ASR = fraction of poisons that survive to a recall** = *not quarantined on
write* **and** *retrieved in top-k*. Lower is better.

## Setup

| | |
|---|---|
| Embedder | ONNX `all-MiniLM-L6-v2`, 384-dim, cosine (local, **no API key**) |
| Engine | in-memory DuckDB + USearch HNSW + Tantivy BM25, `strategy="auto"` RRF |
| Benign corpus / seed | 200 fluent in-distribution clinical-style notes (≥ `MIN_BASELINE_SAMPLES` = 30, so the z-score baseline is active) |
| z-score threshold | 3.0 σ (`PoisoningPolicy::with_outlier_threshold(3.0)`) |
| Trials | 30 poisons / attack / seed × **3 seeds** = **90** per attack |
| top-k | 5 |
| CI | Wilson 95% on pooled (hits, n) |
| Hardware | Apple Silicon (`aarch64/macos`) |

## Attack patterns (mnemo roadmap #37: MINJA + consolidation)

1. **MINJA (canonical)** — indirect-ingest note (`SourceType::Retrieval`,
   `source:web`) carrying MINJA's self-referential *bridging* phrasing
   ("from now on, always … whenever asked about … remember this").
2. **MINJA (evasive)** — the same false fact with the bridging markers stripped
   to a bare statement. Probes the lexical lane's disclosed blind spot.
3. **Consolidation (off-distribution trigger)** — a fluent "consolidated note"
   redirect whose payload is a **novel token** (`zqxtrigger…`, `zqxpayload…`),
   no lexical markers. Isolates the embedding z-score lane.
4. **Consolidation (in-distribution)** — a fluent redirect in benign vocabulary,
   no novel tokens, no markers. The semantic-poison case the hash bench could
   not test.

## Results (ONNX `all-MiniLM-L6-v2`, n=90/attack, 3 seeds)

| attack | defense lane | ASR (defense ON) | 95% CI | ASR (OFF) |
|---|---|---:|---|---:|
| MINJA (canonical) | lexical / self-referential | **0.0%** | [0.0, 4.1] | 100.0% |
| MINJA (evasive, markers stripped) | lexical blind spot → z-score | 100.0% | [95.9, 100] | 100.0% |
| Consolidation (off-distribution trigger) | embedding z-score (real embedder) | 100.0% | [95.9, 100] | 100.0% |
| Consolidation (in-distribution) | semantic blind spot (disclosed) | 100.0% | [95.9, 100] | 100.0% |

**Benign false-positive control:** **0 / 300** clean in-distribution writes
quarantined = **0.0%** [95% 0.0, 1.3]. The defense does not harm legitimate
memories.

### z-score lane evidence (proves the gate is engaged)

To rule out "the gate silently didn't run," each poison payload is also scored
directly with the shipped `score_embedding_outlier` against the trained baseline
(n = 200 ≥ 30, so the gate is **active**), at the same 3.0σ threshold:

| attack | mean benign z | mean poison z | max poison z | flagged at 3σ |
|---|---:|---:|---:|---:|
| MINJA (canonical) | 1.24 | 1.55 | 1.59 | 0% |
| MINJA (evasive) | 1.24 | 1.56 | 1.59 | 0% |
| Consolidation (off-distribution trigger) | 1.24 | 1.44 | 1.47 | 0% |
| Consolidation (in-distribution) | 1.24 | 1.26 | 1.33 | 0% |

Poison embeddings sit only **~1.3–1.6σ** from the benign mean — barely above the
benign set's own **1.24σ**, and far below the 3.0σ threshold. The gate ran; the
poisons are simply **not embedding-space outliers** under a real dense embedder.

## Headline finding

- **The lexical / self-referential lane is the effective defense.** Against the
  canonical MINJA attack (the one the paper actually describes), it drops ASR
  from **100% → 0%** with **0% benign false-positives**, even with a real
  embedder. This is mnemo's real, always-on protection.
- **The embedding z-score lane does *not* add protection here — and this
  corrects the hash-embedder bench.** The sibling bench reported the z-score gate
  catching a novel-token "AgentPoison" trigger. That result was an **artifact of
  the hash embedder**: novel tokens landed in literally unseen hash dimensions
  (hitting the `1e-6` variance floor → astronomical z-scores). A **real dense
  embedder does not behave that way** — it maps even gibberish into the semantic
  manifold, so the same poisons score ~1.5σ, below any usable threshold. At 3σ
  the lane flags **0%** of poison **and** 0% of benign; there is no clean
  operating point where it separates them (≈0.3σ margin). **Operators should not
  rely on the z-score lane against fluent or marker-stripped semantic poison.**
- **Marker-stripped and consolidation-style redirects survive (ASR 100%).** This
  is a real, disclosed gap: mnemo does not currently detect fluent in-vocabulary
  memory poisoning at the embedding layer. The defense-in-depth answer is
  write-side provenance/trust controls + the lexical lane, not the z-score gate.

## Reproduce (no credentials)

```bash
# One-time: fetch a public sentence-transformer ONNX export (model.onnx +
# tokenizer.json side by side), e.g. sentence-transformers/all-MiniLM-L6-v2.
MNEMO_ONNX_MODEL_PATH=/path/to/all-MiniLM-L6-v2/model.onnx \
  cargo run --release --features onnx -p mnemo-poisoning-bench --bin poisoning_real_bench
# → writes bench/results/poisoning_real.json + prints the tables above
```

Alternative real embedders (same guard, same metric): `--embedder openai`
(`OPENAI_API_KEY`) or `--embedder ollama` (local Ollama). The bench is **never**
gated behind a paid embedder.

**Refuse-to-score-on-noop.** `run_real_bench` routes the embedder through
`guard_real_embedder` and **errors out before touching the store** if the
embedder is non-semantic (all-zero) — a poisoning number produced by a
zero-vector embedder is meaningless (the z-score lane is provably inert). In an
environment without a real embedder the bench **fails loud rather than emitting a
fabricated number** (unit test `refuses_noop_embedder`). CI exercises the harness
on the offline `DeterministicEmbedding` (real, non-zero) so correctness is
covered without a model download; the published ONNX numbers above require the
model.

## Limitations — what this is **NOT**

- **Retrieval-level, not end-to-end.** ASR measures whether the poison *survives
  to a recall*, not whether a downstream LLM is actually redirected. The latter
  needs a generative agent + judge (out of scope; [#44](https://github.com/sattyamjjain/mnemo/issues/44)).
- **One embedder, one threshold.** Results are for MiniLM at 3σ. A different
  embedder or a swept threshold may shift the z-score lane — but the ~0.3σ
  poison/benign margin here suggests no threshold yields a clean separation for
  these attacks.
- **DuckDB backend only.** The PostgreSQL/pgvector path is not exercised.
- **Fixed attack templates.** Four hand-written families; not an exhaustive
  adversary. A determined attacker adapts.
- **Not a claim that mnemo is poisoning-proof.** It is an honest measurement of
  what the shipped detector does — including where it does not help.
