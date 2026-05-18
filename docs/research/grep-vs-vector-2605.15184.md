# Grep vs vector retrieval inside agent harnesses — arXiv 2605.15184 anchor

> Recorded 2026-05-17. **Composition anchor, not a marketing claim.**
> Sen et al.'s paper is an external research artifact; mnemo's relevance
> is structural — hybrid RRF over BM25 + vector + graph + recency is
> already the documented default. This note records the framing the
> paper provides for mnemo's existing surfaces and the new
> v0.4.4 `RetrievalMode::HarnessAware` lever, without claiming any
> result the paper does not.

## Citation

- **Paper:** Sen et al. (PwC), "Is Grep All You Need?" (paraphrased
  title used in prose; literal title in §Sources).
- **arXiv:** [2605.15184](https://arxiv.org/abs/2605.15184)
- **Released:** May 2026

## What the paper measures

Two findings from the abstract drive today's anchor:

1. **"grep generally yields higher accuracy than vector retrieval in
   our comparisons in experiment 1"** — Across a controlled agent
   harness, exact-keyword BM25-style retrieval outperformed pure
   embedding-vector retrieval on the experiment-1 corpus.
2. **"overall scores still depend strongly on which harness and
   tool-calling style is used, even when the underlying conversation
   data are the same"** — The result envelope shape (how the
   retrieved hits are framed for the agent) is a measured lever
   independent of the substrate's retrieval strategy.

The paper's contribution is the *measurement*: a reproducible
experimental design that isolates retrieval mode from envelope
format. It does not propose a new substrate; it asks whether the
mainstream "vector-first" default is the right one.

## Where mnemo fits

mnemo's documented default retrieval path is **already hybrid RRF
over BM25 + vector + graph + recency**, with operator-configurable
weights via `RecallRequest.hybrid_weights`. The paper's BM25-favours
finding is the substrate's *expected* shape — mnemo did not need a
pivot to accommodate it; the substrate was already hedged.

What v0.4.4 adds for the second finding (envelope-format lever) is a
typed `RetrievalMode::HarnessAware { harness, format }` variant that
selects a per-harness envelope adapter (Claude Code / Codex /
Gemini CLI / Chronos / Generic) without changing which records the
substrate retrieves. The five adapters are intentionally minimal —
each produces a deterministic string shape; envelope-content
stability is not claimed in v0.4.4.

| Paper's lever | mnemo surface |
|---|---|
| Compare BM25 vs vector vs hybrid on the same data | `bench/locomo/src/bin/grep_vs_vector_replay.rs` (landed alongside this anchor in PR-A) routes a LongMemEval-shaped slice through `mnemo.recall` in all three modes and emits a Markdown table |
| Hold retrieval constant, vary envelope per harness | `RetrievalMode::HarnessAware { harness, format }` (new in v0.4.4) — the substrate retrieves via `HybridRrf`, an adapter reshapes the response per harness |
| Reproducible-experiment harness | The replay bin's smoke run is bundled-dataset-only; the gated 116-question LongMemEval slice + GPT-judge-scored metric require the same secrets as [#44](https://github.com/sattyamjjain/mnemo/issues/44) |

## What this anchor is NOT

- **Not an endorsement of grep-default.** mnemo's default stays
  hybrid RRF. The paper's first finding sharpens *why* the hybrid
  path is the right default (BM25 contributes more than a
  vector-first reading would suggest), not why mnemo should pivot
  to grep-default.
- **Not a benchmark-result claim.** The bundled smoke run in the
  replay bin uses `NoopEmbedding` (zero vectors) so the vector-only
  column is degenerate by design. Real numbers comparable to the
  paper need the gated run.
- **Not an integration with the paper's harness.** The paper does not
  ship an API or a public test corpus mnemo can bind against.
- **Not a stability claim on envelope contents.** The five
  `HarnessAware` adapters produce deterministic shapes per harness
  but the exact byte-level content is not a stability surface in
  v0.4.4. Operators pinning a specific envelope format should pin
  the mnemo minor version.

## Why a `RetrievalMode` typed enum at all

Before v0.4.4, `RecallRequest.strategy: Option<String>` was the only
recall-mode lever — string-typed, with magic values `"semantic"`,
`"lexical"`, `"auto"`, `"graph"`. The paper's second finding
(envelope-format matters independently of retrieval) is awkward to
express as a string variant — `HarnessAware` carries two fields
(`harness`, `format`) that don't compose into a flat string.

v0.4.4 introduces `RetrievalMode` as a typed superset alongside the
legacy string field. `RecallRequest.mode: Option<RetrievalMode>` is
additive — when set it takes precedence; when unset the engine
falls back to parsing `strategy` exactly as in v0.4.3. **No SDK
breaking change** — the Python / TypeScript / Go SDKs continue to
marshal `strategy: string` and continue to work.

A future v0.5.x can migrate the SDKs to a typed `mode` field; that
breaking change is out of scope for v0.4.4.

## Cross-references

- Bench scaffold: [`../../bench/locomo/src/bin/grep_vs_vector_replay.rs`](../../bench/locomo/src/bin/grep_vs_vector_replay.rs) — runnable today against the bundled `longmemeval_m.jsonl`.
- RetrievalMode enum + 5 adapters: [`../../crates/mnemo-core/src/retrieval.rs`](../../crates/mnemo-core/src/retrieval.rs).
- Companion read-side composition anchor: [`argus-2605.03378.md`](argus-2605.03378.md) (ARGUS — 2026-05-05, same composition-anchor pattern).
- Companion outcome-diffing anchor: [`delegate52-2604.15597.md`](delegate52-2604.15597.md) (DELEGATE-52 — 2026-05-09).
- Companion curator anchor: [`../comparisons/anthropic-dreams.md`](../comparisons/anthropic-dreams.md) (Dreams — 2026-05-06).
- v0.4.4 carry list: [`../../CHANGELOG.md`](../../CHANGELOG.md) `[0.4.4]` section.
- Gated full-run blocker: [#44](https://github.com/sattyamjjain/mnemo/issues/44).

## Sources

- arXiv 2605.15184 — https://arxiv.org/abs/2605.15184 — *"Is Grep All You Need?"* (Sen et al., PwC, May 2026; literal title used here per the §Sources-only paraphrase rule).
- Hacker News front-page surface (per 2026-05-17 daily-prompt digest).
