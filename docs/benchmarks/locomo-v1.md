# LoCoMo v1 — mnemo's first **real-embedder** retrieval benchmark

> **Preliminary (n=45).** Retrieval quality of mnemo's recall path measured with a
> **real semantic embedder** — local ONNX `all-MiniLM-L6-v2` (384-dim), no API key —
> on the bundled LoCoMo-/LongMemEval-style slice. Primary metric: **gold-document
> recall@k + MRR**, with a **Wilson 95%** interval. This is *retrieval* quality, not
> LLM-judged end-to-end QA accuracy (that needs a gated dataset + judge model,
> [#44](https://github.com/sattyamjjain/mnemo/issues/44)).

Raw result: [`bench/results/locomo_v1.json`](../../bench/results/locomo_v1.json)
(deterministic key order, no wall-clock stamp in the payload).

## Why this bench exists

mnemo already ships a **byte-reproducible** LoCoMo number — but that one runs under a
*deterministic hash-bag-of-tokens* embedder so the report can be recomputed bit-for-bit
offline (`reproduction_bench`; recall@1 24.4% on `auto`). A hash embedder is a lexical
**floor**, not a semantic signal — it deliberately understates what mnemo's vector lane
does with a real model. This bench closes that gap: it wires a **real** embedder through
the *same* recall path and reports what semantic retrieval actually recovers, with an
honest confidence interval.

**Hard guard against a silent no-op.** The single worst failure mode for a retrieval
benchmark is scoring under `NoopEmbedding` (all-zero vectors) and publishing the
resulting "semantic recall" as if it meant something. Before scoring, the runner routes
its resolved embedder through `guard_real_embedder`
([`bench/locomo/src/real_embedder.rs`](../../bench/locomo/src/real_embedder.rs)) and
**refuses to emit any score** if the embedder is not semantic-capable, naming the
embedder it found. A silently-noop benchmark is worse than no benchmark. A unit test
(`refuses_noop_embedder`) pins this behaviour.

## Setup

| | |
|---|---|
| **Embedder** | ONNX `all-MiniLM-L6-v2`, 384-dim, cosine (local, **no API key**) |
| **Engine** | in-memory DuckDB storage + USearch HNSW (cosine) + Tantivy BM25, RRF fusion |
| **Corpus** | bundled LongMemEval_M slice, **45 records** — SHA-256 `9ed6e435558d25cad1ead016cdf9ed87dbeda80edd18ae6fd5a9aed7cd5314ed` |
| **Dataset path** | `crates/mnemo-core/benches/data/longmemeval_m.jsonl` |
| **Metric** | gold-document recall@k (each query's source record is its gold doc, matched by `lme_id` metadata) + MRR |
| **Queries (n)** | 45 |
| **Top-K** | 10 |
| **Seeds** | mean of **5** (absorbs UUID-v7 + approximate-HNSW run-to-run variance) |
| **CI** | Wilson 95% on recall, computed on `round(mean_recall × n)` successes over n=45 |
| **Paired CI** | percentile bootstrap over queries (10 000 resamples, fixed seed) + exact McNemar |
| **Hardware** | `arm64/darwin` (Apple M4) |

## Results (mean of 5 seeds, n=45 — **preliminary**)

| strategy | recall@1 | recall@1 95% CI | recall@5 | recall@10 | MRR | p50 ms | p95 ms | index build ms |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| `lexical` (BM25) | 0.422 | [0.290, 0.567] | 0.689 | 0.689 | 0.501 | 9.2 | 21.9 | 622.6 |
| **`semantic`** (vector) | **0.689** | **[0.543, 0.805]** | **0.889** | **0.911** | **0.770** | 13.4 | 16.2 | 1050.4 |
| `auto` (RRF hybrid) | 0.631 | [0.476, 0.749] | 0.844 | 0.898 | 0.726 | 22.0 | 25.3 | 781.7 |

### Paired comparisons (the ones that answer the question)

Those three intervals are *marginal*: each says where one strategy's own recall
lies, and none of them says whether one strategy beats another. Overlapping
intervals do not establish a difference and do not rule one out. Because every
strategy is scored on the **same 45 queries**, the comparison can be made
properly — each query is its own control:

| comparison | paired Δ recall@1 | 95% CI | McNemar b/c | exact p | separates at 95%? |
|---|---:|---|---:|---:|---|
| `semantic` − `lexical` | **+0.267** | [0.133, 0.400] | 12 / 0 | 4.9e-4 | **yes** |
| `semantic` − `auto` | +0.058 | [−0.031, 0.160] | 4 / 2 | 0.69 | **no** |

The first row is the claim the README makes, and it holds: on 12 queries the
vector lane wins where BM25 fails, on **zero** does BM25 win where the vector
lane fails, and the interval on the gap excludes zero.

The second row is the honest correction to a comparison this page used to make
by eye. At n=45 the `semantic`-over-`auto` gap **does not separate**, and the
point estimate is not evidence that `auto` costs you recall. At this effect size
it would take roughly **n=127** paired queries to resolve — just past the repo's
own n≥100 bar, which is why the corpus size, not the fusion weights, is the next
thing to fix.

Latency is per query end-to-end **including the ONNX embed round-trip** for the
`semantic`/`auto` lanes; `lexical` needs no query embedding, hence its lower latency.

## Reading the result (honest)

On this tight single-fact slice the **vector (`semantic`) lane is the strongest mode** —
recall@1 **0.689**, recall@10 **0.911**, MRR **0.770**. mnemo's default `auto` RRF fusion
sits just below it (recall@1 0.631, a gap that does not separate — see the paired table):
equal-weighting a strong semantic signal with the
weaker BM25/recency/graph lanes dilutes it slightly when queries closely paraphrase their
gold document. BM25-only trails on recall@1 (0.422) and, notably, its recall@5 and
recall@10 are identical (0.689) — a lexical miss at rank 5 is still a miss at rank 10,
whereas the vector lane keeps recovering gold as k grows (0.889 → 0.911).

**Takeaways:** for paraphrase-heavy single-fact recall prefer `strategy="semantic"`; treat
the default `auto` weights as *tunable* (via the public `hybrid_weights` / `rrf_k` knobs)
rather than fixed; and re-test fusion on a larger, noisier corpus where BM25's exact-token
advantage has room to show. The `auto`-vs-`semantic` gap is **not significant** at n=45 —
that is now measured rather than eyeballed (paired Δ +0.058, 95% CI [−0.031, 0.160],
McNemar exact p=0.69), so do not switch away from the default on the strength of it.

## Reproduce (no credentials)

```bash
# 1. One-time: fetch a public sentence-transformer ONNX model + tokenizer.
#    Any all-MiniLM-L6-v2 export works; you need model.onnx + tokenizer.json
#    side by side (e.g. from sentence-transformers/all-MiniLM-L6-v2 on HF).
#    No auth, no gated download.

# 2. Run the bench against it (ONNX is the DEFAULT embedder):
MNEMO_ONNX_MODEL_PATH=/path/to/all-MiniLM-L6-v2/model.onnx \
  cargo run --release --features onnx -p mnemo-locomo-bench --bin locomo_v1_bench
# → writes bench/results/locomo_v1.json + prints the table above
```

Alternative real embedders (same guard, same metric): `--embedder openai` (needs
`OPENAI_API_KEY`) or `--embedder ollama` (local Ollama, `ollama pull nomic-embed-text`).
The bench is **never** gated behind a paid embedder — the default ONNX path runs offline
with no account.

## Limitations — what this is **NOT**

- **Preliminary, small n.** 45 queries. The Wilson 95% intervals are wide (±~0.13 on
  recall@1) and overlap across lanes — read magnitudes as indicative, not as a ranking
  with significance. Scaling to the gated full LoCoMo / LongMemEval sets is the follow-up
  ([#44](https://github.com/sattyamjjain/mnemo/issues/44)).
- **Retrieval, not QA.** The metric is gold-document recall, not the LLM-judged answer
  correctness vendors headline. No generative model is invoked here.
- **Not a competitive claim.** This is mnemo measured against *itself* across strategies.
  There is **no head-to-head** with Mem0 / Letta / Zep — running those on identical
  hardware was out of scope for this change, so no such number is asserted.
- **DuckDB backend only.** The engine under test is in-memory DuckDB + USearch HNSW +
  Tantivy. The **PostgreSQL / pgvector semantic path is NOT exercised** by this run.
- **Corpus is the bundled LongMemEval_M slice**, the repo's LoCoMo-style stand-in — not
  the full published LoCoMo conversation set.
- **Recall is not byte-deterministic.** Approximate-HNSW level-assignment RNG + fresh
  UUID-v7 ids make recall jitter slightly run-to-run; the committed JSON is one
  representative 3-seed run. The *byte-reproducible* companion number lives in
  [`reproduction_bench`](../../bench/locomo/results/reproduction_2026-07-06.md) (exact
  brute-force index + neutralised recency, hash embedder).

## Provenance

- Runner: [`bench/locomo/src/bin/locomo_v1_bench.rs`](../../bench/locomo/src/bin/locomo_v1_bench.rs)
- Guard: [`bench/locomo/src/real_embedder.rs`](../../bench/locomo/src/real_embedder.rs)
- ONNX embedder: [`crates/mnemo-core/src/embedding/onnx.rs`](../../crates/mnemo-core/src/embedding/onnx.rs) (`--features onnx`, `ort` 2.0.0-rc.11)
- Raw JSON: [`bench/results/locomo_v1.json`](../../bench/results/locomo_v1.json)
