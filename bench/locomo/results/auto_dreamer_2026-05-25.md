# Auto-Dreamer offline consolidation — 2026-05-25

> Auto-Dreamer-style offline consolidation bench. Exercises the engine's `run_decay_pass` + `run_consolidation` path on a synthetic multi-session trajectory and reports the two axes Auto-Dreamer headlines: **smaller active bank, equal-or-better recall**.

## Setup

- Sessions per trial: 6.
- Facts per session: 12.
- Trials: 3 (medians reported).
- Decay thresholds: archive=0.40, forget=0.10.
- Consolidation: tag-overlap clusters, `min_cluster_size = 3`.
- Engine: in-memory DuckDB + USearch (dim=3, `NoopEmbedding` — vector lane degenerate by design) + Tantivy BM25.

## Results (median across trials)

| metric | value |
|---|---:|
| active_bank_pre | 72.0 |
| active_bank_post | 1.0 |
| active_bank_ratio | 0.014 |
| recall_pre | 0.167 |
| recall_post | 1.000 |
| offline pass elapsed (ms) | 41.6 |

## Per-trial detail

| trial | active_pre | active_post | ratio | recall_pre | recall_post | decay (arch/forg) | cons (clusters/new/orig) | elapsed (ms) |
|---:|---:|---:|---:|---:|---:|---|---|---:|
| 0 | 72 | 1 | 0.014 | 0.167 | 1.000 | 0/0 | 1/1/72 | 43.3 |
| 1 | 72 | 1 | 0.014 | 0.167 | 1.000 | 0/0 | 1/1/72 | 41.6 |
| 2 | 72 | 1 | 0.014 | 0.167 | 1.000 | 0/0 | 1/1/72 | 41.5 |

## Auto-Dreamer assertions

- **Smaller active bank (`ratio < 1.0`):** **yes**.
- **Equal-or-better recall (`recall_post ≥ recall_pre`):** **yes**.

## Honest-disclaimer block

- **Not a faithful Auto-Dreamer reproduction.** Anthropic's description points at an LLM-driven reflection summarizer; mnemo's `run_consolidation` clusters by tag overlap and emits a structured `[Consolidated from N memories] …` bundle. The bench measures the *shape* of the active-bank vs recall tradeoff, not the absolute scores any LLM-based summarizer would report.
- **"Criterion-style"** here means the structured-report pattern the other `bench/locomo` bins use, not the `criterion` crate. The `criterion` target lives at `crates/mnemo-core/benches/longmemeval_bench.rs` and is intentionally separate.
- **`NoopEmbedding` makes the vector lane degenerate.** Recall signal rides on BM25 + tag clustering; the needle string survives the consolidation bundle so BM25 still finds it. For real numbers, swap to a real embedder (gated behind [#44](https://github.com/sattyamjjain/mnemo/issues/44)).
- **Backdated `created_at` + explicit `decay_rate` drive a deterministic decay outcome.** This is the bench's lever for producing reproducible Archived / Forgotten counts; production decay schedules are operator-tuned.
- **Single-agent, single-scope.** Multi-agent share / delegation paths are out of scope for this bin.
