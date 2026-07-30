# implicit_association — 2026-07-30

> mnemo's own **indirect-query (implicit-association)** retrieval probe, with an **orientation-cache** arm. Framing: InMind ([arXiv:2607.24368](https://arxiv.org/abs/2607.24368)) — **framing only, NOT a baseline** (see "What this is NOT").

## Setup

- Embedder: Ollama `nomic-embed-text` (768-dim), cosine HNSW; refuses to score under NoopEmbedding.
- Engine: in-memory DuckDB + USearch HNSW + Tantivy BM25 + `OrientationCacheStore`, RRF (`auto`) recall.
- Corpus: `bench/locomo/data/implicit_association.jsonl` (30 rows, 12 domains, 6 distractors/row).
- Corpus SHA-256: `bc591e337b004e2ae9ae3a55fc897d25ca89ef147fe75760c12b35c886a5534e`
- Protocol: top-k=10, 5 in-process repeats/row; orientation arm warmed by 2 prior recalls of the row's `direct_query`.

## Results

| arm | recall@1 | recall@5 | recall@5 Wilson-95 |
|---|---:|---:|---|
| `direct` (answer-blind control) | 0.987 | 1.000 | [0.975, 1.000] |
| `indirect` (blind spot) | 0.400 | 0.867 | [0.803, 0.912] |
| `indirect+orientation` — top-k memories (A) | 0.367 | 0.867 | [0.803, 0.912] |
| &nbsp;&nbsp;└ orientation-map surfaced (B) | — | 0.933 | [0.882, 0.963] |
| &nbsp;&nbsp;└ combined A‖B (@5) | — | 1.000 | [0.975, 1.000] |

Sub-counts A (top-k memories) and B (orientation map) are reported **separately, not merged**; the combined row is their OR at k=5. n(direct)=150, n(indirect)=150, n(orientation)=150.

- **Implicit-association blind spot** = `direct − indirect` recall@5 = **+0.133**.
- **Orientation-cache recovery** = `indirect+orientation(A‖B) − indirect` recall@5 = **+0.133**.

## Reading this honestly

`direct` measures whether the decisive record is retrievable at all; `indirect` measures whether a similarity query that shares no wording with it surfaces it (the blind spot). The orientation arm tests whether a constant-token context map, warmed by the fact's own earlier access, keeps the decisive entity visible for a later indirect question. Sub-count B (the map) is a binary *surfaced* signal, not a ranked hit — that is why it is reported apart from the ranked top-k sub-count A. On a 30-row corpus the Wilson intervals are wide; treat sub-0.1 gaps as ties and the numbers as directional.

## What this is NOT

- **NOT a reproduction of InMind, and NOT comparable to InMind's 84.0% / 14.4%.** InMind is a 125-task, expert-verified benchmark (113 tasks grounded in citable public sources) that scores an **LLM backbone's answers** with an in-context arm. This bin is mnemo's **retrieval-side** probe on a 30-row hand-built corpus with **no LLM and no in-context arm**: it measures whether the decisive record is *surfaced*, not whether a model *answers correctly*. arXiv:2607.24368 is the framing, not a baseline.
- **NOT a leaderboard claim.** Small n ⇒ wide CIs.
- **Reproducible + local**: fixed corpus (SHA above), local Ollama, no API key, no network beyond localhost.

## Reproducing

```text
ollama pull nomic-embed-text
cargo run --release -p mnemo-locomo-bench --bin implicit_association
```
