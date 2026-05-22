# MINTEval-shaped interference bench — 2026-05-22

> Scaffold run reproducing the [arXiv:2605.18565](https://arxiv.org/abs/2605.18565) MINTEval shape against mnemo's `mnemo.recall` tool. Compares the default read path (fact-identity-unaware) against the v0.4.7 opt-in current-fact resolver.

## Setup

- Distractor pool: 50 synthetic facts per trial.
- Trials per K: 5.
- Target fact: revised K+1 times under the same `fact_id`.
- Scoring: top-1 content contains the most-recent revision (deterministic exact-match; GPT-judge scoring deferred behind [#44](https://github.com/sattyamjjain/mnemo/issues/44)).
- Engine: in-memory DuckDB + USearch (dim=3, `NoopEmbedding` — vector lane degenerate by design) + Tantivy BM25.

## Results

| K | default accuracy | resolver accuracy | resolver chain len | default p50 (ms) | resolver p50 (ms) |
|---:|---:|---:|---:|---:|---:|
| 1 | 100.0% (5/5) | 100.0% (5/5) | 1 | 157.75 | 37.79 |
| 3 | 100.0% (5/5) | 100.0% (5/5) | 3 | 47.82 | 59.85 |
| 5 | 100.0% (5/5) | 100.0% (5/5) | 5 | 166.42 | 68.42 |
| 10 | 100.0% (5/5) | 100.0% (5/5) | 9 | 215.13 | 208.98 |

## Honest-disclaimer block

- **Not a faithful MINTEval reproduction.** The paper uses a curated corpus + GPT-judge; this bin uses synthetic facts + exact-content match.
- **NoopEmbedding makes the vector lane degenerate.** The resolver still surfaces the interference signal because it post-processes after recall. For real numbers, swap to a real embedder (gated behind #44).
- **`chain len` column** is the length of the supersession chain emitted on a representative trial (K=10 typically emits 10 superseded entries; K=1 emits 1).
