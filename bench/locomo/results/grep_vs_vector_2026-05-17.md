# grep_vs_vector_replay — 2026-05-17

> **Scaffold run** reproducing the Sen et al. arXiv:2605.15184 experiment design (grep vs vector retrieval inside an agent harness) against mnemo's `mnemo.recall` tool. Smoke metric only — see disclaimer below.

## Setup

- Dataset: `/Users/sattyamjain/CommonProjects/mnemo/bench/locomo/../../crates/mnemo-core/benches/data/longmemeval_m.jsonl`
- Dataset SHA-256: `9ed6e435558d25cad1ead016cdf9ed87dbeda80edd18ae6fd5a9aed7cd5314ed`
- Records: 45
- Top-K per query: 5
- Engine: in-memory DuckDB storage + USearch HNSW (dim=3) + NoopEmbedding (zero vectors) + Tantivy BM25 full-text

## Results

| Mode (CLI) | mnemo strategy | Accuracy (smoke) | Query failures | p50 latency (ms) | p95 latency (ms) |
|---|---|---:|---:|---:|---:|
| `vector_only` | `"semantic"` | 6.7% (3/45) | 0/45 | 5.28 | 5.81 |
| `bm25_only` | `"lexical"` | 53.3% (24/45) | 2/45 | 2.71 | 4.27 |
| `rrf_hybrid` | `"auto"` | 6.7% (3/45) | 2/45 | 11.54 | 15.43 |

*Query failures* count as misses in the accuracy column. The common cause is Tantivy's BM25 query parser rejecting queries with un-escaped punctuation (apostrophes, question marks in certain positions). The failure column lets a reader see when a mode's accuracy is dragged down by parser strictness vs by the substrate's actual recall behaviour.

## Honest-disclaimer block

- **Not the official LongMemEval metric.** This bin uses a deterministic exact-substring match (`expected ⊆ any hit's content`) so a smoke run is reproducible without an API key. The official GPT-judge-scored metric requires `OPENAI_API_KEY` or `ANTHROPIC_API_KEY` and is gated behind [#44](https://github.com/sattyamjjain/mnemo/issues/44).
- **NoopEmbedding makes `vector_only` degenerate.** The scaffold ships zero-vector embeddings so the wiring is self-contained. Swap to `OnnxEmbedding` or `OpenAiEmbedding` for the gated run; the absolute `vector_only` accuracy here is meaningless until a real embedder lands.
- **Bundled dataset is synthesized, not the published LongMemEval.** 45 medical-dialogue records under `crates/mnemo-core/benches/data/longmemeval_m.jsonl`. Override via `MNEMO_LONGMEMEVAL_PATH` for the real 116-question LongMemEval slice (gated dataset access).
- **Comparison shape, not magnitudes.** The intent of this bin is to confirm each mode routes through mnemo's recall path end-to-end and to give operators a runnable scaffold for the gated comparison; absolute numbers from the smoke run are not comparable to the paper's published numbers.

## Reproducing

```text
cargo run --release --bin grep_vs_vector_replay -p mnemo-locomo-bench
```
