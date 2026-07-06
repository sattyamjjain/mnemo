# reproduction_bench — claimed vs observed (LoCoMo single-hop)

> 2026-07-06 — one well-known LoCoMo subtask (**single-hop retrieval**) re-run under mnemo's **default** hybrid recall (`strategy="auto"`: semantic + BM25 + graph-expansion + recency, RRF-fused), **deterministic and offline**. Published next to competitors' **own** LoCoMo figures — *cited, and NOT re-run in this harness*. Only mnemo's row is reproducible here. Reproducibility-by-disclosure, riding the 2026 memory-benchmark reproducibility crisis. No "best"/"first" claim.

- Dataset: LongMemEval_M single-hop slice (n=45), SHA-256 `9ed6e435558d25cad1ead016cdf9ed87dbeda80edd18ae6fd5a9aed7cd5314ed`.
- Embedder: `hash-bag-of-tokens (deterministic, offline)`; seed `0x10c020262026`.
- Metric: gold-document recall@K + MRR (each query answerable from its own turn's content; gold matched by `lme_id`). **Retrieval** quality — NOT the LLM-judged end-to-end QA accuracy the vendors report (gated; [#44](https://github.com/sattyamjjain/mnemo/issues/44)).

### Reproducibility method (two disclosed choices)

The default recall path is time- and approximation-dependent; two changes make the number bit-reproducible without altering mnemo's fusion:

1. **Exact vector index.** mnemo's default index is USearch **HNSW** — an *approximate* NN structure whose level-assignment RNG makes recall@k jitter run-to-run on tight-margin text. This bench swaps in an **exact brute-force cosine** index (distance, then stable insertion order on ties). HNSW is by construction an approximation of this exact search, so it is the deterministic reference HNSW tracks; a production HNSW deployment sees values within its approximate-NN noise floor. Every other lane (BM25, graph, the RRF fusion) is mnemo's default.
2. **Recency neutralised.** The corpus is seeded in one batch, so every memory is equally recent — a wall-clock recency signal carries no information here and only injects run-to-run noise. The recency half-life is set to ~ages so `recency_score ≡ 1.0` for all records (a constant lane), not dropped.

## Observed (mnemo, reproducible offline)

| metric | value |
|---|---:|
| **recall@1** | **24.4%** [Wilson 95% 14.2%, 38.7%] |
| recall@3 | 37.8% |
| recall@5 | 46.7% |
| MRR | 0.358 |

> **Disclosure:** 2/45 queries errored in the default `auto` BM25 lane (Tantivy's query parser rejects some natural-language punctuation, e.g. the apostrophe in *"patient's"*). The query is **not** sanitised — that would measure a non-default path — so an errored recall surfaces no gold and is counted as an honest **miss**. The observed rates above already include those misses. (Same handling as `semantic_recall_bench`; the parser gap in default recall is a real, disclosed limitation, not hidden.)

## Claimed (vendors' published LoCoMo figures — cited, NOT re-run here)

| system | claimed | note | source |
|---|---|---|---|
| Mem0 | 92.5 (LoCoMo, LLM-judged QA) | vendor-published; independent/community re-runs land materially lower (the reproducibility gap this bench rides) | [source](https://mem0.ai/research) |
| Zep | 84 → 58.44 (corrected) | the 84% LoCoMo claim was re-scored to 58.44% under corrected evaluation | [source](https://github.com/getzep/zep-papers/issues/5) |
| MemPalace | 100 → 60.3 R@10 (corrected) | 100% used top_k=50 (> sessions/conversation, i.e. every session returned); honest retrieval R@10 is 60.3% | [source](https://github.com/MemPalace/mempalace/issues/29) |
| Supermemory | ~99 (self-reported, not verified) | QA accuracy from an 8-/12-agent ensemble the authors frame as an experimental proof-of-concept, not production | [source](https://dev.to/varun_pratapbhardwaj_b13/5-ai-agent-memory-systems-compared-mem0-zep-letta-supermemory-superlocalmemory-2026-benchmark-59p3) |

**How to read this.** The *observed* row is mnemo's own number on a small bundled retrieval slice, reproducible offline with a fixed seed and a Wilson-95 you can re-run. The *claimed* rows are each vendor's **own published** figure at their own (often LLM-judged, full-dataset) protocol — reproduced here **only as citations**, not re-run in mnemo's harness. They are therefore **not a ranking against** the observed number: different task (retrieval vs end-to-end QA), different dataset scale, different judge. The corrected columns (Zep 84→58.44, MemPalace 100→60.3) are exactly why mnemo publishes a re-runnable number instead of a headline. Reproduce: `cargo run --release -p mnemo-locomo-bench --bin reproduction_bench`.
