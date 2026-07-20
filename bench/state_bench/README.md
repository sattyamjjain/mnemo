# mnemo on Microsoft STATE-Bench (Agent Learning Track)

> **Status: harness ready — number PENDING model access.** The full integration
> (adapter, offline learnings extraction, run orchestration) is built and the
> mnemo half is smoke-tested offline. Producing a *score* requires hosted models
> this environment does not have (below). We do **not** publish a fake or partial
> number. When GPT-5.4 + an agent model are available, `run_state_bench.sh` is
> turnkey.

## What STATE-Bench is (resolved, not guessed)

- **Repo:** <https://github.com/microsoft/STATE-Bench> · **License:** MIT
- **Pinned commit:** `4efcbf2d4fe60df04878859b692d9391f3d5b33a` (v0.8.1, 2026-07-16)
- **Announcement:** [MS Open Source Blog, 2026-05-19](https://opensource.microsoft.com/blog/2026/05/19/introducing-state-bench-a-benchmark-for-ai-agent-memory/)
  · **Leaderboard:** <https://microsoft.github.io/STATE-Bench/leaderboard/>

STATE-Bench is **not a retrieval test.** It measures whether an *agent* completes
realistic, multi-step **enterprise workflows** across three domains — **travel**,
**customer support**, **shopping assistant** (450 tasks; the Agent Learning Track
uses 100 train trajectories + 50 held-out test tasks per domain). Each task gives
the agent a task-local sandbox DB, domain tools, and a **simulated user**; the
agent must gather info with tools, apply policy, mutate the DB to the correct
final state, and follow the procedure in conversation. Headline metrics:
**Task Completion pass@1** (avg over 5 runs), **pass^5**, **UX Score** (LLM-judged,
1–5), **Cost Per Task**. Published baseline: **GPT-5.1 without memory**
(~50–60% pass@1 across domains — cite the [leaderboard](https://microsoft.github.io/STATE-Bench/leaderboard/)
directly, do not paraphrase).

mnemo's entry is the **Agent Learning Track**: it plugs into a single read-only
hook, `retrieve_learnings(query, top_k=3) -> list[str]`.

## Why a Python driver, not a Rust crate

STATE-Bench is **Python-native** (Python 3.12+, `uv`, a `StateBenchAgent`
subclass discovered by class name, run via `uv run python -m state_bench.scripts.run_batch`).
A Rust crate mirroring `bench/audit_conformance/` would have to reimplement or
shell out to the entire Python harness (simulator, judge, tools, scoring) — far
more glue. mnemo already ships a **public Python SDK** (`python/`, PyO3
`MnemoClient` exposing `remember`/`recall`/`forget`), so the whole integration is
one small adapter using the **public API only** — **no `mnemo-core` change**.

## The integration (this directory)

| File | Role |
|---|---|
| [`agents/mnemo_memory_agent.py`](agents/mnemo_memory_agent.py) | `MnemoMemoryAgent(StateBenchAgent)` — `retrieve_learnings` = one `MnemoClient.recall`; static `build_learnings` extracts one deterministic procedural learning per train trajectory and `remember`s it into an embedded mnemo DuckDB store. |
| [`build_learnings.py`](build_learnings.py) | CLI wrapper for offline extraction (credential-free). |
| [`run_state_bench.sh`](run_state_bench.sh) | End-to-end orchestration: checkout @ pinned SHA → `uv sync` + build the mnemo SDK into that venv → copy adapter → per domain × seed: build learnings → `run_batch` → `compute_metrics`. Fails loud without the locked GPT-5.4 client. `--build-only` runs the credential-free extraction smoke. |
| `results/` | Scored trajectories + metrics land here; `results/state_bench.md` is the honest report (currently PENDING). |

**Backend / config for the number:** embedded **DuckDB** (on-prem, no server);
retrieval `strategy="auto"` (hybrid RRF: semantic + BM25 + recency); **real
embedder = OpenAI `text-embedding-3-small`** (1536-dim) via
`MNEMO_STATEBENCH_EMBED_KEY`/`OPENAI_API_KEY`. **Never `NoopEmbedding` semantics**
— mnemo's post-0.5.13 recall hard-errors under a no-op embedder by design, so
without an embedding key the adapter falls back to the **lexical (BM25)** lane and
says so loudly (offline-verifiable, but not the semantic number).

## Why it can't run *here* yet

STATE-Bench makes LLM calls in **three** places (`docs/AGENT_LEARNING_TRACK.md`):
the **locked GPT-5.4 user simulator**, the **locked GPT-5.4 judge** ("Every
official run requires the protocol-locked GPT-5.4 evaluation client"), and the
**agent under test** (gpt-5.1 / claude-sonnet-4.5-class). Verified in this
environment (2026-07-20): `OPENAI_API_KEY`, `AZURE_OPENAI_*`, `ANTHROPIC_API_KEY`
all unset; the only local model is `nomic-embed-text` (an *embedder*, which
cannot act as agent, simulator, or judge). There is no offline/mock mode. So no
honest number can be produced here — hence PENDING, not a partial run.

## How to run (when GPT-5.4 + an agent model are available)

Set the STATE-Bench clients (see the repo's `.env.example` / `docs/setup/eval-client.md`):

```bash
export STATE_BENCH_EVAL_ENDPOINT="https://<gpt54-resource>.openai.azure.com"
export STATE_BENCH_EVAL_DEPLOYMENTS="<gpt-5.4 deployment>"
export STATE_BENCH_EVAL_API_KEY="<key>"           # or Azure token auth
# agent under test (Azure or OpenAI):
export STATE_BENCH_AGENT_PROVIDER="openai"
export STATE_BENCH_AGENT_MODEL="gpt-5.1"
export STATE_BENCH_AGENT_API_KEY="<key>"
# mnemo real embedder:
export MNEMO_STATEBENCH_EMBED_KEY="<openai key>"  # text-embedding-3-small

bash bench/state_bench/run_state_bench.sh          # SEEDS=3 NUM_RUNS=5 TOP_K=3
```

Then summarise per-domain **pass@1 / pass^5 / UX / cost**, the aggregate, `n`,
wall-clock, hardware, the exact mnemo config, and the **across-seed spread** into
`results/state_bench.md`, next to the cited GPT-5.1-no-memory baseline. Publish
any category where mnemo underperforms. **Never write "state of the art."**

## Honest framing (read this before quoting any future number)

A STATE-Bench score is **dominated by the agent model** (GPT-5.4/5.1) and the
locked simulator/judge; mnemo contributes exactly **one read-only
`retrieve_learnings` hook**. So a "mnemo STATE-Bench number" is a *"agent-model +
mnemo memory-hook delta"* — it measures whether learnings mnemo surfaces from
past trajectories improve task completion, **not** mnemo's retrieval quality in
isolation (that is what mnemo's [LoCoMo / LongMemEval retrieval benches](../RESULTS.md)
report). This is a worthwhile entry — it is the **on-prem / embedded / auditable**
memory shape nobody has posted — and it is evidence *for* mnemo's regulated-AI
wedge (the same store carries the hash-chained, tamper-evident audit log), **not**
a repositioning. The learnings extraction here is deterministic (no LLM); an
LLM-distilled extraction is a documented future improvement that would likely
raise the number.

## Regression gate

`.github/scripts/check_bench_regression.py` is dataset-scoped
(`--dataset ∈ {locomo, longmemeval}`, comparing `recall@10`) — a retrieval-
regression check, out of scope for STATE-Bench (agentic task completion) by
construction. No Rust crate was added to the workspace `Cargo.toml` members
(this is a Python driver, not a crate).
