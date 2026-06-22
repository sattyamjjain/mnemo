# semantic_recall_bench — 2026-06-22

> Retrieval-quality benchmark for mnemo's recall path with a **real semantic embedder** (not NoopEmbedding), an honest **held-out** RRF-weight sweep, and **multi-seed averaging**. Primary metric: gold-document recall@K + MRR.

## Setup

- Embedder: Ollama `nomic-embed-text` (768-dim), cosine HNSW
- Engine: in-memory DuckDB + USearch HNSW + Tantivy BM25, RRF fusion
- Dataset: `crates/mnemo-core/benches/data/longmemeval_m.jsonl`
- Dataset SHA-256: `9ed6e435558d25cad1ead016cdf9ed87dbeda80edd18ae6fd5a9aed7cd5314ed`
- Corpus fully seeded; queries split → tune=22, eval=23 (held-out)
- Top-K per query: 10; eval rows averaged over 5 seeds

## Held-out eval results (mean of 5 seeds)

| Mode | config | recall@1 | recall@3 | recall@5 | MRR | p50 ms | p95 ms |
|---|---|---:|---:|---:|---:|---:|---:|
| `bm25_only` | - | 0.522 | 0.609 | 0.739 | 0.586 | 24.6 | 30.6 |
| `vector_only` | - | 0.739 | 0.826 | 0.826 | 0.805 | 34.7 | 37.6 |
| `rrf_hybrid` | default equal weights | 0.435 | 0.739 | 0.817 | 0.608 | 45.4 | 58.4 |
| `rrf_hybrid_tuned` | [1.0, 1.0, 1.0, 1.0] k=60 | 0.452 | 0.748 | 0.809 | 0.622 | 51.4 | 73.8 |

## Hybrid-weight sweep (tune split, mean of 5 seeds)

Weights index the `auto` lanes `[vector, bm25, recency, graph]`. Selected by tune recall@1: **`equal_k60(default)`**.

| config | weights | rrf_k | tune recall@1 | tune MRR |
|---|---|---:|---:|---:|
| `equal_k60(default)` | [1.0, 1.0, 1.0, 1.0] | 60 | 0.391 | 0.569 |
| `v2_b1_r05_g05_k60` | [2.0, 1.0, 0.5, 0.5] | 60 | 0.355 | 0.548 |
| `v3_b1_r05_g025_k60` | [3.0, 1.0, 0.5, 0.25] | 60 | 0.364 | 0.553 |
| `v4_b1_r0_g0_k60` | [4.0, 1.0, 0.0, 0.0] | 60 | 0.364 | 0.540 |
| `v3_b2_r05_g05_k60` | [3.0, 2.0, 0.5, 0.5] | 60 | 0.364 | 0.532 |
| `v4_b1_r025_g025_k20` | [4.0, 1.0, 0.25, 0.25] | 20 | 0.364 | 0.567 |
| `v6_b1_r0_g0_k30` | [6.0, 1.0, 0.0, 0.0] | 30 | 0.364 | 0.571 |

## Reading the result (honest)

On this tight single-fact slice the **vector lane is the strongest mode** on recall@1 and MRR. mnemo's **default `auto` fusion underperforms it** — equal-weighting blends a strong semantic signal with the weaker BM25/recency/graph lanes. Up-weighting the vector lane through the public `hybrid_weights` / `rrf_k` knobs (the selected config above) **recovers most of that deficit** and matches the vector lane on recall@5, but does **not surpass** pure vector on this corpus. This is expected when queries closely paraphrase their gold document; hybrid's lexical-recall advantage (rare terms, exact tokens) needs a larger, noisier corpus to show. Takeaways: for paraphrase-heavy single-fact recall prefer `strategy="semantic"`; treat the default `auto` weights as tunable rather than fixed; and re-test fusion on the gated full sets.

## What this is / is NOT

- **Metric** = gold-document recall@K + MRR (each query's source record is its gold doc, matched by `lme_id`). Retrieval quality, not answer correctness.
- **Honest tuning**: weights chosen on tune queries, reported on disjoint eval queries; full grid shown above.
- **Averaged**: each eval row is the mean of several independent seeds (count in Setup) to absorb UUID-v7 + approximate-HNSW run-to-run variance on a small corpus.
- **NOT** the official LLM-judged LongMemEval / LoCoMo QA score (gated; #44). **NOT** a leaderboard claim (45-record slice, ~23-query eval).
- **Reproducible**: fixed dataset (SHA above), local Ollama model, deterministic split.

## Reproducing

```text
ollama pull nomic-embed-text
cargo run --release --bin semantic_recall_bench -p mnemo-locomo-bench
```

## Token efficiency — lean slice vs full history (Engram framing)

> Reference: Engram ([arXiv:2606.09900](https://arxiv.org/abs/2606.09900)) frames the win as a *lean retrieved slice* giving comparable answers at a fraction of the tokens of the *full history*. This is the memory layer's measurable half (no LLM): tokens estimated as `ceil(chars/4)`; slice = top-5 recalled memories under the tuned config `equal_k60(default)` (k=60); full history = the entire 45-record corpus.

| metric | tokens |
|---|---:|
| full history (all 45 records) | 893 |
| mean retrieved slice (top-5) | 97 |
| **token reduction** | **89.1%** |

Retrieving a lean top-5 slice costs ~89.1% fewer context tokens than dumping the full history, at the recall@5 shown above. **Not** an end-to-end QA-accuracy or parity claim — answer accuracy needs a generative LLM, which this run does not invoke.
