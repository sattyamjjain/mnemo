# `mnemo-locomo-bench`

Authenticated nightly benchmark crate for `mnemo`. Five bins today:

| Bin | What it measures | Authentication |
|---|---|---|
| `mnemo-locomo` | Full LoCoMo dialogue-grounded recall (overall + temporal + multi-session + open-domain) with cross-judge variance bands (GPT-5.1 + Claude-3.7-Sonnet) | Gated dataset + judge API keys (see `.github/workflows/locomo-nightly.yml`); falls back to `MockJudge` for deterministic local runs |
| `grep_vs_vector_replay` | Three-mode (`vector_only` / `bm25_only` / `rrf_hybrid`) recall replay against a LongMemEval-shaped slice, exact-substring smoke metric | Runnable today on the bundled 45-record synthesized slice with no API key; gated GPT-judge-scored run requires the same secrets as [#44](https://github.com/sattyamjjain/mnemo/issues/44) |
| `interference` | **v0.4.7** — MINTEval-shaped interference scenario (arXiv:2605.18565). Revises a target fact K∈{1,3,5,10} times, queries via the v0.4.7 current-fact resolver, reports current-fact-accuracy@K + supersession-chain length per K. Default vs resolver arms. | Runnable today on a synthetic distractor pool; the official MINTEval GPT-judge scoring is gated behind [#44](https://github.com/sattyamjjain/mnemo/issues/44). See [`src/bin/interference.rs`](src/bin/interference.rs). |
| `orientation` | **v0.4.8** — PEEK-shaped repeated-context scenario (arXiv:2605.19932). Issues K∈{3,6,10,15} related recall calls per trial against a shared synthetic context, compares hybrid-only vs orientation-cache token cost, asserts top-1 parity. Heuristic Distiller, NoopEmbedding by design. | Runnable today on a synthetic context pool with no API key; calibrating per-call token cost against `tiktoken-rs` is a follow-up. See [`src/bin/orientation.rs`](src/bin/orientation.rs). |
| `auto_dreamer_consolidation` | **v0.4.8** — Auto-Dreamer-style offline consolidation scenario. Ingests a multi-session trajectory, runs `run_decay_pass` + `run_consolidation` offline, reports `active_bank_ratio` and held-out `recall_pre` / `recall_post`. Asserts Auto-Dreamer's "smaller active bank, equal-or-better recall" axis. | Runnable today on a synthetic multi-session pool with no API key; LLM-summarizer arm is gated behind [#44](https://github.com/sattyamjjain/mnemo/issues/44). See [`src/bin/auto_dreamer_consolidation.rs`](src/bin/auto_dreamer_consolidation.rs). |

## `mnemo-locomo` (v0.4.1 P0-1)

Wired into `.github/workflows/locomo-nightly.yml`. Reads the gated
dataset from `MNEMO_LOCOMO_DATASET_PATH`, runs each dialogue through
the engine in the chosen mode, asks the configured judge(s), and
emits both a JSONL trace and a Markdown report at
`docs/benchmarks/locomo-<date>.md`.

```bash
MNEMO_LOCOMO_DATASET_PATH=/path/to/locomo \
  cargo run --release --bin mnemo-locomo -p mnemo-locomo-bench -- \
  --mode default --judge mock --out-dir docs/benchmarks
```

## LongMemEval replay (arXiv:2605.15184) — `grep_vs_vector_replay`

Added 2026-05-17 in response to the Sen et al. arXiv:2605.15184
"grep vs vector retrieval inside agent harnesses" experiment design.
The bin routes each LongMemEval-shaped question through
`mnemo.recall` three times (BM25-only, vector-only, RRF-hybrid),
measures smoke accuracy (exact-substring match against the `expected`
field), captures p50/p95 latency per mode, and writes a Markdown
table to `bench/locomo/results/grep_vs_vector_<YYYY-MM-DD>.md`.

### What this bin does NOT do

Documented in detail in the bin's module-level rustdoc; the
short version:

1. **Not the official LongMemEval metric.** Smoke metric is exact-substring;
   official GPT-judge scoring needs API keys (gated behind #44).
2. **Not a real vector run.** Scaffold uses `NoopEmbedding` (zero vectors)
   so the vector-only column is degenerate. Swap to `OnnxEmbedding`
   or `OpenAiEmbedding` for the gated run.
3. **Not the published LongMemEval dataset.** Default is the bundled
   45-record synthesized slice at
   `crates/mnemo-core/benches/data/longmemeval_m.jsonl`. Override
   via `MNEMO_LONGMEMEVAL_PATH` for the real 116-question slice.
4. **Not a perf number comparable to the paper.** The bin's purpose
   is wiring + scaffold for the gated run; absolute numbers from the
   smoke run are not directly comparable to the paper's published
   results.

### Smoke run

```bash
cargo run --release --bin grep_vs_vector_replay -p mnemo-locomo-bench
# writes: bench/locomo/results/grep_vs_vector_<YYYY-MM-DD>.md
```

### Gated full run (requires #44 secrets)

```bash
MNEMO_LONGMEMEVAL_PATH=/path/to/longmemeval_s.jsonl \
OPENAI_API_KEY=sk-... \
  cargo run --release --bin grep_vs_vector_replay -p mnemo-locomo-bench
# (real GPT-judge scoring is a v0.4.4+ follow-up; today's bin emits
#  the smoke metric regardless of whether the API keys are set)
```

### Modes covered

| CLI label | `RecallRequest.strategy` value | What it does |
|---|---|---|
| `vector_only` | `"semantic"` | USearch HNSW vector search only; no BM25, no graph, no decay |
| `bm25_only`   | `"lexical"` | Tantivy BM25 keyword search only |
| `rrf_hybrid`  | `"auto"`    | mnemo's default RRF fusion: vector + BM25 + recency + decay |

The fourth existing strategy `"graph"` is intentionally omitted from
this bin — it requires a relation graph the LongMemEval-shaped data
does not carry. A graph-aware comparison is a follow-up.

## Auto-Dreamer offline consolidation — `auto_dreamer_consolidation`

Added 2026-05-25. Mirrors Auto-Dreamer's "smaller active bank,
equal-or-better recall" axis against mnemo's existing
`run_decay_pass` + `run_consolidation` path
([`crates/mnemo-core/src/query/lifecycle.rs`](../../crates/mnemo-core/src/query/lifecycle.rs)).
The bin seeds `S` sessions × `F` facts each with one needle per
session, backdates older sessions so the offline decay pass marks
them `Archived` / `Forgotten` deterministically, snapshots the
active bank + held-out recall pre, runs the offline pass, and
re-snapshots post. Emits a Markdown report
(`bench/locomo/results/auto_dreamer_<YYYY-MM-DD>.md`) plus a JSON
summary (`auto_dreamer_<YYYY-MM-DD>.json`) carrying
`active_bank_ratio`, `recall_pre`, `recall_post` so the headline is
citable in the README.

### Run

```bash
cargo run --release --bin auto_dreamer_consolidation -p mnemo-locomo-bench
# defaults: 8 sessions × 25 facts × 5 trials,
# archive_threshold=0.40, forget_threshold=0.10, min_cluster_size=3.
# Tunable: --sessions, --facts-per-session, --trials,
# --archive-threshold, --forget-threshold, --min-cluster-size.
```

### What this bin does NOT do

Documented in detail in the bin's module-level rustdoc; the short
version: not a faithful Auto-Dreamer reproduction (mnemo's
`run_consolidation` clusters by tag overlap, not an LLM
summarizer); not the `criterion` crate (the same structured-report
pattern the other `bench/locomo` bins use); not a real embedder
run (`NoopEmbedding` so the wiring is self-contained);
single-agent, single-scope.

## Cross-references

- bin module rustdoc: [`src/bin/grep_vs_vector_replay.rs`](src/bin/grep_vs_vector_replay.rs)
- existing criterion bench (latency only): [`crates/mnemo-core/benches/longmemeval_bench.rs`](../../crates/mnemo-core/benches/longmemeval_bench.rs)
- gated-secrets backlog: [#44](https://github.com/sattyamjjain/mnemo/issues/44)
- arXiv:2605.15184 reference: [`docs/research/grep-vs-vector-2605.15184.md`](../../docs/research/grep-vs-vector-2605.15184.md) — (TBA in companion docs PR)
- reflection / consolidation modules: [`crates/mnemo-core/src/query/reflection.rs`](../../crates/mnemo-core/src/query/reflection.rs), [`crates/mnemo-core/src/query/lifecycle.rs`](../../crates/mnemo-core/src/query/lifecycle.rs)
