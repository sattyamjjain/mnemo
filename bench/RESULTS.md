# mnemo — memory-quality benchmark (real embedder)

**One honest number, reproducible from a local Ollama model.** This is a
credibility asset, not a paid feature — the whole repo is Apache-2.0.

## Headline

Running mnemo's recall path with a **real semantic embedder**
(`nomic-embed-text`, 768-dim, via local Ollama) over the bundled
LongMemEval_M slice, gold-document retrieval on the held-out eval split:

| mode | recall@1 | recall@3 | recall@5 | MRR | errored |
|---|---:|---:|---:|---:|---:|
| **`vector_only`** (semantic) | **0.739** | **0.826** | **0.826** | **0.805** | 0 / 23 |
| `bm25_only` (lexical) | 0.522 | 0.609 | 0.739 | 0.586 | ~1 / 23 |
| `rrf_hybrid` (default weights) | 0.435 | 0.739 | 0.817 | 0.608 | ~1 / 23 |

**Token efficiency (Engram "lean slice vs full history" framing):** retrieving
a lean **top-5 slice** costs **~89% fewer context tokens** than dumping the
full session history (893 → ~97 estimated tokens; `ceil(chars/4)`), at the
recall@5 above.

The single number to quote: **semantic recall@1 = 0.739 (MRR 0.805), at ~89%
token reduction vs. full history**, real embedder, LongMemEval_M.

## Exact config (reproducible)

- **Embedder:** Ollama `nomic-embed-text` (768-dim), cosine HNSW — **never
  NoopEmbedding**. Dimensionality probed at runtime.
- **Engine:** in-memory DuckDB + USearch HNSW + Tantivy BM25, RRF fusion over
  `[vector, bm25, recency, graph]` lanes.
- **Dataset:** `crates/mnemo-core/benches/data/longmemeval_m.jsonl`
  (LongMemEval_M sample, 45 records).
  SHA-256 `9ed6e435558d25cad1ead016cdf9ed87dbeda80edd18ae6fd5a9aed7cd5314ed`.
- **Protocol:** full corpus seeded; queries split tune=22 / **eval=23
  (held-out)**; top-K=10 per query; eval averaged over 5 seeds.
- **Date:** 2026-06-22.
- **Reproduce:**
  ```bash
  ollama pull nomic-embed-text
  cargo run --release -p mnemo-locomo-bench --bin semantic_recall_bench
  # writes bench/locomo/results/semantic_recall_<date>.{md,json}
  ```

## Honest caveats (read these)

- **Single-run, not seed-averaged across process restarts.** Numbers are the
  mean of 5 in-process seeds, but the approximate-NN index (HNSW) + the RRF
  fusion-weight *selection* sit near a noise floor: across two back-to-back
  runs the swept "best" hybrid config flipped (`[6,1,0,0] k=30` ↔ equal
  weights `k=60`) and tuned recall@1 swung 0.70 ↔ 0.45. Treat any gap under
  ~0.05 as a tie. See *The FID Lottery* (single-seed eval noise) for why
  one-shot leaderboard deltas mislead. **`vector_only` is the one stable,
  reproducible strong mode** (recall@1 0.739 in every run); the hybrid/tuned
  numbers are within noise of each other.
- **This is retrieval quality + token efficiency, NOT end-to-end QA
  accuracy.** True LongMemEval QA accuracy needs a generative LLM to answer
  and a judge to score; this run has only an embedding model and invokes no
  LLM. We measure what the memory layer is actually responsible for: surfacing
  the gold evidence (recall@K/MRR) and doing it on a lean token budget.
- **LongMemEval_M (45 q), not LongMemEval_S (500 q).** The bundled slice is
  small and single-fact-paraphrase-heavy (which is why the vector lane
  dominates). It is a wiring/credibility check, **not** a competitive
  leaderboard claim and **not** averaged at _S scale.
- **No cherry-picking.** mnemo's *default* `auto` RRF fusion (0.435 recall@1)
  underperforms pure `vector_only` on this paraphrase-heavy slice — reported
  as-is. Actionable signal: the default `auto` lane weights are tunable
  (`hybrid_weights` / `rrf_k`), not sacred; for paraphrase-heavy single-fact
  recall prefer `strategy="semantic"`.

## Reference, not a parity claim

Framed against **Engram** ([arXiv:2606.09900](https://arxiv.org/abs/2606.09900))
— "a lean retrieved slice answers as well as the full history at a fraction of
the tokens" — as a **reference point for the framing**, not a claim of parity
with Engram or any hosted memory service. The full per-mode tables, the
held-out RRF sweep, and the raw JSON live at
[`bench/locomo/results/semantic_recall_2026-06-22.md`](locomo/results/semantic_recall_2026-06-22.md).

## BEAM-style retrieval — reproduced vs. self-reported

A second, **deterministic** number over mnemo's default hybrid recall
(`strategy="auto"`: semantic + BM25 + graph-expansion + recency, RRF-fused) on
two BEAM-style subtasks. Run with the offline hashed embedder (no network, no
LLM), 100 queries × 5 pooled repeats/subtask, top-5, seed `0xbea320262026`
(2026-07-04):

| subtask | **reproduced** (this run, seed `0xbea320262026`) | self-reported (upstream) |
|---|---:|---:|
| `multi_hop` (graph-linked answer, no shared query token) | **0.6%** (3/500) [Wilson 95% 0.2%–1.7%] | — (BEAM reports one overall accuracy, not per-subtask) |
| `open_domain` (gold among same-schema distractors) | **68.6%** (343/500) [Wilson 95% 64.4%–72.5%] | Hindsight BEAM **64.1%** @ 10M tokens ([source](https://hindsight.vectorize.io/blog/2026/04/02/beam-sota)) |

**Honesty note — do not read the two columns as a ranking.** The reproduced
numbers are on a *small synthetic fixture* with a lexical offline embedder and
**no LLM judge**; the upstream **64.1%** is on the real BEAM benchmark (10M-token
corpus, LLM-graded). Self-reported memory scores are a vendor-run **upper
bound** — not independently reproduced across labs (the reproducibility gap the
[Hindsight paper](https://arxiv.org/abs/2512.12818) itself flags for pre-LoCoMo
memory evals) — and a synthetic-fixture number is **not comparable** to it. The
low `multi_hop` figure is an honest result, not a bug: mnemo's default `auto`
RRF barely surfaces an answer reachable *only* through a graph edge against
lexically-equivalent distractors — the `graph` / `reconstruct` strategies are
the tools aimed at multi-hop (see [`bench/locomo/src/bin/reconstruct_ab.rs`](locomo/src/bin/reconstruct_ab.rs)).
No "first" / "best" claim is made. Reproduce:
`cargo run --release -p mnemo-locomo-bench --bin beam_bench`
(writes `bench/locomo/results/beam_<date>.{md,json}`).

## Backend note

Semantic recall is supported on **both** backends: **DuckDB + USearch**
(default, used for the headline above) and, since v0.5.7,
**PostgreSQL + pgvector** — the pgvector HNSW ANN path is implemented
(`crates/mnemo-postgres/src/pgvector_index.rs`, [#99](https://github.com/sattyamjjain/mnemo/issues/99));
if the pgvector extension is genuinely absent it still hard-errors rather than
returning empty. The numbers above were measured on the DuckDB backend.
