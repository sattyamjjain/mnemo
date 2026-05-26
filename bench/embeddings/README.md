# `mnemo-embeddings-bench`

Embedding-backend selection bench + SLA-aware recommender. New in
v0.4.9.

Anchored on [arXiv:2605.23618](https://arxiv.org/abs/2605.23618)
(GE2 vs local encoders — quality + latency): the right embedding
backend is the one whose measured nDCG and tail-latency clear the
operator's SLO on the operator's workload, not the one with the
fanciest reputation.

This crate runs each available backend against a small labeled
fixture and reports:

- **nDCG@10** and **recall@10** (quality, binary relevance, log2 discount)
- **p50** and **p95** single-vector embed latency
- **throughput** at batch sizes 1 / 8 / 32 (vectors/sec)

Then the recommender picks the **highest-nDCG backend whose p95 is
≤ the SLO** and reports the nDCG gap vs the absolute best-quality
backend — so the operator sees the explicit quality tradeoff for
choosing the fast one.

## Backends measured

| Backend | When it runs | Notes |
|---|---|---|
| `noop` | always | Zero vectors — degenerate quality floor reference. |
| `hashing-baseline` | always | Bench-local deterministic character-3-gram feature-hashing trick. **Not** added to `mnemo-core`; lives in this crate only so default builds report a non-trivial row alongside `noop`. Lexical, not semantic. |
| `openai` | `OPENAI_API_KEY` is set | Network-bound. Uses `MNEMO_EMBEDDING_MODEL` (default `text-embedding-3-small`). |
| `onnx` | `MNEMO_ONNX_MODEL_PATH` is set AND `mnemo-core` is built with the `onnx` feature | Local inference; requires the model + `tokenizer.json` next to it. |

## Run

### CLI (recommended)

```bash
cargo run --release -p mnemo-mcp-server -- bench embeddings --slo-ms 50
```

The CLI prints the table + recommendation and exits.

### Criterion (latency only)

```bash
cargo bench -p mnemo-embeddings-bench --bench embedding_quality
```

HTML reports land in `target/criterion/embed_single/`. Quality
(nDCG@10 / recall@10) is **not** part of the criterion target — it
is computed in the CLI path via `mnemo_embeddings_bench::run_all`.

## Fixture

50 documents across 5 topics (databases, machine-learning,
networking, security, operating-systems) and 10 queries with
binary-relevance gold IDs. Lives at `data/corpus.json` and
`data/queries.json`. Designed so a real semantic embedder
out-performs lexical baselines (queries are deliberate paraphrases
of corpus content, not keyword copies).

## What this crate is NOT

- **Not a faithful arXiv:2605.23618 reproduction.** The paper uses
  MTEB-shaped curated datasets + multiple downstream tasks; this
  bench uses a 50-doc fixture scored by nDCG@10 with binary
  relevance. The *shape* of the quality-vs-latency tradeoff is
  what carries over.
- **Not a managed-cloud recommendation.** Default builds do not
  require an OpenAI key. mnemo's embedded-first wedge is preserved
  — the recommender will pick a local backend when no remote one
  is configured.
- **Not a change to retrieval defaults.** The default read path,
  RRF weights, and engine wiring are untouched. This bench consumes
  `EmbeddingProvider` impls; it does not modify them.
- **`hashing-baseline` is not a real semantic embedder.** It is a
  feature-hashing-trick character-n-gram bag whose cosine similarity
  reflects lexical overlap, not meaning. It exists only so default
  builds report a non-zero row.
- **Criterion target measures embed latency only.** Quality is a
  separate side computation surfaced via the CLI; criterion's HTML
  reports cover the wall-clock dimension.

## Cross-references

- Embedding trait: [`crates/mnemo-core/src/embedding/mod.rs`](../../crates/mnemo-core/src/embedding/mod.rs)
- OpenAI backend: [`crates/mnemo-core/src/embedding/openai.rs`](../../crates/mnemo-core/src/embedding/openai.rs)
- ONNX backend (feature-gated): [`crates/mnemo-core/src/embedding/onnx.rs`](../../crates/mnemo-core/src/embedding/onnx.rs)
- CLI subcommand: `mnemo bench embeddings --slo-ms <N>` (registered in [`crates/mnemo-cli/src/main.rs`](../../crates/mnemo-cli/src/main.rs))
