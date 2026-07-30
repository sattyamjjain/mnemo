# Implicit-association retrieval probe (with the orientation-cache arm)

mnemo's own **indirect-query** retrieval probe: does the memory layer surface a
decisive stored fact when the query shares **no wording** with it and only
bridges through world knowledge? And does mnemo's opt-in, constant-token
**orientation cache** — which keeps distilled decisive knowledge visible
alongside recall — close that gap?

- **Bin:** [`bench/locomo/src/bin/implicit_association.rs`](../../bench/locomo/src/bin/implicit_association.rs)
- **Corpus:** [`bench/locomo/data/implicit_association.jsonl`](../../bench/locomo/data/implicit_association.jsonl) (30 rows, 12 everyday domains, 6 distractors/row, source-cited bridges)
- **Latest run:** [`bench/locomo/results/`](../../bench/locomo/results/) (`implicit_association_<date>.{md,json}`)
- **Framing:** InMind, [arXiv:2607.24368](https://arxiv.org/abs/2607.24368) — **framing only, not a baseline** (see below)

## Why

Similarity retrieval surfaces a fact when a query *resembles* it. Agents ask
**indirect** questions whose answer hinges on a stored fact they share no wording
with — e.g. stored *"My anniversary falls on Bastille Day"*, asked *"which mid-July
fireworks holiday should I plan a party around?"*. Bridging requires world
knowledge (Bastille Day = 14 July), which a pure embedding lookup of the query may
not carry. mnemo ships the mechanism InMind argues recovers most of this gap: the
namespace-scoped [orientation cache](../../crates/mnemo-core/src/query/orientation_cache.rs)
(PEEK-anchored, arXiv:2605.19932), a deterministic distiller that extracts
capitalized entities / `UPPER_SNAKE = value` constants / fenced schemas from recall
hits into a bounded, constant-token "context map" returned alongside the memories.
Until now no bench measured whether that map surfaces a fact an indirect query does
not resemble.

## Design

Each corpus row is a source-cited implicit association: a `stored_fact` containing a
capitalized `target_substring` entity; an `indirect_query` that shares **no
significant token** with the fact (enforced by
[`implicit_association_corpus.rs`](../../bench/locomo/tests/implicit_association_corpus.rs));
an answer-blind `direct_query` control; a named `bridge`; and 6 distractor facts so
retrieval has something to prefer.

Three arms per row, real embedder (Ollama `nomic-embed-text`, 768-dim), a fresh
in-memory engine (DuckDB + USearch HNSW + Tantivy BM25 + `OrientationCacheStore`)
each trial, `N=5` in-process repeats:

| arm | query | orientation cache | hit when |
|---|---|---|---|
| `direct` | `direct_query` | off | `target_substring` in top-k memories |
| `indirect` | `indirect_query` | off | `target_substring` in top-k memories |
| `indirect+orientation` | `indirect_query` | on, **warmed by 2 prior recalls of the row's `direct_query`** | (A) in top-k memories **or** (B) in the returned orientation map |

Sub-counts **A** (top-k memories) and **B** (orientation map) are reported
**separately, never merged**; B is a binary *surfaced* signal (the map is not
ranked), so it is kept apart from the ranked A. The runner refuses to emit a score
under `NoopEmbedding` (`guard_real_embedder`), and every rate carries a Wilson-95
interval (`stats::wilson_95`).

## Result (representative; see the dated results file for the live run)

| arm | recall@1 | recall@5 |
|---|---:|---:|
| `direct` (control) | ~1.00 | 1.00 |
| `indirect` (blind spot) | ~0.40 | ~0.87 |
| `indirect+orientation` — memories (A) | ~0.40 | ~0.87 |
| &nbsp;&nbsp;└ orientation-map surfaced (B) | — | ~0.93 |
| &nbsp;&nbsp;└ combined A‖B (@5) | — | **1.00** |

- **The blind spot is real:** every fact is directly retrievable (`direct` ≈ 1.00),
  yet the `indirect` query misses the target at rank-1 ~60% of the time and misses
  ~13% even at k=5 (`direct − indirect` recall@5 gap ≈ **+0.13**).
- **The orientation cache recovers it — via the map, not re-ranking:** the cache
  does **not** change the top-k memory ranking (sub-count A ≈ `indirect`), but its
  constant-token map surfaces the decisive entity ~93% of the time (sub-count B),
  lifting combined surfacing to 1.00@5 (`indirect+orientation − indirect` ≈ **+0.13**,
  fully closing the gap).

On a 30-row corpus the Wilson intervals are wide; treat sub-0.1 gaps as ties and the
numbers as directional.

## What this is NOT

- **NOT a reproduction of InMind, and its numbers are NOT comparable to InMind's
  84.0% / 14.4%.** InMind is a 125-task, expert-verified benchmark (113 tasks
  grounded in citable public sources) that scores an **LLM backbone's answers** with
  an in-context arm. This is mnemo's own **retrieval-side** probe on a 30-row
  hand-built corpus with **no LLM and no in-context arm** — it measures whether the
  decisive record is *surfaced*, not whether a model *answers correctly*.
  arXiv:2607.24368 is cited as the **framing**, not a baseline.
- **NOT a leaderboard claim.** 30 rows ⇒ wide CIs.
- **NOT a faithful PEEK reproduction** — the orientation cache uses a deterministic
  regex distiller, not a learned extractor.
- The orientation arm is explicitly **warmed by prior direct access to the fact**;
  the number quantifies "keep a previously-seen fact visible for a later indirect
  question", not zero-shot bridging.

## Reproducing

```text
ollama pull nomic-embed-text
cargo run --release -p mnemo-locomo-bench --bin implicit_association
```

No API key, no network beyond `localhost`. The corpus SHA-256 is recorded in the
results JSON so anyone can confirm they ran the committed rows.
