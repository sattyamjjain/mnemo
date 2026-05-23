# Orientation-cache vs hybrid-only — 2026-05-23

> PEEK-anchored bench ([arXiv:2605.19932](https://arxiv.org/abs/2605.19932)). Measures the v0.4.8 orientation-cache mode against the default hybrid path on a repeated-context scenario. Reports (a) the bounded constant-token guarantee of the rendered map, (b) the per-call payload delta the cache adds, and (c) top-1 hit-rate parity.

## Setup

- Trials per K: 2.
- K values: [3, 6, 10, 15] (number of repeated-context recall calls per trial).
- Shared context: 30 facts referencing a fixed cast of entities + constants + a fenced schema.
- Token estimate: heuristic ~4 chars/token (not `tiktoken-rs`).
- Engine: in-memory DuckDB + USearch (dim=3, `NoopEmbedding` — vector lane degenerate by design) + Tantivy BM25 + `OrientationCacheStore` (default 512-token budget).

## Results (median across trials)

| K | hybrid p50 hits-tokens | orientation p50 map-tokens | orientation p50 hits+map | map ≤ budget? | top-1 parity |
|---:|---:|---:|---:|---|---|
| 3 | 210 | 88 | 298 | yes | 0/2 vs 0/2 |
| 6 | 210 | 88 | 298 | yes | 1/2 vs 2/2 |
| 10 | 209 | 88 | 297 | yes | 0/2 vs 0/2 |
| 15 | 209 | 88 | 298 | yes | 0/2 vs 0/2 |

## Assertions

- **Constant-token guarantee:** rendered map ≤ 512-token budget on every K — **yes**
- **Top-1 parity:** orientation arm never lower than hybrid arm — **yes**

These are the v0.4.8 honest claims for the orientation cache. The cache is a *bounded augmentation* of the recall payload, not a payload reducer in this measurement. The PEEK-style win — agent uses the warm map to skip rehydrating hits in subsequent contexts — is a workflow optimisation downstream of the engine and is NOT measured here. See the honest disclaimers below.

## Honest-disclaimer block

- **Not a faithful PEEK reproduction.** Heuristic distiller, no learned encoder.
- **`NoopEmbedding` makes the vector lane degenerate.** BM25 + the orientation map carry the signal. For real numbers, swap to a real embedder (gated behind #44).
- **Token estimate is `(len / 4)`-heuristic.** Calibrate with `tiktoken-rs` for production sizing decisions.
- **This bench measures per-call payload only.** The workflow-level PEEK win (agent reads the map and requests fewer hits next call) is downstream of the engine and is not modeled here.
- **`top-1 parity` is a regression guard.** The orientation cache MUST NOT lower top-1 hit rate at the bench scale.
