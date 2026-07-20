# STATE-Bench × mnemo — results

> **PENDING (no number yet).** The harness is complete and the mnemo half is
> smoke-tested offline, but a *score* requires the protocol-locked **GPT-5.4**
> evaluation client (user simulator + judge) and an **agent model**
> (gpt-5.1-class), neither of which is reachable from the environment this was
> built in. **No number is published until a real, credentialed run completes.**
> This file is filled in by `run_state_bench.sh` once models are available.

## Exactly which configuration will produce the number

| Field | Value |
|---|---|
| Benchmark | Microsoft STATE-Bench, **Agent Learning Track** |
| Pinned commit | `4efcbf2d4fe60df04878859b692d9391f3d5b33a` (v0.8.1) · MIT |
| Domains | travel, customer_support, shopping_assistant (50 held-out test tasks each) |
| mnemo backend | **DuckDB, embedded** (on-prem, no server) |
| Retrieval | `strategy="auto"` — hybrid RRF (semantic + BM25 + recency) |
| Embedder | **OpenAI `text-embedding-3-small`, 1536-dim** (real; never `NoopEmbedding`) |
| Memory hook | `retrieve_learnings(query, top_k=3)` → `MnemoClient.recall` (read-only) |
| Learnings | 1 deterministic procedural note per train trajectory (no LLM extraction in v1) |
| `--num-runs` | 5 (official) · `--retrieve-learnings-top-k` 3 (official) |
| Seeds | ≥ 3 outer seeds; report the across-seed spread of the aggregate |
| Locked eval client | GPT-5.4 (simulator + judge) — protocol-fixed, not a mnemo choice |
| Agent model | reported per run (e.g. `gpt-5.1`); the score is dominated by it |

## Metrics to be recorded (per domain + aggregate)

- Task Completion **pass@1**, **pass^5**, **UX Score** (LLM-judged), **Cost Per Task**
- `n` (tasks × runs), wall-clock, hardware
- The STATE-Bench **GPT-5.1-no-memory baseline**, cited from the leaderboard (not paraphrased)
- Any task category skipped, and why (none planned — all three domains)

## Offline smoke (credential-free) — the mnemo half ✅ passed 2026-07-20

Proves the mnemo ingest + retrieval plumbing end-to-end, independent of the
agent/simulator/judge LLMs. Ran on macOS (aarch64-apple-darwin), Python 3.12, a
`maturin develop` build of the mnemo public SDK (links `mnemo-core`; build 4m06s):

- **`build_learnings`** extracted **100 procedural learnings** (one per
  `customer_support` train trajectory) and `remember`ed them into an embedded
  mnemo DuckDB store.
- **`retrieve_learnings`** (lexical BM25 lane; no embedding key in the smoke) for
  the query *"customer wants to return a blender and get a refund to their card"*
  returned **3/3 relevant hits** — all blender / damaged-item / return-and-refund
  tasks (e.g. `121-hard_shipping_wrong_item…`, `60-challenge_low_value_lost…`,
  `23-hard_customer_says_damaged_but_wrong_item_blender`).

This validates the `MnemoClient.recall` return shape and BM25 retrieval over the
ingested notes. The **semantic** lane (OpenAI `text-embedding-3-small`) is
exercised only in a credentialed run.

## Honesty

Not "state of the art." The score reflects the agent model + mnemo's one
read-only memory hook, not mnemo retrieval quality in isolation (see the LoCoMo /
LongMemEval benches for that). mnemo's differentiator here is being the on-prem /
embedded / auditable entry — evidence for the regulated-AI wedge, not a
repositioning. See [../README.md](../README.md).
