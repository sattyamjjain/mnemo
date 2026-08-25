# Mnemo

[![CI](https://github.com/sattyamjjain/mnemo/actions/workflows/ci.yml/badge.svg)](https://github.com/sattyamjjain/mnemo/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![Docs](https://img.shields.io/badge/docs-sattyamjjain.github.io-blue.svg)](https://sattyamjjain.github.io/mnemo/)

**On-prem, MCP-native, cryptographically-auditable memory for regulated AI** (EU AI Act Art.12 · India DPDP · HIPAA §164.312(b)).

📖 **Documentation:** <https://sattyamjjain.github.io/mnemo/> — the full mdBook (quickstart, architecture, MCP tool reference, REST/SDK guides, compliance docs), deployed from [`docs/`](docs/) on every push to `main`.

Mnemo (from Greek *mneme* — memory) is an **embedded** database (DuckDB in-process, or your own PostgreSQL) whose primitives — **REMEMBER**, **RECALL**, **FORGET**, **SHARE** — are exposed as [MCP](https://modelcontextprotocol.io/) tools any AI agent connects to directly. What sets it apart for regulated deployments: every write and delete is a **SHA-256 hash-chained `agent_events` entry an external verifier can check offline** (no store, no hosted tier to trust), and [`mnemo-compliance`](crates/mnemo-compliance) layers signed audit-log export + DPDPA consent records on top.

**→ [Positioning: on-prem, MCP-native, cryptographically-auditable memory for regulated AI](docs/POSITIONING.md)** — how mnemo compares to Mem0, Letta, and native provider memory on the compliance-audit axis, wired to the shipped bench numbers.

## On-prem, MCP-native, cryptographically-auditable memory for regulated AI

Mnemo runs **in-process on your own infrastructure** (embedded DuckDB or your
PostgreSQL) with **no hosted tier to trust** — and its memory-write log is
**tamper-evident**: every write and delete is a SHA-256 hash-chained
`agent_events` entry, and an **external verifier can detect any post-hoc
mutation offline**, without consulting the store. That is the record-keeping
substrate regulated AI deployments need (EU AI Act Art.12 / Art.26(6), India
DPDPA, HIPAA §164.312(b) audit controls).

This is **proven, not asserted** — a deterministic, offline bench writes memories
through the real path, then shows an outside verifier accepts the pristine log,
catches 100% of single-byte mutations (256 trials, Wilson 95% ≥ 98.5%), and
confirms `forget` appends a signed delete event rather than erasing the trail:

```bash
cargo run --release -p mnemo-audit-conformance-bench
# → bench/audit_conformance/results/conformance.md  (byte-stable report + recomputable SHA-256 crypto vector)
```

**Reproducible-by-disclosure memory:** mnemo publishes its LoCoMo numbers with a
fixed seed + a Wilson-95 you can re-run offline (`cargo run --release -p
mnemo-locomo-bench --bin reproduction_bench`), tabled against vendors' cited,
not-re-run claims — see
[`bench/locomo/results/reproduction_2026-07-06.md`](bench/locomo/results/reproduction_2026-07-06.md).

**Real-embedder retrieval quality (measured, not asserted):** the byte-reproducible
number above runs under a *deterministic hash-bag* embedder, which is a lexical
**floor**, not a retrieval result.

<!-- BEGIN generated: recall-headline -->
<!-- Generated from bench/results/locomo_v1.json by scripts/gen_recall_number.py — do not hand-edit. -->

The one headline real-embedder number is the generated block below: **Xenova/all-MiniLM-L6-v2 384-dim, n=45, recall@1 0.689** [0.543, 0.805], against a lexical control on the same corpus and harness. On the same 45 queries the paired gap over that control is **+0.267** [95% 0.133, 0.400], McNemar exact p=4.9e-4 — it separates.
<!-- END generated: recall-headline -->

There is deliberately only one headline, and an older measurement is
kept below it under its own heading rather than beside it.

Every published number, this one and the poisoning / audit / retrieval numbers
further down, is collected in one place with its exact reproduction command, its
raw results file, and a "what this does **not** show" column:
**[`docs/benchmarks/index.md`](docs/benchmarks/index.md)** (the single benchmark
entry point). The mirror of that page, things a reader might reasonably assume
mnemo measures and it does not, is
**[`docs/security/known-limitations.md`](docs/security/known-limitations.md)**. Full per-mode tables and the different-axes caveat vs. Mem0/Letta
are in the benchmark tables below and [`bench/RESULTS.md`](bench/RESULTS.md).

<!-- BEGIN generated: recall-number -->
<!-- Generated from bench/results/locomo_v1.json by scripts/gen_recall_number.py — do not hand-edit. -->

**Real-embedder recall, measured on the supported backend.** Gold-document **recall@1 = 0.689** [Wilson 95% 0.543, 0.805], recall@5 0.889, recall@10 0.911, MRR 0.770. Against the lexical control **on the same 45 queries** the paired gap is **+0.267** [95% 0.133, 0.400], McNemar exact p=4.9e-4 (12 queries won, 0 lost) — the gap separates at 95%.

| | |
|---|---|
| embedder | `Xenova/all-MiniLM-L6-v2 (onnx/model.onnx)` (384-dim, backend `onnx`) |
| weights sha256 | `759c3cd2b7fe…` |
| weights source | https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx |
| storage backend | `duckdb (in-memory)` |
| corpus | `crates/mnemo-core/benches/data/longmemeval_m.jsonl`, n=45 queries, mean of 5 seeds |
| **control (lexical)** | recall@1 0.422 [0.290, 0.567] |
| **paired gap vs control** | +0.267 [0.133, 0.400], McNemar b=12/c=0, exact p=4.9e-4 |
| hardware | arm64/darwin Apple M4 |
| measured | 2026-08-24 at `3b876fc` |

The lexical row is the control, not a second headline: it is the same corpus and the same harness with the vector lane switched off, so what the embedder buys is the difference between them. **Do not read that difference off the two intervals** — they overlap (0.543 sits below 0.567), and overlapping intervals neither establish nor rule out a difference. The paired row is the one that answers it: the same queries scored both ways, so each query is its own control.

Reproduce:

```bash
# fetch the exact weights the sha256 above pins
curl -sSL --fail -o model.onnx https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx
MNEMO_ONNX_MODEL_PATH=./model.onnx cargo run --release --features onnx \
  -p mnemo-locomo-bench --bin locomo_v1_bench
```

**What this number is not.** It is not a LoCoMo leaderboard score and is not comparable to one: the corpus is the bundled LongMemEval_M slice, not full LoCoMo. It is retrieval quality only, with no LLM in the loop and no answer-correctness judge. It says nothing about poisoning resistance or audit integrity, which are measured separately. **n=45 is below 100, so the bench marks it `preliminary`;** treat the interval, not the point estimate, as the claim.
<!-- END generated: recall-number -->

This block is generated from [`bench/results/locomo_v1.json`](bench/results/locomo_v1.json) by
[`scripts/gen_recall_number.py`](scripts/gen_recall_number.py); the `doc-guards` CI job runs it
with `--check`, so the README cannot drift from the measured result. Making it
CI-*reproducible* (a model-fetch bench job) remains
[#125](https://github.com/sattyamjjain/mnemo/issues/125).

<details>
<summary><strong>Earlier measurement, different embedder and corpus</strong> (nomic-embed-text 768-dim, n=23)</summary>

An earlier real-embedder run, kept for provenance and **not** a second headline.
It used a **different embedder on a different slice**, so it is not comparable to
the number above and the two must never be quoted together as a range.

| | |
|---|---|
| embedder | `nomic-embed-text` (768-dim, local via Ollama, no API key) |
| corpus | bundled LongMemEval_M held-out slice, **n=23**, mean of 5 seeds |
| recall@1 | **0.739** [Wilson 95% 0.535, 0.875] |
| recall@5 | 0.826 [0.629, 0.930] |
| MRR | 0.805 |
| mode | `vector_only`, the one stable strong mode |
| first measured | 2026-06-29 @ `640b7b1`; raw data in [`docs/benchmarks/baseline.json`](docs/benchmarks/baseline.json) |

**The interval is the point.** At n=23 the Wilson 95% interval is
**[0.535, 0.875]**, which is wider than the headline's and *fully overlaps* it
([0.543, 0.805]). That overlap is **not** a finding of equivalence — the same
arithmetic that cannot prove a difference cannot prove its absence either. What
can be said is narrower and worth saying plainly: these are two separate runs on
different corpora, so unlike the semantic-vs-lexical comparison above **there is
no paired statistic available here at all**, and at n=23 the interval is far too
wide to resolve a gap of this size in either direction. The higher point estimate
is therefore not evidence that this embedder is better. Neither run clears the
repo's own n≥100 bar, and both are marked preliminary for that reason.

Reproduce with no credentials and no special build feature:

```bash
ollama pull nomic-embed-text && cargo run --release -p mnemo-locomo-bench --bin semantic_recall_bench
```

The runner **refuses to emit a score under a no-op embedder**.

</details>

And the **memory-poisoning defense is measured on a real embedder, not asserted.**
Through a real ONNX MiniLM embedder (not a hash stand-in), mnemo's always-on
lexical / self-referential lane drops canonical **MINJA** ASR **100% → 0%** at
**0/300 benign false-quarantine**. Honestly, the opt-in *embedding z-score* lane
does **not** generalise to a dense embedder: on MiniLM, poison lands ~1.5σ from
the benign mean (below the 3σ gate), so marker-stripped and consolidation-style
redirects survive (ASR 100%) — a limitation we **publish rather than hide**,
correcting the hash-embedder bench's rosier z-score reading. ASR + Wilson-95 +
benign-FPR per attack, refuse-to-score-on-noop:
[`docs/BENCH_POISONING.md`](docs/BENCH_POISONING.md)

**Non-adaptive Phase-3 exploitation, against a digest-pinned real embedder**
([#37](https://github.com/sattyamjjain/mnemo/issues/37)): a poisoned record that
is already Phase-2-shortened is exploited at **0.956** [Wilson 95% 0.906, 0.980],
**129/135** trials (45 distinct victim queries × 3 seeds; conservative interval at
the distinct-query denominator **[0.852, 0.988]**, n=45). With the detector on:
**0.956** [0.906, 0.980], 129/135 — **defense delta 0.0000**, and **0/135**
records quarantined at a mean z of **1.08** against the 3.0 gate. The matched
benign twin is retrieved at **0.956** [0.906, 0.980], 129/135, so the
**poisoning delta is +0.0000**: the rate is topical retrieval, not poisoning.
**This is not a MINJA number** — Phases 1 and 2 are generative and adaptive and
are not implemented, and it says nothing about an adaptive attacker.
[`docs/benchmarks/2026-08-25-minja-phase3-nonadaptive.md`](docs/benchmarks/2026-08-25-minja-phase3-nonadaptive.md)
· [`bench/results/minja_phase3.json`](bench/results/minja_phase3.json)
(`cargo run --release --features onnx -p mnemo-poisoning-bench --bin poisoning_real_bench`).
This bench is **also `--features onnx`**, so — like the ONNX MiniLM retrieval number
above — it is **not currently CI-reproducible** until the `ort` integration is repaired
([#125](https://github.com/sattyamjjain/mnemo/issues/125)). The byte-stable
hash-embedder defense-*delta* companion runs in CI and is at
[`bench/poisoning/`](bench/poisoning/).

Quarantine is only half the story — the other half is the **auditable layer**,
mnemo's real wedge for [OWASP **ASI06 — Memory & Context Poisoning**](https://genai.owasp.org/2025/12/09/owasp-top-10-for-agentic-applications-the-benchmark-for-agentic-security-in-the-age-of-autonomous-ai/)
(whose recommended control is *provenance on every write + evaluation against
ground truth*). Against three ASI06 attack families — contradictory-fact
overwrite, authority-spoofed origin + provenance forgery, belief-drift
splice/back-date — the SHA-256 hash-chain + read-provenance HMAC layer **rejects
100% of poisoning cover-up/forgery attempts (1500/1500, Wilson 95% [99.7%,
100.0%])** at **0% benign false-positive** (0/300, [0.0%, 1.3%]); a naive store
with no crypto layer catches **0%**. Read honestly: this is **tamper-evidence +
attribution — poisoning cannot be *hidden*** — not write-time prevention. Method,
numbers, and the LoCoMo ground-truth-quality caveat:
[`docs/benchmarks/asi06-poisoning.md`](docs/benchmarks/asi06-poisoning.md)
(`cargo run --release -p mnemo-asi06-poisoning-bench --bin asi06_poisoning`).

That ASI06-recommended control — **provenance on every write** — is now the
default path: every `remember`/`share` records who wrote it, under what
capability, in what session, hash-chained and revocable by principal (see
[Write provenance & FORGET BY PROVENANCE](#write-provenance--forget-by-provenance)).
The **compositional ("Salami") case** — the harder shape where each of `N` writes
is individually benign but their union is harmful ([arXiv:2608.01637](https://arxiv.org/abs/2608.01637))
— is measured, not defended, at [`bench/salami_poisoning/`](bench/salami_poisoning/):
individually-benign slices sail through the write path (high **save rate**) and a
single trigger recall reliably reconstructs the harmful composition (high
**retrieval-influence rate**), while a topic-matched **benign control** co-retrieves
yet never completes the harm (~0). Both rates carry a Wilson-95 interval;
deterministic + offline. This is the compositional subset of
[#37](https://github.com/sattyamjjain/mnemo/issues/37) — a quantified gap, not a
claimed defense (`cargo run --release -p mnemo-salami-poisoning-bench`).

Regulatory mappings (honest, hedged, *not legal advice*):
[EU AI Act Art.12](docs/compliance/eu-ai-act-art12.md) ·
[India DPDP (Rules 2025)](docs/compliance/dpdp-2027.md) ·
[ASI06 memory-poisoning resistance](docs/security/ASI06.md). Apache-2.0, no SaaS.

## Install from crates.io

The 0.5.x **compliance line** is on crates.io — the minimal set to run
on-prem, hash-chain-audited memory in your own Rust service:

```bash
cargo add mnemo-core mnemo-compliance   # engine + audit-log/consent primitives
cargo add mnemo-mcp                      # (optional) expose it as MCP tools
cargo install mnemo-mcp-server          # server binary → `mnemo`
```

> **Current release: `0.5.27`, cut but not yet published.** The commands above
> resolve **`v0.5.26`**, which is what every published `mnemo-*` crate is on today,
> `mnemo-mcp-server` included. The generated table below shows published crates one
> patch behind the workspace; that is the open release window, not drift, and that
> table is the authority on what is installable right now.
>
> The seven satellite crates that trailed a patch during the `v0.5.25` window have
> caught up, and they are in the tag walk as of `v0.5.26`, so the split that caused it
> cannot recur: see [Naming](#naming).
>
> Three guards keep this honest instead of stale: the test
> [`crates/mnemo-cli/tests/readme_crates_version_matches_workspace.rs`](crates/mnemo-cli/tests/readme_crates_version_matches_workspace.rs)
> fails if the stated release, or any bare current-band `0.5.2x` release literal
> anywhere in this README (the `v`-prefixed feature history is exempt), drifts from
> the workspace `[workspace.package].version`;
> [`scripts/check_version_drift.sh`](scripts/check_version_drift.sh) fails if
> crates.io falls more than one patch behind the workspace; and
> [`scripts/check_publish_closure.sh`](scripts/check_publish_closure.sh) fails if a
> publishable crate is missing from the release closure, which is what let a crate
> fall behind silently in the first place.

<!-- BEGIN generated: published-versions -->
<!-- Regenerate with: python3 scripts/gen_published_versions.py -->

Workspace `[workspace.package].version` (unreleased target): **`v0.5.27`**. The Rust library line and the Python SDK both track the workspace (the wheel compiles `mnemo-core` into itself, so its version names the engine inside it). Only the TypeScript SDK versions independently. Published, per registry:

| Registry | Artifact | Published version | Published |
|---|---|---|---|
| crates.io | `mnemo-core` — engine + hash-chain verify | `v0.5.26` | 2026-08-22 |
| crates.io | `mnemo-mcp-server` — the `mnemo` server binary | `v0.5.26` | 2026-08-22 |
| crates.io | `mnemo-embeddings-bench` — bench crate the server binary depends on | `v0.5.26` | 2026-08-22 |
| PyPI | `mnemo-db` — Python SDK (tracks the workspace) | `v0.5.27` | 2026-08-25 |
| npm | `@mndfreek/mnemo-sdk` — TypeScript SDK (independent) | `v0.4.4` | 2026-05-18 |

_Table generated from the live registries by [`scripts/gen_published_versions.py`](scripts/gen_published_versions.py); `scripts/registry_parity.sh` fails a release if these drift from what the release actually published._
<!-- END generated: published-versions -->

### Naming

This project does not own the `mnemo` name on crates.io, and does not publish a
crate called `mnemo`. Everything it publishes carries the `mnemo-*` prefix.

Two unrelated crates sit on names you might otherwise reach for:

| crates.io name | Who owns it | What it is |
|---|---|---|
| [`mnemo`](https://crates.io/crates/mnemo) | [aayushadhikari7/mnemo](https://github.com/aayushadhikari7/mnemo) | "A personal knowledge vault for your terminal" — not this project |
| [`mnemo-cli`](https://crates.io/crates/mnemo-cli) | [watzon/mnemo](https://github.com/watzon/mnemo) | "CLI management tool for the Mnemo LLM memory proxy" — not this project |

`mnemo-cli` is the easier mistake: this repo's server binary lives in the
directory `crates/mnemo-cli`, but it **publishes as `mnemo-mcp-server`**. The
directory name is not the crate name.

The server binary is:

```bash
cargo install mnemo-mcp-server   # installs the `mnemo` executable
```

The `mnemo` **command** this installs is ours; the `mnemo` **crate** is not.
[`mnemo-db`](https://crates.io/crates/mnemo-db) on crates.io is a defensive
name-reservation pointer that ships no code and redirects here — distinct from
the PyPI `mnemo-db` package, which *is* the real Python SDK (see
[Python](#python) below).

`scripts/check_crate_name_refs.sh` fails CI if a bare `cargo install mnemo` or
`cargo add mnemo` (or the `mnemo-cli` equivalents) reappears anywhere in the
docs.

#### All published `mnemo-*` crates are at one version

Installing the right *name* is only half of it: `cargo install` resolves whatever
crates.io actually has. As of **2026-08-19** that is one number for every crate.

| what | crates.io | workspace | gap |
|---|---|---|---|
| all **21** published `mnemo-*` crates, including `mnemo-core`, `mnemo-mcp` and **`mnemo-mcp-server`** | `v0.5.26` | `v0.5.27` (unreleased) | one patch, the open release window |

The 21 are `mnemo-core`, `mnemo-mcp`, `mnemo-mcp-server`, `mnemo-db`,
`mnemo-embeddings-bench`, `mnemo-attention-state`, `mnemo-compliance`,
`mnemo-md-sync`, `mnemo-pgwire`, `mnemo-rest`, `mnemo-grpc`, `mnemo-postgres`,
`mnemo-admin`, `mnemo-graph`, `mnemo-letta`, `mnemo-codemode`, `mnemo-mesh`,
`mnemo-baseline`, `mnemo-deal`, `mnemo-cma` and `mnemo-amp`. One of them,
**`mnemo-db`, ships no code**: it is a defensive name reservation whose entire
contents are a doc comment pointing at `mnemo-core` and `mnemo-mcp`. It is
counted here because it is a real published artifact someone can `cargo add`,
and they should learn that from the count rather than from an empty crate.

Other `mnemo*` names on crates.io (`mnemo`, `mnemo-cli`, `mnemo-engine`,
`mnemo-rs`, `mnemo-server`) belong to **unrelated projects**. See
[Naming](#naming).

This was not true a day earlier, and the history is worth keeping because two
guards were changed on account of it.

**`mnemo-mcp-server` had stranded at `v0.5.23`**, skipping `v0.5.24` entirely,
because the tag lane that publishes it could not resolve `mnemo-admin`: the
publish closure was written down three times and the copies drifted apart. There
is now one `WALK` definition that the gate, the packaging dry-run and the publish
loop all expand. Its version history on crates.io still shows the gap:
`v0.5.25`, `v0.5.23`, `v0.4.4`.

**Seven satellite crates** (`mnemo-letta`, `mnemo-mesh`, `mnemo-codemode`,
`mnemo-deal`, `mnemo-md-sync`, `mnemo-cma`, `mnemo-baseline`) then sat a patch
behind, because they were not in the tag walk at all and published only on the
push-to-main lane, which had stopped partway through the release. They have since
caught up, but "it resolved itself" is not a fix: two lanes with different
contents is the condition that lets a crate fall behind with **nothing going
red**, which is the confusion the drift guard exists to prevent rather than
describe.

So the split is closed rather than waited out. All seven are in `WALK` as of
`v0.5.26` (they depend on `mnemo-core` alone and nothing depends on them, so
folding them in reorders nothing), and
[`scripts/check_publish_closure.sh`](scripts/check_publish_closure.sh) now
asserts the general form in CI: every publishable workspace member must appear in
the closure or carry a written exemption. A new crate cannot be orphaned by being
left out of a list.

[`scripts/check_version_drift.sh`](scripts/check_version_drift.sh) treats one
patch behind an unreleased workspace as a publish in flight: green, but it
*names* the crates rather than reporting "every published crate matches
workspace", which it used to say while seven of them did not.

The summary table above is written by hand and is a narrative of one moment. The
authoritative, per-registry numbers are in
the generated table under [Install from crates.io](#install-from-cratesio), which *is*
generated from the live registries by
[`scripts/gen_published_versions.py`](scripts/gen_published_versions.py) and
re-checked by the `doc-guards` CI job, so a stale table there fails the build
rather than quietly misinforming a reader.

The wedge — a memory-write log an auditor can verify **offline, without trusting
the store** — is the [`mnemo-core`](https://crates.io/crates/mnemo-core)
hash-chain verify API:

```rust
use mnemo_core::hash::{verify_chain, ChainVerificationResult};

// `records` is the exported, hash-chained write log. The store is never
// consulted — the verifier is a pure function an auditor runs on their machine.
let result: ChainVerificationResult = verify_chain(&records);
assert!(result.valid);          // false + `first_broken_at` names any post-hoc mutation
```

Signed NDJSON export and DPDPA consent records live in
[`mnemo-compliance`](https://crates.io/crates/mnemo-compliance)
(`export_audit_log`, `verify_ndjson_signed`). Why this matters, and how mnemo
compares to Mem0 / Letta / native provider memory on the compliance-audit axis:
**[docs/POSITIONING.md](docs/POSITIONING.md)**.

## Quickstart

### 1. Build

```bash
cargo build --release
```

### 2. Configure your AI agent

Add to your MCP client configuration (e.g. Claude Desktop, Cursor, etc.):

```json
{
  "mcpServers": {
    "mnemo": {
      "command": "./target/release/mnemo",
      "args": ["--db-path", "./agent.mnemo.db"],
      "env": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }
}
```

### 3. Use it

Your AI agent now has persistent memory with 21 MCP tools:

| Tool | Description |
|------|-------------|
| `mnemo.remember` | Store a new memory with semantic embeddings |
| `mnemo.recall` | Search memories by semantic similarity, keywords, or hybrid |
| `mnemo.forget` | Delete memories (soft delete, hard delete, decay, consolidate, archive) |
| `mnemo.forget_subject` | GDPR / DPDPA subject erasure: redact (default, preserves the hash chain) or hard-delete every memory tagged `subject:<id>` |
| `mnemo.share` | Share a memory with another agent |
| `mnemo.checkpoint` | Snapshot the current agent memory state |
| `mnemo.branch` | Create a branch from a checkpoint for experimentation |
| `mnemo.merge` | Merge a branch back into the main state |
| `mnemo.replay` | Replay events from a checkpoint |
| `mnemo.delegate` | Delegate scoped, time-bounded permissions to another agent |
| `mnemo.verify` | Verify SHA-256 hash chain integrity |
| `mnemo.trajectory_audit` | GEM-aligned trajectory-correctness audit ([arXiv:2605.26252](https://arxiv.org/abs/2605.26252)): unregulated growth, missing semantic revision, capacity-driven forgetting, read-only retrieval |
| `mnemo.consolidate` | v0.5.0 — Group related memories into one revisable **topic document** (Infini-Memory): collects members as evidence, preserves provenance, records a hash-chained audit event; pass `supersede` to revise a fact while keeping the old version in history |
| `mnemo.remember_plan` | v0.4.14 — Cache a *successful* retrieval/reasoning plan (query signature + steps + chunk ids + outcome score) into the experience-memory tier (DocTrace; only when the mode is enabled) |
| `mnemo.recall_plan` | v0.4.14 — Replay the best cached plan for a structurally-similar query instead of re-running full retrieval; RBAC-gated, returns a miss when nothing matches |
| `mnemo.attention_state.put` | v0.4.5 — Store an opaque attention-state blob keyed by `(agent_id, prefix_hash)` (anchored on [arXiv:2605.18226](https://arxiv.org/abs/2605.18226); only registered when the server is built with `MnemoServer::with_attention_state(...)`) |
| `mnemo.attention_state.get` | v0.4.5 — Look up an attention-state blob by `(agent_id, prefix_hash)`; returns `null` on miss |
| `mnemo.mem_write` | Agent-controlled memory (AutoMEM): persist an entry you explicitly decided to keep into your flat, agent-managed store |
| `mnemo.mem_read` | Agent-controlled memory (AutoMEM): read back only your own `agent-managed` entries |
| `mnemo.mem_revise` | Agent-controlled memory (AutoMEM): supersede a stale agent-managed entry with a corrected one (newest wins) |
| `mnemo.mem_forget` | Agent-controlled memory (AutoMEM): drop an agent-managed entry (soft by default; `hard=true` for permanent) |

## Access Protocols

| Protocol | Crate | Use Case |
|----------|-------|----------|
| **MCP** (stdio) | `mnemo-mcp` | AI agent integration via rmcp 3.0 |
| **REST** (HTTP) | `mnemo-rest` | Web clients, dashboards, OTLP ingest |
| **gRPC** | `mnemo-grpc` | High-performance service-to-service (12 RPCs) |
| **pgwire** | `mnemo-pgwire` | Connect with any PostgreSQL client (`psql`) |
| **AMP** (memorywire) | `mnemo-amp` | AMP-conformant adapter: 5 ops × 4 memory types over a JSON-Schema 2020-12 envelope |

### Attention-state-memory substrate (v0.4.5)

mnemo v0.4.5 ships an [attention-state-memory substrate](docs/research/context-memorization-2605.18226.md) anchored on [arXiv:2605.18226](https://arxiv.org/abs/2605.18226) (Context Memorization). Two new MCP tools — `mnemo.attention_state.put` and `mnemo.attention_state.get` — store and retrieve opaque attention-state blobs keyed by `(agent_id, prefix_hash)`. The substrate is implemented in [`crates/mnemo-attention-state`](crates/mnemo-attention-state) with a typed `AttentionStateStore` trait + an `InMemoryAttentionStateStore` reference impl.

**Honest scope:** mnemo ships the *store*. The producer (inference runtime that extracts prefix states) and the consumer (re-injection on the next generation) are out of scope; the substrate's blob format, model compatibility, and quantization sensitivity are the producer's responsibility. Tools are registered only when `MnemoServer::with_attention_state(...)` is configured at startup; unconfigured calls return a spec-shaped error result, not a panic. See the [research anchor](docs/research/context-memorization-2605.18226.md) for the operator recipe + the explicit non-overclaim disclaimers.

### Memory under interference — current-fact resolver (v0.4.7)

[arXiv:2605.18565](https://arxiv.org/abs/2605.18565) (MINTEval) measures how often memory systems return a *superseded* value of a fact after the same fact has been revised K times. mnemo v0.4.7 ships an **opt-in current-fact resolver** that post-processes the standard recall result set: candidates sharing the same value under a caller-chosen `fact_key` (typical convention: `"fact_id"`) are grouped, and only the most-recent write per group is kept. When `include_supersession_chain = true`, older fact-versions are returned in the response's new `superseded` field for audit.

Enable via the MCP `recall` tool param `current_fact_resolver: { fact_key, include_supersession_chain }`, the REST `?current_fact_key=…&current_fact_include_chain=true` query params, or the Rust `RecallRequest.current_fact_resolver = Some(...)` field directly. **The default read path is unchanged** — the resolver is purely additive and opt-in. The MINTEval-shaped bench at [`bench/locomo/src/bin/interference.rs`](bench/locomo/src/bin/interference.rs) compares default vs resolver arms across `K ∈ {1, 3, 5, 10}` revisions; see the resolver module doc at [`crates/mnemo-core/src/query/current_fact_resolver.rs`](crates/mnemo-core/src/query/current_fact_resolver.rs) for the contract + the explicit "not a contradiction detector / not a write-side guard" disclaimers.

### Repeated-context recall — orientation cache (v0.4.8)

[arXiv:2605.19932](https://arxiv.org/abs/2605.19932) (PEEK — Prefix-Encoded Episodic Knowledge) shows that a small, token-budgeted "orientation map" maintained alongside an agent's retrieval surface (key entities, `UPPER_SNAKE` constants, fenced schema fragments that have been useful) lets agents re-enter long-running contexts with a fraction of the recall payload. mnemo v0.4.8 ships an **opt-in orientation cache** that post-processes the standard recall result set: a heuristic Distiller extracts transferable knowledge from each hit and a priority Evictor enforces a fixed token budget (default 512). The recall response carries the bounded rendered map alongside `top-k` so the caller has both *what is in scope* and *what is relevant right now* in one payload.

Enable via the MCP `recall` tool param `orientation_cache: { namespace?, token_budget?, include_in_response?, distill? }`, the REST `?orientation_cache=true&orientation_namespace=…&orientation_token_budget=…` query params, the gRPC `OrientationCacheRequest` message, the pgwire `/*+ orientation_cache */` SQL hint comment, or the Rust `RecallRequest.orientation_cache = Some(OrientationCacheConfig::new())` field directly. The store is in-process, namespace-scoped (`(org_id, agent_id)` by default), and lost on restart — persistence is a v0.5.x knob. **The default read path is unchanged** and orientation rendering only fires when both the caller passes a config AND the engine has an `OrientationCacheStore` attached via `MnemoEngine::with_orientation_cache_store()`. See the PEEK-anchored bench at [`bench/locomo/src/bin/orientation.rs`](bench/locomo/src/bin/orientation.rs) for the repeated-context scenario (`K ∈ {3, 6, 10, 15}` calls per trial) and the module doc at [`crates/mnemo-core/src/query/orientation_cache.rs`](crates/mnemo-core/src/query/orientation_cache.rs) for the contract + the explicit "not a learned summariser / not a context-window extender / not persisted" disclaimers.

### Cost-aware, answer-impact-scored recall — evidence budget (v0.4.12)

The default recall path front-loads: it returns the top-`limit` records by fused score, and an LLM caller pays context tokens for every chunk whether or not it changes the answer. mnemo v0.4.12 ships an **opt-in cost-aware evidence budget** that runs over the already-ranked candidate set and returns *the smallest prefix that clears a configurable sufficiency bar*, capped by an optional `max_evidence`. It is purely subtractive — it only ever returns a prefix of the ranked list, so it can never reorder or silently lower the retrieval's top-k cosine ordering (enforced by a property test).

Relevance is computed through a pluggable **`EvidenceScorer` trait** so callers can swap the signal used to decide sufficiency: **`CosineScorer`** (default — cosine of candidate vs query embedding, falling back to the fused retrieval score when embeddings are absent/degenerate) and **`DeltaScorer`** — an *answer-impact* scorer that rates a chunk by whether adding it to the evidence already selected would change a downstream answer. The "would the answer change?" judgement is an **injectable closure** (`DeltaScorer::new(|ctx| …)`) so the engine core stays model-agnostic; `DeltaScorer::stub()` ships a deterministic marginal-novelty heuristic for tests and offline use.

Enable via the Rust `RecallRequest.evidence_budget = Some(EvidenceBudget { max_evidence, stop_when_sufficient, sufficiency_threshold, scorer })` field. Attach a custom answer-impact scorer with `MnemoEngine::with_evidence_scorer(Arc<dyn EvidenceScorer>)`; when the budget selects `ScorerKind::Delta` but no scorer is attached, the path falls back to cosine rather than erroring. The recall response carries an `evidence_selection` diagnostics block (scorer name, examined vs returned counts, cumulative score, early-stop / capped flags). **The default read path is unchanged** — no `evidence_budget` means the legacy front-loaded top-`limit`. See the module doc + property test at [`crates/mnemo-core/src/query/evidence.rs`](crates/mnemo-core/src/query/evidence.rs) and the end-to-end integration test at [`crates/mnemo-core/tests/cost_aware_recall.rs`](crates/mnemo-core/tests/cost_aware_recall.rs).

### Budgeted evidence retention — EMBER-style capsules (arXiv:2606.05894)

Where the v0.4.12 evidence budget caps *how many records* come back, an agent that must keep evidence **resident** in a bounded context window has a different problem: raw chunk dumps burn the window fast — a handful of full records and it's full, even though most of each record is filler around one salient fact. [EMBER (arXiv:2606.05894)](https://arxiv.org/abs/2606.05894) frames this as a *writer* problem: under a fixed retained-token budget, keep compact, **recoverable** evidence rather than raw text.

mnemo extends the verified recall surface with an opt-in `RecallRequest.retained_token_budget: Option<usize>`. When set, the engine packs the recalled hits into at most `budget` tokens as verbatim **evidence capsules** — a short verbatim excerpt **plus a retrieval key** (the record id, so the caller re-fetches the full chunk on demand) — ranked by a v0 **recoverability heuristic** (`recency × retrieval-hit-rate`) standing in for EMBER's learned writer. It is purely **additive**: the `memories` list is unchanged, and the capsule view rides in `RecallResponse.retained_evidence`. No new enum, no protocol change — just the one parameter on the existing surface.

The eval at [`crates/mnemo-core/tests/budgeted_evidence_retention.rs`](crates/mnemo-core/tests/budgeted_evidence_retention.rs) measures the knob's value on a LongMemEval-style fixture (60 gold facts, each behind a noisy chunk) at a fixed **8192-token** budget — budgeted capsules vs naive truncation:

```
| arm               | covered | recall@budget | retained tokens |
|-------------------|--------:|--------------:|----------------:|
| naive truncation  |      45 |         0.750 |             n/a |
| budgeted capsules |      60 |         1.000 |            4380 |
```

Because each capsule costs a fraction of a raw chunk, every gold fact survives the budget (recall 1.000 vs 0.750) using barely half the tokens. See the module doc + tests at [`crates/mnemo-core/src/query/retained.rs`](crates/mnemo-core/src/query/retained.rs) for the cost model + the full "what this is NOT" block (v0 heuristic, not EMBER's learned writer; read-side projection, nothing mutated).

### Agent-controlled memory mode — agent-managed flat store (AutoMEM, arXiv:2606.04315)

Mnemo's default path is an **ingestion + retrieval pipeline**: content is written, indexed, and recalled by vector + BM25 + graph + RRF. [AutoMEM (arXiv:2606.04315)](https://arxiv.org/abs/2606.04315) frames a **crossover**: the fixed pipeline wins **single-shot** retrieval (it ingested everything, so the fact is in the index), but on **long-horizon, multi-session** workloads an agent that **controls its own writes** over a simple flat store can win, because it *revises stale facts in place* instead of letting every version pile up and pollute retrieval.

So mnemo adds an opt-in **agent-controlled memory mode** over the MCP tool surface — four tools the agent itself calls to manage a flat store it curates:

| MCP tool | What the agent does | Composed from |
|---|---|---|
| `mnemo.mem_write` | persist an entry it judged worth keeping | `remember` (+ reserved `agent-managed` tag) |
| `mnemo.mem_read` | read back its own flat store (tag-scoped) | `recall` filtered to `agent-managed` |
| `mnemo.mem_revise` | supersede a stale entry with a corrected one | soft-`forget` old + `remember` new (`metadata.revises`) |
| `mnemo.mem_forget` | drop an entry it no longer wants | `forget` (soft / hard) |

These are **thin compositions over the verified `remember` / `recall` / `forget` primitives** plus a reserved `agent-managed` tag — no new engine enum or method. **The default `mnemo.recall` pipeline is untouched and remains the fallback for single-shot queries.** The point is *write-control*: the agent (not an ingestion heuristic) decides what persists.

The crossover eval at [`crates/mnemo-core/tests/agent_managed_crossover.rs`](crates/mnemo-core/tests/agent_managed_crossover.rs) measures both directions on a multi-session fixture (12 tracked facts × 3 revisions + 12 incidental details), holding retrieval to BM25 so the measured variable is purely write-control:

```
| query family              | mode           | recall@k | precision |  F1   |
|---------------------------|----------------|---------:|----------:|------:|
| long-horizon current-fact | fixed-pipeline |    1.000 |     0.333 | 0.500 |
| long-horizon current-fact | agent-managed  |    1.000 |     1.000 | 1.000 |
| single-shot incidental    | fixed-pipeline |    1.000 |     1.000 | 1.000 |
| single-shot incidental    | agent-managed  |    0.000 |     0.000 | 0.000 |
```

**The pipeline still wins single-shot retrieval** (recall 1.000 vs 0.000 — it ingested the incidental details the agent chose to skip); the **agent-managed path wins long-horizon write-control** (current-fact F1 1.000 vs 0.500 — it revised in place, so no stale versions dilute precision). The agent-managed mode is *for* long-horizon write-control, not a replacement for single-shot retrieval. See the tool inputs at [`crates/mnemo-mcp/src/tools/agent_managed.rs`](crates/mnemo-mcp/src/tools/agent_managed.rs).

### Experience-memory tier — cached plan replay (DocTrace, arXiv:2606.10921) — v0.4.14

The four primitives above (**REMEMBER / RECALL / FORGET / SHARE**) operate on *tier 1*: the raw memory store. [DocTrace (arXiv:2606.10921)](https://arxiv.org/abs/2606.10921) adds a *tier 2* — an **experience memory** that caches a **successful retrieval/reasoning plan** and **replays** it when a structurally-similar query recurs, instead of re-running full retrieval. mnemo ships this as a **mode, not a new store**: two new ops on the existing engine surface, gated behind `MnemoEngine::with_experience_memory()` (or the `MNEMO_EXPERIENCE_MEMORY=1` env on the server) so **default behaviour is unchanged**.

| Op | MCP tool | What it does |
|---|---|---|
| `REMEMBER_PLAN` | `mnemo.remember_plan` | Persist `{query-signature, steps, chunk ids, outcome score}` — **only** when the outcome clears the success threshold (failures are never cached). |
| `RECALL_PLAN` | `mnemo.recall_plan` | On a new query, return the best stored plan whose signature matches above a Jaccard threshold (default 0.7) — the cached chunk-set + step order — else a miss. |

Plans are persisted as **ordinary `MemoryRecord`s** carrying a reserved tag with the plan payload in `metadata`, so the tier is **backend-agnostic** (DuckDB + PostgreSQL, unchanged schema) and **RBAC/consent-gated** exactly like every other record (private plans are invisible to other agents; shared plans honour the ACL). A query *signature* is its normalized significant-token set; structural similarity is the Jaccard overlap of two signatures — deterministic and embedder-agnostic for the v0 replay gate. Plan records are excluded from ordinary `recall` (replayed only via `RECALL_PLAN`). See [`crates/mnemo-core/src/query/experience.rs`](crates/mnemo-core/src/query/experience.rs) and the contract tests at [`crates/mnemo-core/tests/experience_memory.rs`](crates/mnemo-core/tests/experience_memory.rs) (store-on-success, replay-on-similar, no-replay-on-dissimilar, RBAC, mode-off).

### Domain-scoped recall — anti vector-search-dilution (MASDR-RAG, arXiv:2606.11350) — v0.4.15

`RetrievalMode::DomainScoped` adds a recall mode that **pre-filters to a metadata-defined sub-corpus before the dense similarity step**, then runs a single vector pass. The predicate rides on an additive, optional `RecallRequest.domain_scope` kwarg (a `DomainScope { org_id, namespace, doc_class, tags }`) — no breaking change to existing callers, and the legacy `strategy` / typed `mode` paths are untouched. Over MCP, pass a `domain_scope` object to `mnemo.recall` (named `domain_scope`, not `scope`, because `scope` already filters visibility):

```jsonc
{ "query": "indemnification cap", "domain_scope": { "org_id": "acme", "namespace": "legal", "doc_class": "contract" } }
```

**Why scoping beats more-documents.** Dense retrieval ranks by semantic similarity, and similarity is *not* domain-awareness: a query about one tenant's contracts is highly similar to every other tenant's contracts too. As the corpus grows, those off-domain-but-on-topic records crowd into the top-k and push the genuinely relevant ones out — so adding documents makes precision **worse**, not better. Restricting the candidate set to the right sub-corpus *before* the vector search removes the dilutants entirely, so precision is independent of how large the rest of the corpus gets. The dilution eval at [`crates/mnemo-core/tests/domain_scoped_dilution.rs`](crates/mnemo-core/tests/domain_scoped_dilution.rs) reproduces the curve on a corpus growing 50 → 1,000 docs (10 fixed in-domain gold docs, the rest same-topic off-domain distractors):

```
| corpus | flat P@10 | domain-scoped P@10 | gap    |
|-------:|----------:|-------------------:|-------:|
|     50 |     0.100 |              1.000 | +0.900 |
|    200 |     0.000 |              1.000 | +1.000 |
|   1000 |     0.000 |              1.000 | +1.000 |
```

Flat semantic recall collapses to **0.000** P@10 by 1,000 docs; domain-scoped holds at **1.000**. The metadata predicate is enforced through the existing storage layer, so it works on **both** the DuckDB and PostgreSQL backends and respects RBAC/scope exactly like ordinary recall.

### Active-reconstruction recall — `reconstruct` (MRAgent, arXiv:2606.06036) — v0.5.1

`strategy = "reconstruct"` (typed: `RetrievalMode::Reconstruct`) adds an **opt-in** recall strategy that, instead of returning only the top-k snippets, (a) retrieves candidates with the default hybrid RRF, (b) **walks the existing memory-graph edges** from those hits to gather linked/causal context, and (c) synthesises a deterministic **belief-state node** returned *alongside* the raw hits — MRAgent's cue → linked-context → reconstruct pattern. It is surfaced as a strategy parameter on the existing recall API across all four protocols (MCP `strategy: "reconstruct"`, REST `?strategy=reconstruct`, gRPC `RecallRequest.strategy`, pgwire `SELECT ... /*+ reconstruct */`), so the tool surface is unchanged.

```jsonc
{ "query": "Project Apollo owner", "strategy": "reconstruct" }
// → { "memories": [ ...the usual top-k... ],
//     "reconstruction": { "cue": "...", "summary": "Reconstructed belief ...",
//                         "source_ids": [...], "linked_context_ids": [...], "confidence": 0.x } }
```

The raw `memories` list is **exactly** what the default `auto` path returns — `reconstruct` is purely additive (the belief node is extra), and the synthesis is rule-based (no LLM), so it is deterministic and reproducible. **This is an option to test reconstruction-vs-retrieval on your own data, not a claim that retrieval is wrong.** MRAgent reports up to ~23% gains on multi-hop questions; the A/B harness at [`bench/locomo/src/bin/reconstruct_ab.rs`](bench/locomo/src/bin/reconstruct_ab.rs) lets you check the mechanism on mnemo itself by measuring *gold-coverage@k* — whether a graph-linked answer that flat top-k retrieval misses is recovered by the reconstruction's linked context. On its (adversarially multi-hop, by-construction) fixture, `reconstruct` lifts coverage@5 from **0.083 → 0.208**; run it on your own corpus before drawing conclusions. The graph walk uses the same `related_to` relations the `graph` strategy traverses, so it works on both the DuckDB and PostgreSQL backends and respects RBAC/scope like ordinary recall.

### Offline consolidation — Auto-Dreamer-shaped active-bank shrink (v0.4.8)

Anthropic's Auto-Dreamer consolidation runs offline, away from the agent's interactive loop, and produces a smaller *active bank* of semantic summaries that should serve subsequent recall at least as well as the raw episodic trace it replaced. mnemo's existing `run_decay_pass` + `run_consolidation` path ([`crates/mnemo-core/src/query/lifecycle.rs`](crates/mnemo-core/src/query/lifecycle.rs), plus the reflection module at [`crates/mnemo-core/src/query/reflection.rs`](crates/mnemo-core/src/query/reflection.rs) that mirrors the same offline-housekeeping shape) is the engine-side equivalent: the decay pass marks low effective-importance records as `Archived` / `Forgotten`, the consolidation pass replaces tag-overlap clusters of episodic memories with structured `[Consolidated from N memories] …` semantic bundles, and the originals are flipped to `Consolidated` rather than deleted (so the chain stays auditable). The new Auto-Dreamer-shaped scenario at [`bench/locomo/src/bin/auto_dreamer_consolidation.rs`](bench/locomo/src/bin/auto_dreamer_consolidation.rs) exercises both passes end-to-end on a synthetic multi-session trajectory and reports the two axes Auto-Dreamer headlines as its claim: `active_bank_ratio = post / pre` (expects `< 1.0`) and held-out `recall_post >= recall_pre`. A JSON summary lands beside the Markdown report so the headline number is citable here.

### Topic-document consolidation (Infini-Memory, arXiv:2606.10677) — v0.5.0

`mnemo.consolidate` / `MnemoEngine::consolidate` adds a **caller-driven** consolidation primitive: it groups a *chosen* set of member memories into one revisable **topic document** — a semantic unit that collects related evidence, preserves metadata, and revises facts over time. This is the interactive, by-id sibling of the offline `run_consolidation` pass above: instead of clustering by tag overlap on a schedule, the caller names the members and the topic.

```jsonc
{ "memory_ids": ["<id1>", "<id2>", "<id3>"], "topic_name": "Acme account" }
```

The primitive is deterministic (no LLM): absent a caller `summary`, the document body is a stable join of the member contents ordered by `(created_at, id)`. It **preserves provenance** — the topic document's metadata records `consolidated_from` plus each member's source, timestamp, and confidence — and links `topic_document --consolidated_from--> member` relations so the evidence set is retrievable as a unit. Every consolidation appends a hash-chained `MemoryConsolidated` audit event.

**Fact revision keeps history.** Pass `supersede: <old_topic_document_id>` to revise a fact: the new document becomes version *N+1* (`prev_version_id → old`), the old document is **retained** (not deleted — deleting a mid-chain record would break `verify_integrity`) and marked `Consolidated` with a `superseded_by` pointer, and a `MemoryRevised` event is appended alongside. Reuse the same `topic_name` across revisions so the current-fact resolver (keyed on the `topic` metadata) collapses to the current view at recall time. The primitive is protocol-agnostic — reachable identically over MCP, REST (`POST /v1/consolidate`), and gRPC (`rpc Consolidate`). It is not exposed over pgwire, which stays SQL-only (`SELECT`/`INSERT`/`DELETE` → recall/remember/forget); consolidation is a primitive-RPC operation, not a SQL statement.

Run via `cargo run --release --bin auto_dreamer_consolidation -p mnemo-locomo-bench` — defaults to 8 sessions × 25 facts × 5 trials with archive/forget thresholds of 0.40 / 0.10 and `min_cluster_size = 3`; all tunable via CLI flags. **The default read path is unchanged** — the bench only consumes existing `mnemo_core::query::lifecycle::*` APIs and adds no public surface. See the bin module rustdoc for the full "what this bin is NOT" block (not a faithful Auto-Dreamer reproduction; not the `criterion` crate; `NoopEmbedding` makes the vector lane degenerate by design; single-agent, single-scope).

### Embedding-backend selection — SLA-aware recommender (v0.4.9)

[arXiv:2605.23618](https://arxiv.org/abs/2605.23618) (GE2 vs local encoders — quality + latency) motivates choosing an embedding backend by *measured* quality and tail-latency on the operator's workload, not by reputation. mnemo v0.4.9 ships [`bench/embeddings`](bench/embeddings), a criterion-driven bench + SLA-aware recommender that runs each *configured* backend (Noop and a bench-local hashing baseline always; `OpenAiEmbedding` when `OPENAI_API_KEY` is set; `OnnxEmbedding` when `MNEMO_ONNX_MODEL_PATH` is set and `mnemo-core` was built with the `onnx` feature) against a 50-document / 10-query labeled fixture and reports nDCG@10, recall@10, p50/p95 single-vector embed latency, and throughput at batch sizes 1/8/32. The recommender then picks the **highest-nDCG backend whose p95 ≤ the SLO** and reports the explicit nDCG gap vs the absolute best-quality backend (so the operator sees "you give up 0.003 nDCG for 7x lower p95 latency" rather than a black-box choice).

Run via `mnemo bench embeddings --slo-ms <N>` (built into the `mnemo` binary) or `cargo bench -p mnemo-embeddings-bench` (criterion HTML reports at `target/criterion/embed_single/`). **The default read path is unchanged** — no retrieval defaults, no RRF weights, no `EmbeddingProvider` impls are touched. The embedded-first wedge stays: default builds run without `OPENAI_API_KEY` and the recommender picks a local backend. See [`bench/embeddings/README.md`](bench/embeddings/README.md) for the full "what this bench is NOT" block (not a faithful arXiv:2605.23618 reproduction; not a managed-cloud default; `hashing-baseline` is a bench-local lexical sanity floor, not a production backend).

### mnemo as a golem:vector provider (v0.4.6)

mnemo v0.4.6 ships a vertical-slice WASM-component implementation of the [`golem:vector@1.0.0`](https://github.com/golemcloud/golem-ai/issues/21) WIT interface — three load-bearing functions (`upsert-vector` / `search-vectors` / `delete-vectors`) — split across two crates: [`crates/mnemo-golem-wit`](crates/mnemo-golem-wit) (the WASM component, compiled to `wasm32-wasip2` via `cargo component build`) and [`crates/mnemo-golem-host`](crates/mnemo-golem-host) (the Rust host that owns an `Arc<MnemoEngine>` and supplies the WIT host imports). The two-crate split is forced by mnemo-core's C++ deps (DuckDB + USearch) which cannot compile to WASM — see [`docs/research/golem-vector-wit-provider.md`](docs/research/golem-vector-wit-provider.md) for the layering rationale, the per-function gap list (27 of 30 deferred to v0.5.x), and the wasmtime-component-loader wiring step explicitly deferred. The vertical-slice integration is functionally complete as a Rust trait surface today (`MnemoGolemProvider` + `MnemoGolemHost`) with 5 integration tests + an end-to-end example showing REMEMBER → RECALL → DELETE through a real `MnemoEngine`.

### Which parts of the MCP spec mnemo implements

The current spec revision is [**2026-07-28**][spec2026], which removes
protocol-level sessions and the `initialize` handshake, adds `server/discover`,
requires `Mcp-Method` / `Mcp-Name` headers and `ttlMs` / `cacheScope` on list
results, and deprecates Roots, Sampling, Logging and the HTTP+SSE transport.

**mnemo negotiates `2025-11-25`, not `2026-07-28`.** mnemo speaks MCP through
the [`rmcp = "3.0"`][rmcp] workspace dep (resolving to 3.1.3), and follows
rmcp's implementation as it lands rather than racing the spec. rmcp knows the
newer revision but its `ProtocolVersion::LATEST` is still `2025-11-25`, so that
is what a handshake settles on.

Where mnemo already satisfies the new revision it is because of decisions taken
earlier, not because it tracked the spec. Cross-call state has always travelled
through **explicit, server-minted handles passed as ordinary tool arguments**
(`checkpoint_id`, `lease_token`), which [SEP-2567] makes the sanctioned
replacement for sessions; mnemo implements none of the deprecated features; and
tool listings are already deterministically ordered.

Row by row, with what is still open and what waits on rmcp:
**[docs/src/integrations/mcp-2026-07-28.md](docs/src/integrations/mcp-2026-07-28.md)**.
It is not a marketing page. Four rows are still open, every one of them carrying a
sentence saying what a caller should assume instead, and a CI test refuses to let
an open row exist without one. It also states plainly that **mnemo implements no
OAuth authorization**: no RFC 9728 Protected Resource Metadata document is served
on any transport, which is a conformant position on stdio (the spec says stdio
servers **SHOULD NOT** follow the authorization spec and should take credentials
from the environment, which is what mnemo does) and a thing worth saying out loud
rather than leaving a reader to infer from silence.

<details>
<summary>Earlier anchor: the March 2026 MCP Roadmap</summary>

The [MCP 2026 Roadmap][roadmap] (published 2026-03-09) organised the protocol's
direction around four priority areas: Transport Evolution and Scalability, Agent
Communication, Governance Maturation, and Enterprise Readiness. mnemo's
operator-held HMAC keystore, AES-256-GCM at-rest encryption, dual DuckDB /
PostgreSQL backends and `mnemo-compliance` crate sit under **Enterprise
Readiness** as an *attestable memory* layer.

This is kept as history. The July spec release superseded the roadmap as the
statement of current direction, and the conformance table above is where mnemo's
actual state is recorded. The four-priority-area mapping is preserved in
[`docs/src/integrations/mcp-server.md`](docs/src/integrations/mcp-server.md).

</details>

[spec2026]: https://modelcontextprotocol.io/specification/2026-07-28/changelog
[roadmap]: https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/
[rmcp]: https://crates.io/crates/rmcp
[SEP-2567]: https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2567

## SDKs

### Python

```bash
pip install mnemo-db
```

> **Why `mnemo-db` and not `mnemo`?** A 2021 notebook project (last release 2021-07-06, unrelated) holds the unqualified `mnemo` name on PyPI. Our distribution publishes as `mnemo-db`; the import path stays `from mnemo import …` so existing code is unaffected.

<!-- BEGIN generated: python-sdk-compat -->
<!-- Regenerate with: python3 scripts/gen_published_versions.py -->

> **Version line & wire compatibility.** `pip install mnemo-db` gives **`v0.5.27`**. The Python SDK is **not** independently versioned: `python/` is PyO3 bindings that compile `mnemo-core` *into the wheel*, so the wheel version names the engine inside it, and [`workspace_version_fence.rs`](crates/mnemo-cli/tests/workspace_version_fence.rs) fails CI if `pyproject.toml` and `mnemo/__init__.py` drift from `[workspace.package].version`.
>
> - **In-process, `MnemoClient` (the PyO3 extension).** `mnemo-db` `v0.5.27` *is* `mnemo-core` `v0.5.27`. There is no version-skew question to answer: the engine is the wheel.
> - **`pip install mnemo-db` and `cargo add mnemo-core` do not currently resolve the same version.** PyPI has `v0.5.27`; crates.io has `v0.5.26`. The wheel publishes on merge to `main` while the crates publish on a tag, so the Python side leads inside an open release window. Pin deliberately if you embed both.
> - **Over MCP, the `agno` / `camel` / `agno-memory` adapters.** These embed no engine; they spawn the external `mnemo` server binary you install and bind to its **MCP tool surface** (the 23 registered tools), not to a `mnemo-core` version. They are wire-compatible with any **0.5.x** `mnemo-mcp-server`. Server properties such as the rmcp 3.0 transport and the tool-catalog attestation come from **that binary**, not from the SDK, so run a current one to get them.
<!-- END generated: python-sdk-compat -->

```python
from mnemo import MnemoClient

client = MnemoClient(db_path="agent.mnemo.db")
result = client.remember("The user prefers dark mode", tags=["preference"])
memories = client.recall("user preferences", limit=5)
client.forget([result["id"]])

# Mem0-compatible aliases also available:
# client.add(), client.search(), client.delete()
```

#### Framework Integrations

Mnemo provides native integration modules for 15 agent frameworks:

| Framework | Integration Class | Connection |
|-----------|------------------|------------|
| [OpenAI Agents SDK](https://github.com/openai/openai-agents-python) | `MnemoAgentMemory` | MCP stdio |
| [LangGraph](https://github.com/langchain-ai/langgraph) | `MnemoLangGraphTools` | MCP stdio |
| [CrewAI](https://github.com/crewAIInc/crewAI) | `ASMDMemory` | Direct PyO3 |
| [Google ADK](https://github.com/google/adk-python) | `MnemoADKToolset` | MCP stdio |
| [Agno](https://github.com/agno-agi/agno) | `MnemoAgnoTools` | MCP stdio |
| [Pydantic AI](https://github.com/pydantic/pydantic-ai) | `MnemoPydanticToolset` | MCP stdio |
| [AutoGen](https://github.com/microsoft/autogen) | `MnemoAutoGenWorkbench` | MCP stdio |
| [Smolagents](https://github.com/huggingface/smolagents) | `MnemoSmolagentsTools` | MCP stdio |
| [Strands Agents](https://github.com/strands-agents/sdk-python) | `MnemoStrandsClient` | MCP stdio |
| [Semantic Kernel](https://github.com/microsoft/semantic-kernel) | `MnemoSKPlugin` | MCP stdio |
| [Llama Stack](https://github.com/meta-llama/llama-stack) | `register_mnemo_toolgroup` | REST API |
| [DSPy](https://github.com/stanfordnlp/dspy) | `create_mnemo_tools` | Direct PyO3 |
| [CAMEL AI](https://github.com/camel-ai/camel) | `create_mnemo_camel_tools` | Direct PyO3 |
| [Mem0](https://github.com/mem0ai/mem0) (compat) | `Mem0Compat` | Direct PyO3 |
| LangGraph Checkpointer | `ASMDCheckpointer` | Direct PyO3 |

All integrations are auto-imported via `from mnemo import <ClassName>` — dependencies fail gracefully if not installed.

#### Memory-tool servers and shared-memory adapters (v0.3.4 → v0.4.1)

| Surface | Class | What it does |
|---|---|---|
| [Anthropic memory tool `memory_20250818`](docs/src/integrations/anthropic-memory-tool.md) | `MnemoMemoryToolServer` | Client-side handler for the 6-op `view`/`create`/`str_replace`/`insert`/`delete`/`rename` surface — every "file" lands as a Mnemo memory with hash-chain + ACL coverage. `pip install 'mnemo-db[anthropic-memory-tool]'`. |
| [Letta Conversations-style shared memory](docs/src/integrations/letta-conversations.md) | `MnemoLettaShared` | Multiple agents sharing a single audit-replayable memory stream. `attach`/`detach`/`read`/`write`/`list_participants` over Mnemo memories tagged `conversation:<id>` + `participant:<agent_id>`. |
| [Cloudflare R2 workspace](docs/src/integrations/r2-workspace.md) | `CloudflareR2Workspace` | Drop-in R2 backend for `MnemoSnapshotStore` — same signed-manifest contract as the AWS S3 path; `pip install 'mnemo-db[openai-sandbox-r2]'`. |
| [Letta-protocol-compat REST surface](crates/mnemo-letta/) (`mnemo-letta` crate) | `mnemo_letta::router(engine)` | `POST /v1/agents`, `POST /v1/agents/{id}/messages`, `GET /v1/agents/{id}/memory` — drop in front of any `MnemoEngine` so a Letta-Code-shaped benchmark or notebook can talk to Mnemo without code changes. New in v0.4.0-rc3 (B5). |
| [AMP / memorywire wire format](crates/mnemo-amp/) (`mnemo-amp` crate) | `mnemo_amp::MnemoAmpStore` + `AmpRouter` | AMP-conformant `MemoryStore` surface: 5 ops (`remember`/`recall`/`forget`/`merge`/`expire`) × 4 memory types over a JSON-Schema 2020-12 envelope. `merge`/`expire` are thin compositions over the real primitives (no fictitious engine method); a fan-out router fuses multi-adapter recall with RRF; an optional HITL diff-and-approve hook gates long-term writes and records approvals in the hash-chained audit log. Conformance: recall@5 + RRF-holds-under-rank-0-injection vs max-fusion. New in v0.4.13. |
| [Mannsetu DPDPA consent manager](crates/mnemo-compliance/src/mannsetu.rs) (`mnemo-compliance` crate) | `MannsetuConsentSource` + `ConsentTokenGuard` | DPB-registered consent-manager binding plus a per-write guard with expiry / scope / revocation checks. Refuses any `remember` whose consent token is missing, expired, wrong-scope, or revoked. New in v0.4.0-rc3 (B4). |
| [DPDPA "data passport" PDF](python/mnemo/dpdpa_passport.py) | `mnemo.dpdpa_passport.build_passport_pdf` | One-page PDF showing every personal data point Mnemo holds for a subject, suitable for Section 11 / 12 access requests. Hand-rolled PDF — zero third-party deps, byte-for-byte reproducible. New in v0.4.0-rc3 (Q3). |
| [Provenance SDK](python/mnemo/provenance.py) | `mnemo.provenance.verify_read_provenance` | Pure-Python verifier for the HMAC-SHA256 receipts that Mnemo returns alongside `recall(..., with_provenance=True)`. Auditors verify offline without compiling Rust. New in v0.4.0-rc3 (Q1). |
| [Claude Code installer](python/mnemo/install_claude_code.py) | `python -m mnemo install claude-code [--hardened <manifest>]` | Idempotently registers Mnemo as an MCP server in `~/.claude.json`. The `--hardened` flag switches the registered launcher to the v0.4.0-rc3 hardened mode. New in v0.4.0-rc3 (Q2). |
| [Anthropic CMA-Memory compat shim](crates/mnemo-cma/) (`mnemo-cma` crate) | `CmaTreeRoot` + `import_cma_tree` + `audit_bridge` | Drop-in for the Anthropic Context-Managed Agent Memory beta announced 2026-04-23. Mounts an existing CMA `.memory/` tree, mirrors writes through to mnemo's HMAC chain, and exports back byte-identical so users can leave cleanly. New in v0.4.1 (P0-2). |
| [Agent behavioural-baseline exporter](crates/mnemo-baseline/) (`mnemo-baseline` crate) | `AgentBaseline` + `JsonExporter` (OTel + OCSF) | Per-agent rolling profile (recall/write rates, namespace fanout, tool mix, HMAC continuity) emitted to OpenTelemetry semconv 1.31 + OCSF 1.4 Application-Activity envelopes with z-score+EWMA drift detection. Anti-leak invariant: emitted payloads never carry memory contents. Plugs into the agentic-SOC telemetry gap RSAC 2026 flagged. New in v0.4.1 (P0-3). |
| [1M-context recall budget planner](crates/mnemo-core/src/budget/) (`mnemo-core::budget`) | `ContextBudget::for_model` + `plan_recall` | First OSS memory store with an explicit per-model `ContextBudget → RecallPlan` planner. Per-model table covers `deepseek-v4-1m`, `claude-3.7-sonnet-1m`, `gpt-5.1-400k`, `gemini-2.5-pro-2m` plus their smaller siblings. Typed `FallbackStrategy` (TruncateOldest / SummarizeOldestK / DropDuplicates / None). Property test: never overflows total context. New in v0.4.1 (P1-4). |
| [Project-Deal counterparty discovery + reputation](crates/mnemo-deal/) (`mnemo-deal::discovery` + `::reputation`) | `AgentAdvertisement` + `compute_reputation` | `/.well-known/mnemo-deal-agent.json` advertisement (Ed25519-keyed, capability-tagged) plus an advisory reputation score with 90-day half-life decay and per-dispute 10% penalty. mnemo becomes not just the deal ledger but the directory of the agent-deal substrate. **Advisory only** — see `docs/deal-reputation-threats.md`. New in v0.4.1 (P1-5). |

### TypeScript

> **Status: maintenance only, not on the 0.5 train.** `npm install
> @mndfreek/mnemo-sdk` gives **0.4.4** (published 2026-05-18) while the Rust line
> is on 0.5.x. It still works, because it is a thin MCP-over-STDIO client that
> targets the server's **tool surface** rather than a `mnemo-core` version: the
> published 0.4.4 package was run against a `mnemo-mcp-server` built from
> **`v0.5.26`**, and `remember`, `recall` and `verify` all succeeded with a valid
> hash chain. Publishing newer versions (`package.json` is at 0.4.8) is blocked on
> an expired `NPM_TOKEN`, an operator action rather than a code change. What that
> verification does and does not cover, plus the `recall` default that will bite
> you first, is in [`sdks/typescript/README.md`](sdks/typescript/README.md).

```typescript
import { MnemoClient } from "@mndfreek/mnemo-sdk";

const client = new MnemoClient({ dbPath: "agent.mnemo.db" });
await client.connect();

const { id } = await client.remember({ content: "User prefers dark mode" });
const { memories } = await client.recall({ query: "user preferences" });
await client.share({ memory_id: id, target_agent_id: "auditor-agent" });

await client.close();
```

### Go

```go
import "github.com/sattyamjjain/mnemo/sdks/go"

client, err := mnemo.NewClient(mnemo.ClientOptions{DbPath: "agent.mnemo.db"})
defer client.Close()

result, _ := client.Remember(mnemo.RememberInput{Content: "User prefers dark mode"})
memories, _ := client.Recall(mnemo.RecallInput{Query: "user preferences"})
_, _ = client.Share(mnemo.ShareInput{MemoryID: result.ID, TargetAgentID: "auditor-agent"})
```

## Storage Backends

Two backends implement the same `StorageBackend` trait. As of v0.5.7 they are
**feature-equivalent for recall** — semantic / hybrid / graph / domain-scoped
recall now run real pgvector ANN on PostgreSQL ([#99](https://github.com/sattyamjjain/mnemo/issues/99)).
The matrix below is explicit about what each backend does.

| Backend | Best For |
|---------|----------|
| **DuckDB** (default) | Single-agent, embedded, zero-config — semantic/vector recall via USearch HNSW |
| **PostgreSQL** + pgvector | Multi-agent CRUD / ACL / audit at scale — **plus** semantic/vector recall via the pgvector HNSW index |

### Backend capability matrix

| Capability | DuckDB + USearch | PostgreSQL + pgvector |
|---|:---:|:---:|
| Semantic / vector recall (`strategy="semantic"`) | ✅ | ✅ ¹ |
| Hybrid RRF recall (`strategy="auto"`) | ✅ | ✅ ¹ |
| Graph recall (`strategy="graph"`) | ✅ | ✅ ¹ |
| Domain-scoped recall (`strategy="domain_scoped"`) | ✅ | ✅ ¹ |
| Active-reconstruction recall (`strategy="reconstruct"`) | ✅ | ✅ ¹ |
| Lexical / BM25 recall (`strategy="lexical"`) | ✅ | ✅ |
| Exact / filter recall (`strategy="exact"`) | ✅ | ✅ |
| Remember / CRUD / soft+hard delete | ✅ | ✅ |
| ACL / sharing / delegation | ✅ | ✅ |
| Checkpoint / branch / merge / replay | ✅ | ✅ |
| Hash-chain audit / `verify` | ✅ | ✅ |

¹ **Postgres vector recall runs a real pgvector ANN query** against the
`idx_memories_embedding_hnsw` HNSW index (cosine `<=>`, `ORDER BY … LIMIT k`),
with the same permission-safe oversample-then-filter the USearch backend uses,
so filtered/scoped recall never under-returns. As of **v0.5.18** the
`VectorIndex::search` / `filtered_search` methods are **async** (#99 resolved):
the `sqlx` query is `.await`ed directly on the caller's ambient Tokio runtime, so
Postgres semantic recall works from inside the server/CLI `#[tokio::main]`
runtime on **any flavor** — single- or multi-threaded — with **no `block_on`
bridge** and therefore no "runtime within a runtime" panic or deadlock. If the
pgvector extension / `<=>` operator is genuinely absent at runtime, the vector
lane still **fails loud** with a typed
[`Error::BackendUnsupported`](crates/mnemo-core/src/error.rs) — never a silent
empty result. Verified end-to-end against a live pgvector Postgres by the
`MNEMO_TEST_POSTGRES_URL`-gated integration test
[`crates/mnemo-postgres/tests/pgvector_ann.rs`](crates/mnemo-postgres/tests/pgvector_ann.rs):
nearest-in-rank-order + permission filter under a multi-threaded runtime, **and**
a `current_thread`-runtime regression test that the old bridge would have
panicked on.

That integration test **skips** when `MNEMO_TEST_POSTGRES_URL` is unset, so on an
ordinary CI run it proved nothing about the fail-loud claim.
[`crates/mnemo-postgres/tests/semantic_recall_fails_loud.rs`](crates/mnemo-postgres/tests/semantic_recall_fails_loud.rs)
closes that hole: it needs no database, runs on every `cargo test --workspace`,
and asserts through the **engine** (not just the index) that `semantic`, `auto`,
`graph` and `domain_scoped` recall each return `BackendUnsupported` rather than
`Ok(empty)` — against a store that provably contains a record, so "no matches"
and "not implemented" cannot be confused. Re-introducing a single
`.unwrap_or_default()` on any of the four `filtered_search` call sites in
`recall.rs` turns it red.

### Embedder support matrix — which embedders actually produce semantic results

Semantic recall is only as real as the **embedder** behind it. A backend's vector
index cannot manufacture meaning from a query that was never embedded — so the
*supported semantic path is a real embedder + either backend*, and mnemo now
**fails loud** rather than silently returning empty when the embedder cannot
produce vectors (v0.5.13).

| Embedder | How it is configured | Semantic / hybrid / `auto` / graph / domain-scoped recall | Lexical / exact recall |
|---|---|:---:|:---:|
| **OpenAI** (`OpenAiEmbedding`) | `OPENAI_API_KEY` set | ✅ **supported semantic path** | ✅ |
| **ONNX, local** (`OnnxEmbedding`) | `MNEMO_ONNX_MODEL_PATH` set + built with `--features onnx` | ✅ **supported semantic path** (fully on-prem) | ✅ |
| **Deterministic** (`DeterministicEmbedding`) | in-process, offline | ✅ works — but lexical-hashing, **for tests/demos, not production semantics** | ✅ |
| **No-op** (`NoopEmbedding`, the default when no key/model is configured) | default | ❌ **hard-errors** `EmbedderNotConfigured` | ✅ |

**The supported semantic path is DuckDB (or PostgreSQL) + a real embedder** —
`OpenAiEmbedding` (HTTP) or, for a fully on-prem deployment, `OnnxEmbedding`
(local model, `onnx` feature). With no real embedder configured, mnemo runs with
the no-op embedder, whose query vectors are all-zero; any semantic /
hybrid (`auto`) / graph / domain-scoped recall then returns a typed
[`Error::EmbedderNotConfigured`](crates/mnemo-core/src/error.rs) — **never a
silent empty result set**. Lexical (BM25) and exact/filter recall need no
embedder and always work. This mirrors the Postgres `BackendUnsupported`
fail-loud behaviour above: mnemo will not pretend to do semantic recall it cannot
actually perform.

### Compliance profiles — processing-log retention

Same posture, applied to **retention**: mnemo's `agent_events` log is append-only
by construction (no `forget` / TTL / decay / cold-tier path removes an event —
each *appends* one), so [`mnemo-compliance`](https://crates.io/crates/mnemo-compliance)
ships a [`RetentionProfile`](crates/mnemo-compliance/src/retention.rs) that
**verifies** a minimum retention floor held across every deletion path, and fails
loud (`ComplianceError::RetentionFloorUnsupported`, naming the backend) if a
backend cannot guarantee it. These are **conformance checks for** the obligation
named — **not** a certification or a claim of compliance.

| Profile | Obligation it maps to | Retention floor | Commencement | Primary source |
|---|---|---:|---|---|
| **India DPDP Rules 2025** (`dpdp`) | Retain personal data, **traffic data and processing logs** (Seventh Schedule) | **365 days** | 2027-05-13 (data-fiduciary obligations; Gazette G.S.R. 846(E), 2025-11-13) | [MeitY DPDP Rules 2025](https://www.meity.gov.in/documents/act-and-policies/digital-personal-data-protection-rules-2025-gDOxUjMtQWa) |
| **EU AI Act Art.19 / 26(6)** (`eu-ai-act-art19`) | Deployers keep automatically-generated logs ≥ 6 months | **180 days** | 2027-12-02 (stand-alone Annex III) / 2028-08-02 (Annex I embedded) per the Digital Omnibus | [Reg (EU) 2024/1689](https://eur-lex.europa.eu/eli/reg/2024/1689/oj) · [Council, 2026-06-29](https://www.consilium.europa.eu/en/press/press-releases/2026/06/29/artificial-intelligence-council-gives-final-green-light-to-simplify-and-streamline-rules/) |
| **HIPAA §164.312(b)** (`hipaa`) | Audit controls; documentation retained six years (§164.316(b)(2)) | **2190 days** (6 yr) | in force | [eCFR 45 CFR §164.312](https://www.ecfr.gov/current/title-45/subtitle-A/subchapter-C/part-164/subpart-C/section-164.312) |

Run the reproducible conformance harness (every deletion path × the floor, byte-stable artifact):

```bash
cargo run --release -p mnemo-retention-conformance-bench                       # DPDP, 365-day floor
cargo run --release -p mnemo-retention-conformance-bench -- --profile eu-ai-act-art19
mnemo compliance retention --profile dpdp                                      # print the profile + backend gate
```

Dates are provisional and depend on the final notified/enacted texts — verify
against the primary sources before relying on them. Why this matters, and how
mnemo compares on the compliance-audit axis: **[docs/POSITIONING.md](docs/POSITIONING.md)**.

## Key Features

- **Hybrid retrieval** — Reciprocal Rank Fusion combining semantic vectors (USearch/pgvector), BM25 keywords (Tantivy), knowledge graph signals, and recency scoring with configurable weights
- **Bitemporal graph layer** ([`mnemo-graph`](docs/src/concepts/temporal-edges.md)) — Graphiti-inspired temporal edges with `valid_from` / `valid_to` (fact validity) plus `recorded_at` (system clock). `graph_expand(seed, depth, as_of)` walks the graph at any point in time without losing later corrections. New in v0.4.0-rc1.
- **AES-256-GCM encryption** — at-rest content encryption via `MNEMO_ENCRYPTION_KEY`
- **Hash chain integrity** — SHA-256 content hashes with chain linking and `verify` tool
- **Memory poisoning detection** — anomaly scoring with prompt injection pattern detection; quarantine for flagged content
- **Cognitive forgetting** — five strategies: soft delete, hard delete, decay, consolidation, archive
- **Feedback-driven consolidation trigger** — opt-in `ConsolidationPolicy::MaturityDriven` gates `run_consolidation` on a per-cluster maturity score (recency × hit-success × edge-degree × redundancy) instead of firing on a fixed schedule. Inherited by `forget` and `checkpoint` automatically across MCP / REST / gRPC / pgwire; the default `FixedSize` policy preserves the v0.4.x behaviour byte-for-byte. New in v0.4.10. <!-- prior art: FluxMem, arXiv:2605.28773 — structural cousin only, not a reproduction. -->
- **Branching and replay** — checkpoint, branch, merge, and replay agent memory timelines
- **Point-in-time queries** — recall memories as they existed at any timestamp with `as_of`
- **Causal debugging** — trace event causality chains up/down with type filtering
- **RBAC + delegation** — ACL-based permissions with scoped, depth-limited transitive delegation
- **Permission-safe ANN** — iterative oversampling with post-filtering for ACL compliance
- **ONNX local embeddings** — run embeddings locally without API calls via `MNEMO_ONNX_MODEL_PATH`
- **S3 cold storage** — archive old memories to S3-compatible storage (feature-gated)
- **LRU cache** — in-memory caching layer for frequently accessed memories
- **Scale-to-zero** — auto-shutdown after configurable idle timeout with checkpoint-on-shutdown
- **OTLP observability** — ingest OpenTelemetry GenAI spans as agent events
- **Append-only audit log** — immutable event log with database-enforced triggers (PostgreSQL)
- **GEM trajectory-correctness audit** — `mnemo-compliance::trajectory_audit` replays the hash-chained event log and reports four trajectory-level signals: (a) unregulated-growth (active-bank vs ceiling), (b) missing-semantic-revision (facts superseded but never revised), (c) capacity-driven-forgetting (deletes outside the 5 named strategies), (d) read-only-retrieval (scopes that only RECALL). Surfaced via `mnemo.trajectory_audit` (MCP), `POST /v1/compliance/trajectory_audit` (REST), and the `TrajectoryAudit` gRPC RPC — same `(agent_id, thread_id)` shape as `mnemo.verify`, on the orthogonal trajectory axis. <!-- anchor: GEM, arXiv:2605.26252 — prior art only, structural cousin. -->
- **MemFail per-operation fault-isolation suite** — `mnemo_core::eval::memfail` decomposes each end-to-end recall into the three operation seams mnemo exposes (`remember` = store, `run_consolidation` = summarize, `recall` = retrieve) and ships three adversarial probe sets plus a canonical stale-context fixture that attributes a stale-recall failure to retrieve when store + summarize check out. Designed for `cargo test` and as a reusable library for downstream eval harnesses. New in v0.4.11.
- **Evidence-weighted conflict resolution** — resolve multi-agent conflicts using source reliability scoring
- **Memory-provenance signing on reads** — every `recall(..., with_provenance=True)` returns an HMAC-SHA256 receipt binding the cited records to a server-side key; supports key rotation. Verify offline from Python via `mnemo.provenance.verify_read_provenance`. New in v0.4.0-rc3.
- **Hardened MCP launcher** — `mnemo mcp-server --manifest <path>` runs a safe-spawn gauntlet (refuse inherited secrets, refuse `--config` argv injection, refuse untrusted parents) BEFORE engine state is constructed. Direct response to the OX-MCP "exfiltrate-then-act" disclosure (2026-04-24). All privileged knobs come from a chmod-restricted TOML manifest. New in v0.4.0-rc3.
- **DPDPA consent-token guard (library — not enforced by default)** — `mnemo-compliance::ConsentTokenGuard` validates a consent token's expiry / scope / revocation, and `MannsetuConsentSource` binds a DPB-registered consent manager. **The core engine does not require a consent token** — `engine.remember` performs no consent check; this is an opt-in guard a caller wires in front of writes, not a default gate. New in v0.4.0-rc3. _(See the enforcement table below.)_
- **MCP tool-catalog attestation (enforced at hardened boot)** — when the manifest sets `tool_catalog_pin_path`, `mnemo mcp-server` fingerprints the tools it is about to advertise (after any `[role_filter]`) and compares them to the pin **before serving stdio**: it refuses to start on any added or mutated tool and on a removed-only downgrade unless `allow_removed_drift = true`, and records every verdict as an `mcp_tool_catalog_drift` audit event. Generate a pin for your exact binary with `mnemo mcp-server --manifest <m> --print-catalog-pin`. On stdio the catalog is static after boot, so this one check is complete for the transport (it defends against a substituted binary, a hostile dependency that injects/renames a tool, and pin drift after an upgrade); it is not a per-request check and is only as strong as the manifest file's permissions. Direct response to arXiv 2604.20994 (function-hijacking via tool-list poisoning). New in v0.4.0, wired in v0.5.20. _(See the enforcement table below.)_
- **Cloudflare Mesh runtime adapter** — SPIFFE-style `MeshIdentity` + per-namespace `MemOp` ACL + `MeshAuditEnvelope` chained into the existing HMAC ledger. First OSS embedded memory DB to speak Cloudflare Mesh attestation natively. New in v0.4.0.
- **Code-mode WIT recall** — `mnemo:memory@0.4` WIT world plus a wasmtime-friendly host runner. Agents call `recall` as a sandboxed WASM function instead of a JSON tool envelope, dropping per-turn token cost ~96% on 200-turn LongMemEval_S samples. New in v0.4.0.
- **Decay-curve score lane** — `DecayLane` (Ebbinghaus + reinforcement) fuses with vector + BM25 + recency in the default recall path. `letta_mode` flag bypasses it for parity with Letta's published numbers. New in v0.4.0.
- **Agent-Deal ledger** — `mnemo-deal` crate ships a chained-HMAC `DealEnvelope` log with `verify_chain → DisputeReport`. v0.4.1 adds advertisement (`/.well-known/mnemo-deal-agent.json`) + advisory reputation (90-day half-life, per-dispute 10% penalty).
- **Markdown+Git working-set adapter** — `mnemo-md-sync` parses YAML-style frontmatter (`mnemo_id`, `tags`, `expires_at`) and provides `MdSyncSpec` + `SyncFlushPolicy` (PreferEngine / PreferDisk / NewerWins). New in v0.4.0.
- **Anthropic CMA-Memory compat shim** — `mnemo-cma` crate mounts, mirrors, and exports the Anthropic CMA-Memory beta filesystem (announced 2026-04-23). Every CMA write is bridged into the mnemo HMAC chain via `CmaSource::CmaBeta` markers. New in v0.4.1.
- **Agent behavioural-baseline exporter** — `mnemo-baseline` crate emits per-agent profiles in OpenTelemetry semconv 1.31 + OCSF 1.4 Application-Activity formats with z-score+EWMA drift detection; anti-leak regex test ensures payloads never carry memory contents. Plugs into the RSAC 2026 SOC telemetry gap. New in v0.4.1.
- **1M-context recall budget planner** — `mnemo-core::budget` adds `ContextBudget::for_model` + `plan_recall` covering `deepseek-v4-1m`, `claude-3.7-sonnet-1m`, `gpt-5.1-400k`, `gemini-2.5-pro-2m`; typed `FallbackStrategy`; property test asserts no model overflows. New in v0.4.1.
- **mnemo doctor + Grafana dashboard** — typed `DoctorReport` + `DoctorFix` recommendations and a committed `dashboards/mnemo-grafana.json` (schemaVersion 39) covering recall p50/p99, tool-catalog drift, HMAC continuity, code-mode token reduction. New in v0.4.1.
- **MCP role-aware tool filter (enforced when configured)** — the manifest `[role_filter]` block (`caller_roles`, `default = "allow_all" | "deny_all"`, per-tool `allow` / `deny`, deny-wins) is parsed and validated at startup **and attached to the hardened MCP server**: a denied tool is hidden from `tools/list` and rejected by `tools/call` with `-32601`. On stdio there is no per-call caller identity, so it acts as a server-wide tool denylist rather than per-caller RBAC; with no `[role_filter]` block present, every advertised tool stays reachable (unchanged). Aligned with the [2025-11-25 MCP authorization spec](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization). New in v0.4.2. _(See the enforcement table below.)_

### Security: what is and isn't enforced today

To report a suspected vulnerability privately, see [`SECURITY.md`](SECURITY.md):
supported versions, private reporting via GitHub security advisories, a
first-response time that can actually be kept, which backends are in scope, and
what is out of scope.

mnemo ships a lot of security *machinery*; not all of it is on the default
request path yet. This table is the honest picture — verified against the code,
not the roadmap. "Enforced" means a live code path rejects/acts; "library /
parsed-only" means the logic exists and is tested but nothing on the default
path calls it.

| Control | Enforced by default? | Where / how |
|---|---|---|
| **Network bearer-token auth (REST + gRPC)** | ✅ when `MNEMO_AUTH_TOKEN` is set — else **open + loud warning** | axum middleware / tonic interceptor → `401` / `UNAUTHENTICATED` |
| Per-record ACL / RBAC (private/shared/public scopes) | ✅ | `check_permission` on `recall` (shared scope) + `share` (admin) |
| Permission-safe ANN (post-filter to accessible ids) | ✅ | `recall` ANN path |
| Scoped, depth-limited delegation | ✅ | `delegate` + ACL checks |
| At-rest AES-256-GCM content encryption | ✅ when `MNEMO_ENCRYPTION_KEY` set | engine `remember`/`recall` |
| Hash-chain integrity + `verify` | ✅ | events + memories |
| Read-provenance HMAC receipt | ✅ opt-in per call (`with_provenance=true`) | `recall` |
| **Write-provenance + FORGET BY PROVENANCE** | ✅ — every REMEMBER/SHARE records who wrote it, under what capability, in what session; hash-chained (tamper-evident) and queryable by memory/principal/session; revocable in one call by principal or session | engine `remember`/`share` write path (DuckDB + PostgreSQL); REST `/v1/provenance/*`, MCP `mnemo.provenance` + `mnemo.forget_by_provenance`; see below |
| **Opaque-reasoning-payload write flag** | ✅ warn-and-record — on REMEMBER, content with the SHAPE of a provider opaque reasoning payload (arXiv:2608.09867) is flagged `opaque_reasoning_payload` on its provenance (hashed in, tamper-evident) and stored, not rejected; sweeps via FORGET BY PROVENANCE. Shape only — **not** proof of a secret; never decodes | `remember` write path → `WriteProvenance.flags`; [`docs/security/opaque-reasoning-payloads.md`](docs/security/opaque-reasoning-payloads.md) |
| Memory-poisoning anomaly + quarantine | ✅ — provenance-aware recall + a published **ASI06 resistance micro-bench** (100.0% vs canonical query-only MINJA, Wilson 95% [98.1%, 100.0%], n=200; 0% vs marker-free paraphrase — honest limitation) | `remember` + `recall` quarantine filter; [`docs/security/ASI06.md`](docs/security/ASI06.md) |
| **Forged-reasoning defense** (reasoning-provenance trust filter) | ✅ opt-in — a real-embedder bench drives planted fabricated-chain-of-thought ASR **100% → 0%** (`nomic-embed-text`, Wilson 95% ASR_on [0.0%, 3.1%], n=120) at **0/180 = 0%** benign false-quarantine [0.0%, 2.1%] | `RecallRequest.reasoning_trust` (`retrieval::ReasoningTrustPolicy`) enforced in `recall`'s `passes_filters`; [`bench/forged_reasoning/`](bench/forged_reasoning/) |
| Append-only audit-log trigger | ✅ on PostgreSQL | DB trigger |
| **MCP role-filter** (manifest `[role_filter]`) | ✅ when a `[role_filter]` block is present and not a no-op — a denied tool is hidden from `tools/list` and rejected by `tools/call` with `-32601`; on stdio there is no per-call caller identity, so this is a **server-wide tool denylist, not per-caller RBAC**. No block → every advertised tool reachable (unchanged) | `mnemo-mcp::role_filter` dispatch, attached by the `mnemo-cli` hardened server; `hardened_mode_attaches_role_filter` (CLI) + `role_filter_*` (library) tests |
| **MCP tool-catalog attestation** | ✅ when a `[tool_catalog_pin]` block is present — the hardened server fingerprints the tools it will advertise (post role-filter) and, **before serving stdio**, refuses to start on any added/mutated tool (and on removed-only drift unless `allow_removed_drift`); every verdict is an `mcp_tool_catalog_drift` audit event. On stdio the catalog is static after boot, so this one boot-time check is complete for the transport — **not per-request**, and only as strong as the manifest file's permissions. No block → no attestation (unchanged) | `mnemo-cli::attest` wired in `run_mcp_server`; `hardened_mode_attests_tool_catalog` (CLI stdio) + `attest` unit tests; generate a pin with `--print-catalog-pin` |
| **Consent-token-per-write** | ❌ **library only** — core engine never calls it | `mnemo-compliance::ConsentTokenGuard` |
| Lease tokens (capability-leased reads) | ✅ **shipped, opt-in** via `--lease-ttl-seconds <N>` (requires `--capability-key`) — `mnemo.recall` mints a lease and `mnemo.forget_subject` refuses without a live one. All four ADR 0001 properties enforced: **freshness** (TTL), **causality** (only a recall mints one), **caller-binding** (the minting principal alone can spend it), and **subject scope** (it covers only the subjects the read returned — [#160](https://github.com/sattyamjjain/mnemo/issues/160)). Unattached, both tools behave exactly as before. This row read "not shipped — removed as dead code" until #159; the earlier removal was real, and so is this | `mnemo-mcp::lease` wired in `server.rs`; `lease.rs` unit tests + `tests/capability_leased_reads.rs` |
| Cloudflare Mesh / Agent-Deal / baseline exporter / CMA shim | ❌ standalone adapter crates — not invoked by the running server | `mnemo-mesh` / `mnemo-deal` / `mnemo-baseline` / `mnemo-cma` |

#### Bearer-token auth (the floor)

The REST and gRPC servers are **unauthenticated unless you set a shared secret**:

```bash
export MNEMO_AUTH_TOKEN="$(openssl rand -hex 32)"
```

With it set, every REST request (except `GET /v1/health` and CORS preflight)
must send `Authorization: Bearer <token>` or get `401`; every gRPC RPC must send
an `authorization` metadata value or get `UNAUTHENTICATED`. The token is compared
in constant time. With it **unset**, both servers run open and log a warning on
every startup — they never silently serve an unauthenticated memory database.
This is a single operator-held secret (the floor for "don't run an open memory
server"), distinct from the per-record ACL/RBAC layer above; it is not user
accounts, scopes, or rotation.

#### Write provenance & FORGET BY PROVENANCE

Every `remember` and `share` records a **write-provenance** entry: the writing
principal, the capability it was authorised under (if any), the session/trace id,
the operation, a timestamp, and a SHA-256 `content_hash` chained to the previous
entry's hash. The chain is **tamper-evident** (`verify_provenance_chain`), the
records are **queryable** (memory ID → provenance; principal/session → everything
it wrote), and a principal or session can be **revoked in one call**. Revocation
removes the memories but not their provenance — the audit trail survives, because
wiping is not remediation.

The authority field references a real **capability** (`model::capability`, a
minimal HMAC-signed `{principal, scope, expiry}` token — part of [#126](https://github.com/sattyamjjain/mnemo/issues/126)),
not a recorded string. mnemo stays standalone: no external audit dependency.

**REST**

```bash
# Who wrote this memory?
curl -s localhost:8080/v1/memories/$ID/provenance
# Everything a principal / session wrote
curl -s localhost:8080/v1/provenance/principal/alice
curl -s localhost:8080/v1/provenance/session/sess-42
# Tamper-evidence over the chain
curl -s localhost:8080/v1/provenance/verify
# FORGET BY PROVENANCE — revoke everything alice wrote
curl -s -XPOST localhost:8080/v1/provenance/forget \
  -H 'content-type: application/json' \
  -d '{"principal":"alice","strategy":"hard_delete"}'
```

**MCP** — tools `mnemo.provenance` (read by `memory_id` | `principal` | `session_id`)
and `mnemo.forget_by_provenance` (revoke by `principal` | `session_id`).

**SDKs** — all three expose read **and** cleanup (an SDK that cannot clean up is
not shipping the feature):

```python
# Python
prov = client.write_provenance(memory_id)          # who wrote it
client.writes_by_principal("alice")                # everything alice wrote
client.forget_by_principal("alice", "hard_delete") # FORGET BY PROVENANCE
```

```typescript
// TypeScript
const prov = await client.getMemoryProvenance(id);
await client.writesByPrincipal("alice");
await client.forgetByProvenance({ principal: "alice", strategy: "hard_delete" });
```

```go
// Go
prov, _ := client.MemoryProvenance(id)
_, _ = client.WritesByPrincipal("alice", 0)
_, _ = client.ForgetByProvenance(mnemo.ForgetByProvenanceInput{Principal: ptr("alice")})
```

### Memory curation interop (Dreams, Routines, and substrate primitives)

Anthropic's [Dreams Research Preview](https://platform.claude.com/docs/en/managed-agents/dreams) (surfaced 2026-05-06 at Code w/ Claude SF) is a managed-agent feature that "lets Claude reflect on past sessions to curate an agent's memory and surface new insights." Its companion [Routines doc](https://code.claude.com/docs/en/routines) describes the long-horizon agents that *consume* curated memory. mnemo's REMEMBER / RECALL / FORGET / SHARE primitives, envelope provenance, and AES-256-GCM at-rest encryption are the substrate any such curator reads from and writes through — Dreams owns *what to curate*, mnemo owns *how to durably store with audit trail*. The two surfaces are complementary, not substitute.

**Honest framing:** the Dreams API itself is a Research Preview behind a Request-access form, and **mnemo does NOT today ship an Anthropic-API adapter**. Today's anchor is substrate-level interop documentation, not integration. A `mnemo-dreams` adapter crate is plausible if/when the API exits Research Preview, but is explicitly NOT in scope for v0.4.x. See [`docs/comparisons/anthropic-dreams.md`](docs/comparisons/anthropic-dreams.md) for the curator-action ↔ mnemo-primitive layering table.

## Why mnemo when Cloudflare Agent Memory exists?

Cloudflare announced Agent Memory GA during [Agents Week (2026-04-30)](https://www.cloudflare.com/agents-week/updates/),
followed by Workers AI inference, Email Service beta, and an Agents
SDK preview. It is the closest hosted competitor to mnemo.

mnemo is an embedded, cryptographically-audited, replayable memory
the regulator can inspect offline. Cloudflare optimises recall
throughput on the edge runtime; mnemo optimises a memory whose every
write is HMAC-chained, every read is provenance-signed, and whose
storage layer survives outside any cloud's audit boundary.

Honest concession: on per-recall p50 against the Workers KV+Vectorize
backend, edge-recall throughput likely favours Cloudflare. mnemo's
axis is provenance, chain replay, point-in-time `as_of`, evidence-
weighted conflict resolution, DPDPA / GDPR subject erasure with audit
preservation, and the v0.4.2 MCP role-aware tool filter — surfaces
that matter when an auditor or regulator must reconstruct exactly what
an agent saw and decided, three months later, without depending on a
cloud account staying live.

A full bench harness against Cloudflare Agent Memory (a planned
`mnemo-bench-cf` crate) was scoped but **has not been built** — it is not a
workspace member and the numbers have not been run.
[`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md)
documents the differentiation scenario list with empty-bench
placeholders so the comparison's contract is explicit.

Retrieval-strategy framing matters here too: [arXiv 2605.15184](https://arxiv.org/abs/2605.15184)
(Sen et al., May 2026) measured BM25 keyword retrieval outperforming
pure vector retrieval on its experiment-1 corpus inside an agent
harness. mnemo's documented default — hybrid RRF over BM25 + vector
+ graph + recency — is already hedged against the vector-first
default the paper questioned. v0.4.4 adds a typed
`RetrievalMode::HarnessAware { harness, format }` variant that lets
the response envelope be reshaped per agent harness (Claude Code,
Codex, Gemini CLI, Chronos, generic) without changing which records
the substrate retrieves. See
[`docs/research/grep-vs-vector-2605.15184.md`](docs/research/grep-vs-vector-2605.15184.md)
for the composition anchor + the explicit non-overclaim disclaimer.

Outcome diffing — reconstructing the artifact's full provenance from
append-only events — is the third trust wall in production agent
systems (alongside aligned-by-training intent and policy-mediated
action). The DELEGATE-52 delegation-corruption result
([arXiv 2604.15597](https://arxiv.org/abs/2604.15597), surfaced on
Hacker News 2026-05-09) puts a 25% baseline on the corruption rate
this layer needs to detect. mnemo's append-only event log + snapshot
substrate is the layer that lets a downstream auditor reconstruct any
artifact's full plan / input / trace / output tetrad and diff against
what the primary agent's plan asked for —
see [`docs/research/delegate52-2604.15597.md`](docs/research/delegate52-2604.15597.md)
for the operator recipe and the explicit non-overlap callout.

### Prior art — the release decision mnemo does not make

**Governed Persistent Memory** ([arXiv:2608.12476](https://arxiv.org/abs/2608.12476),
Guodong Xu, 2026-08-12) is the clearest academic statement of a problem mnemo has
only partly solved, so it is cited here rather than left out.

Its argument: agent memory is wrongly modelled as select–store–retrieve, because
**retrieval never decides whether a contradictory, superseded, retracted, deleted
or stale record may support an outgoing claim**. It proposes a bitemporal
state-transition model with source-bound admission, derived lifecycle state, and
fail-closed structured release, expressed as five executable clauses. On its
hash-frozen 3,600-case GPM-ReleaseBench, the strongest of three *complete* simple
policies matches only **1,800/3,600** and makes unmatched releases on **50% of
violation cases**; in its sealed service evaluation the governed lane is correct
on **2,400/2,400** clusters against **600/2,400** for ungoverned local
Qwen2.5-7B. Those are bounded contract-conformance results by the paper's own
framing, not open-world accuracy — the 7B figure is the ungoverned comparison,
not a claim about model accuracy.

**Where mnemo lands.** Some of the five clauses ship, some ship in weaker partial
forms, and some are not implemented at all — one of them in direct conflict with
a shipped feature. Which clause is in which state is deliberately **not** restated
here, because a second copy is a copy that drifts. It lives in exactly one place,
is generated from a manifest, and every row is asserted against this tree by
`crates/mnemo-compliance/tests/gpm_clause_manifest.rs`:

> **[The clause-by-clause table →](docs/research/governed-persistent-memory-2608.12476.md#where-mnemo-actually-lands)**

The structural gap underneath all five: **mnemo does not implement GPM's
bitemporal derived-lifecycle-state model.** There is bitemporal machinery in
[`mnemo-graph`](crates/mnemo-graph) and `as_of` in core, but no state machine
derives per-record releasability from transitions, and mnemo's release posture is
fail-open — `RECALL` returns records and mnemo's involvement ends. An unforgeable
ledger of a bad release is still a bad release. That is the gap, and it is not
scheduled; the page above says what adoption would actually cost.

### Project Think — loop vs. ledger

Cloudflare extended this story on [2026-05-04](https://blog.cloudflare.com/project-think/)
with **Project Think**, a runtime story for AI agents built on Workers
+ DO Facets — the *durable agentic loop* itself. Project Think is
upstream of mnemo's surface: it owns where the agent runs and how the
loop survives a Worker restart. mnemo owns whether the writes that loop
emits are cryptographically chained, replayable months later, and
inspectable without a Cloudflare account.

These are **complementary, not substitute, surfaces.** An operator can
run their durable loop on Project Think + DO Facets and chain every
memory write into mnemo's HMAC ledger; the bench crate that compares
*Cloudflare Agent Memory vs mnemo as a memory store* does not redo
itself for *Project Think as a runtime vs mnemo as a memory ledger* —
the latter is a layering question, not a benchmark. See
[`docs/comparisons/cloudflare-project-think.md`](docs/comparisons/cloudflare-project-think.md)
for the full layering table and where each side wins.

## Examples

The `examples/` directory contains working integration examples for all major agent frameworks:

| Example | Framework | Language |
|---------|-----------|----------|
| [`openai_agents_example.py`](examples/openai_agents_example.py) | OpenAI Agents SDK | Python |
| [`langgraph_mcp_example.py`](examples/langgraph_mcp_example.py) | LangGraph + MCP | Python |
| [`crewai_mcp_example.py`](examples/crewai_mcp_example.py) | CrewAI + MCP | Python |
| [`google_adk_example.py`](examples/google_adk_example.py) | Google ADK | Python |
| [`agno_example.py`](examples/agno_example.py) | Agno | Python |
| [`pydantic_ai_example.py`](examples/pydantic_ai_example.py) | Pydantic AI | Python |
| [`autogen_example.py`](examples/autogen_example.py) | AutoGen | Python |
| [`smolagents_example.py`](examples/smolagents_example.py) | HuggingFace Smolagents | Python |
| [`strands_agents_example.py`](examples/strands_agents_example.py) | AWS Strands Agents | Python |
| [`semantic_kernel_example.py`](examples/semantic_kernel_example.py) | Microsoft Semantic Kernel | Python |
| [`llama_stack_example.py`](examples/llama_stack_example.py) | Meta Llama Stack | Python |
| [`dspy_example.py`](examples/dspy_example.py) | DSPy | Python |
| [`camel_ai_example.py`](examples/camel_ai_example.py) | CAMEL AI | Python |
| [`browser_use_example.py`](examples/browser_use_example.py) | Browser Use | Python |
| [`basic_memory.py`](examples/basic_memory.py) | Direct PyO3 | Python |
| [`mastra_example.ts`](examples/mastra_example.ts) | Mastra | TypeScript |
| [`vercel_ai_sdk_example.ts`](examples/vercel_ai_sdk_example.ts) | Vercel AI SDK | TypeScript |

## CLI Options

```
mnemo [OPTIONS] [COMMAND]

Options:
  --db-path <PATH>              Database file path [default: mnemo.db] [env: MNEMO_DB_PATH]
  --openai-api-key <KEY>        OpenAI API key [env: OPENAI_API_KEY]
  --embedding-model <MODEL>     Embedding model [default: text-embedding-3-small] [env: MNEMO_EMBEDDING_MODEL]
  --dimensions <DIM>            Embedding dimensions [default: 1536] [env: MNEMO_DIMENSIONS]
  --agent-id <ID>               Default agent ID [default: default] [env: MNEMO_AGENT_ID]
  --org-id <ID>                 Organization ID [env: MNEMO_ORG_ID]
  --onnx-model-path <PATH>      ONNX embedding model path (local inference) [env: MNEMO_ONNX_MODEL_PATH]
  --rest-port <PORT>            Enable REST API on this port [env: MNEMO_REST_PORT]
  --postgres-url <URL>          Use PostgreSQL backend [env: MNEMO_POSTGRES_URL]
  --encryption-key <HEX>        AES-256-GCM encryption key (64 hex chars) [env: MNEMO_ENCRYPTION_KEY]
  --capability-key <HEX>        HMAC key for per-request capabilities (ADR 0002) [env: MNEMO_CAPABILITY_KEY]
  --capability-key-id <ID>      Key id recorded in issued capabilities [default: default] [env: MNEMO_CAPABILITY_KEY_ID]
  --http-port <PORT>            Serve MCP over authenticated Streamable HTTP instead of stdio
                                (needs --features http-transport; requires --capability-key) [env: MNEMO_HTTP_PORT]
  --lease-ttl-seconds <SECS>    Capability-leased reads (#126): recall mints a lease, forget_subject
                                requires one (0 = disabled) [default: 0] [env: MNEMO_LEASE_TTL_SECONDS]
  --idle-timeout-seconds <SECS> Auto-shutdown after idle period (0 = disabled) [default: 0] [env: MNEMO_IDLE_TIMEOUT]

Commands:
  baseline    Train the per-agent embedding-space baseline used by the z-score
              outlier detector (v0.3.3, Task A).
  mcp-server  Start the MCP STDIO server in hardened mode using a TOML manifest
              (v0.4.0-rc3, Task B2). Refuses inherited secrets / argv injection /
              untrusted parents BEFORE engine state is constructed. Privileged
              knobs come from the manifest; key material reaches the binary via
              a chmod-restricted keystore file. See
              `examples/mcp-server/manifest.toml` for an annotated reference.
  eval        Replay a JSONL dataset of {query, expected} rows against an
              in-memory engine and emit a per-row latency / top-k JSONL report
              (v0.4.0-rc3, Task B6). Defaults to the bundled LongMemEval_M
              sample under `crates/mnemo-core/benches/data/longmemeval_m.jsonl`.
              Pass `--with-provenance` + `--provenance-key-hex <hex>` to also
              measure the HMAC-receipt overhead.
  bench       Run a measurement-only benchmark (v0.4.9). Subcommand:
                embeddings  Measure every configured embedding backend
                            (nDCG@10, recall@10, p50/p95 latency, throughput)
                            and recommend the highest-nDCG backend whose p95 ≤
                            the SLO. Flags: --slo-ms <MS> [default: 50],
                            --dimensions <DIM> [default: 384],
                            --latency-samples <N> [default: 32].
              No retrieval defaults / RRF weights change.
  compliance  Compliance primitives (mnemo-compliance). Subcommand:
                retention   Print a processing-log retention-conformance profile
                            (DPDP Rules 2025 / EU AI Act Art.19 / HIPAA
                            §164.312(b)) and gate it against the active backend's
                            append-only floor, failing loud if it cannot honour
                            it. Flags: --profile <dpdp|eu-ai-act-art19|hipaa>
                            [default: dpdp], --floor-days <N> (override the
                            legal-minimum floor).
  capability  Mint per-request capabilities (ADR 0002). Subcommand:
                issue       Issue a signed capability. Flags: --key <HEX>
                            (must match the server's --capability-key),
                            --key-id <ID> [default: default], --principal <ID>,
                            --scope "<tokens>" (`role:<id>` entries become RBAC
                            roles; others are opaque scopes), --ttl-seconds <N>,
                            --format <bearer|json> [default: bearer].
```

### Per-request identity (ADR 0002)

By default the caller is the boot-time `--agent-id`: one process, one caller,
which is exactly right on stdio. Set `--capability-key <hex>` and a request may
instead carry a signed capability whose principal becomes the caller identity
**for that one call** — in `_meta["dev.mnemo/capability"]` on any transport, or
as `Authorization: Bearer <base64url>` over HTTP.

```bash
# One key for the server and for minting.
export MNEMO_CAPABILITY_KEY=$(openssl rand -hex 32)

# Serve MCP over authenticated Streamable HTTP (needs --features http-transport).
mnemo --capability-key "$MNEMO_CAPABILITY_KEY" --http-port 8080

# Mint a caller's token.
mnemo capability issue --key "$MNEMO_CAPABILITY_KEY" \
  --principal alice --scope "role:reader" --ttl-seconds 900
```

A capability that cannot be verified — malformed, forged, expired, unknown key,
or presented to a server holding no key — is **rejected**, never quietly
downgraded to the boot identity. Over HTTP a request carrying **no** capability
is also rejected: the boot fallback is only sound on a transport where the
operator is the sole possible caller, and on a network port it would let any
client that can reach it act as the operator.

> The HTTP transport binds `127.0.0.1` and does not terminate TLS. Put it behind
> a reverse proxy for anything beyond local use.

### Capability-leased reads (#126)

`--lease-ttl-seconds <N>` binds destructive erasure to a read the same caller
just performed, breaking the OX-MCP "exfiltrate-then-act" chain. With it on,
`mnemo.recall` returns a `lease` and `mnemo.forget_subject` requires it:

```jsonc
// mnemo.recall result gains — `subjects` lists what this read actually covered:
"lease": { "token": "01a0…", "agent_id": "alice",
           "scopes": ["forget_subject"], "subjects": ["s1"], "ttl_seconds": 60 }

// mnemo.forget_subject then requires:
{ "subject_id": "s1", "lease_token": "01a0…" }
```

A lease is refused if it is expired, names the wrong scope, was never issued,
**belongs to a different caller** (the replay case it exists to stop), or
**does not cover the subject being erased**. Requires `--capability-key`:
without per-request identity every caller shares the boot agent id, so every
lease would validate for everyone.

**Off by default**, because it changes the contract of two shipped tools.

**All four of [ADR 0001](docs/adr/0001-capability-leased-reads.md)'s properties
are enforced** — freshness, causality, caller-binding, and subject scope. The
`subjects` set is read off the `subject:` tags of the records the recall
*returned*, not inferred from the query, so an injected "forget everything about
bob" cannot ride a legitimate read of `alice`. A read that surfaced no
subject-tagged record mints a lease that authorises **no** erasure — the empty
set means nothing, never everything.

> **What it still does not defend:** a single caller driven through both steps.
> If the same principal is induced to recall subject `s1` and then erase it, the
> lease is issued and spent legitimately. Narrowing raises the cost — the
> injection must now steer a read of the very subject it wants erased — but the
> lease breaks *cross-principal* replay and *stale* authority, not an agent
> acting against its own interest throughout.

## Architecture

```
┌──────────┐  ┌───────────┐  ┌──────────┐  ┌──────────┐
│MCP Client│  │REST Client│  │  gRPC    │  │  psql    │
│ (stdio)  │  │  (HTTP)   │  │          │  │ (pgwire) │
└────┬─────┘  └─────┬─────┘  └────┬─────┘  └────┬─────┘
     │              │              │              │
     ▼              ▼              ▼              ▼
┌────────────────────────────────────────────────────────┐
│                    MnemoEngine                          │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │ Remember │ │  Recall  │ │ Forget/  │ │Checkpoint│ │
│  │ Pipeline │ │ Pipeline │ │Share/... │ │/Branch/  │ │
│  │          │ │  (RRF)   │ │          │ │Merge     │ │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ │
│       └─────────────┴────────────┴────────────┘       │
│                         │                              │
│  ┌──────────────────────▼──────────────────────────┐  │
│  │          StorageBackend (trait)                   │  │
│  │   ┌──────────┐              ┌─────────────┐     │  │
│  │   │  DuckDB   │              │  PostgreSQL  │     │  │
│  │   └──────────┘              └─────────────┘     │  │
│  └──────────────────────────────────────────────────┘  │
│                                                         │
│  ┌────────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐ │
│  │VectorIndex │ │FullText  │ │Embeddings│ │Encrypt │ │
│  │USearch/PG  │ │ Tantivy  │ │OpenAI/   │ │AES-256 │ │
│  │            │ │          │ │ONNX/Noop │ │GCM     │ │
│  └────────────┘ └──────────┘ └──────────┘ └────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Deployment

### Docker

```bash
docker build -t mnemo .
docker run -p 8080:8080 -e OPENAI_API_KEY=sk-... mnemo --rest-port 8080
```

### Kubernetes (Helm)

```bash
helm install mnemo deploy/helm/mnemo \
  --set env.OPENAI_API_KEY=sk-... \
  --set env.MNEMO_REST_PORT=8080
```

The Helm chart includes: Deployment, Service, ConfigMap, Secret, PVC, HPA, and Ingress templates.

### Cloudflare Workers deploy template (design anchor)

> **Status:** *design anchor*, not a shipped template. The `deploy/cloudflare/` scaffold is parked for v0.4.3 follow-up. This section documents the contract that scaffold will produce against — see [`docs/src/integrations/cloudflare-workers-deploy.md`](docs/src/integrations/cloudflare-workers-deploy.md) for the full design note.

[Cloudflare Durable Object Facets](https://blog.cloudflare.com/durable-object-facets-dynamic-workers/) (open beta, 2026-04-30) lets a single Worker dynamically load Durable Object classes, each with its own SQLite database. That's the per-tenant embedded-substrate shape mnemo already runs on DuckDB-per-agent — making Workers the natural managed runtime for an mnemo MCP server when you don't want to operate the box yourself.

The intended layout (single Worker, one DO Facet per tenant, mnemo as the MCP-over-HTTP entrypoint):

```toml
# wrangler.toml (sketch — not yet shipped under deploy/cloudflare/)
name = "mnemo-mcp-worker"
main = "dist/worker.js"

[[durable_objects.bindings]]
name = "MNEMO_TENANT"
class_name = "MnemoTenantFacet"
# DO Facet — each instance gets its own SQLite-backed storage
# matching mnemo's embedded DuckDB-per-agent contract.
```

What stays Rust-native vs. crosses the JS boundary, the file-format compatibility story (mnemo writes DuckDB; the Workers Facet exposes SQLite — a planned bench crate would quantify the gap), and which mnemo surfaces require the operator-held HMAC keystore vs. which can run inside the Worker — all in [`docs/src/integrations/cloudflare-workers-deploy.md`](docs/src/integrations/cloudflare-workers-deploy.md). The Cloudflare-vs-mnemo bench numbers would ship with that `mnemo-bench-cf` crate, which **has not been built** — it is not a workspace member and those numbers have not been run.

## Development

```bash
# Run all tests (unit + integration + MCP + pgwire + REST + admin + gRPC + doctests)
cargo test --all

# Run tests for a specific crate
cargo test -p mnemo-core
cargo test -p mnemo-mcp

# Run integration tests only
cargo test -p mnemo-core --test integration_test

# Lint and format
cargo clippy --all-targets --all-features
cargo fmt --all

# Run benchmarks
cargo bench -p mnemo-core

# Build with optional features
cargo build -p mnemo-core --features onnx     # ONNX local embeddings
cargo build -p mnemo-core --features s3        # S3 cold storage
cargo build -p mnemo-cli --features postgres   # PostgreSQL backend

# Build Python SDK (requires maturin, NOT cargo build)
cd python && maturin develop

# TypeScript SDK
cd sdks/typescript && npm install && npm test

# Go SDK
cd sdks/go && go test ./...
```

## Benchmarks

We run LoCoMo-MC10 and LongMemEval on every release. The canonical
results page is
[`docs/benchmarks/2026-04-25-mnemo-v0.3.4.md`](docs/benchmarks/2026-04-25-mnemo-v0.3.4.md)
— it carries reference rows for Hindsight (91.4% LongMemEval / 89.61%
LoCoMo, [source](https://benchmarks.hindsight.vectorize.io)) and
Letta-Filesystem (74.0%) plus the four mnemo retrieval strategies
side-by-side. The mnemo rows populate from the first authenticated
[nightly run](.github/workflows/benchmarks-nightly.yml) — ungated CI
forks read the empty rows and the workflow's first-run exception
keeps the regression gate honest. Earlier reports under
[`docs/benchmarks/`](docs/benchmarks/) carry the v0.3.0 / v0.3.1 floor
numbers from before the v0.3.3 Tantivy-default + LLM-judge fixes.

**Real-embedder memory-quality result → [`bench/RESULTS.md`](bench/RESULTS.md).**
The **earlier** measurement (see [the headline block above](#mnemo) for the
current one, which uses a different embedder and slice): mnemo's recall path with
a **real local semantic embedder** (`nomic-embed-text`, 768-dim, via Ollama, never
`NoopEmbedding`) over the bundled LongMemEval_M slice, held-out eval:
**semantic recall@1 = 0.739 [Wilson 95% 0.535, 0.875] at n=23 (MRR 0.805), at
~89% token reduction** vs.
dumping the full history (Engram-style lean-slice framing,
[arXiv:2606.09900](https://arxiv.org/abs/2606.09900) — a reference point, not a
parity claim).

First authenticated baseline, **2026-06-29 @ `640b7b1`** (recorded in
[`docs/benchmarks/baseline.json`](docs/benchmarks/baseline.json); `n=23`, mean of 5 seeds):

| mode | recall@1 | recall@3 | recall@5 | MRR | errored |
|---|---:|---:|---:|---:|---:|
| `bm25_only` (`lexical`) | 0.522 | 0.609 | 0.739 | 0.586 | ~1/23 |
| `vector_only` (`semantic`) | **0.739** | 0.826 | 0.826 | **0.805** | 0/23 |
| `rrf_hybrid` (`auto`, default weights) | 0.478 | 0.757 | 0.809 | 0.638 | ~1/23 |
| `rrf_hybrid` (`auto`, swept `v6_b1_r0_g0_k30`) | 0.696 | 0.783 | 0.826 | 0.765 | ~1/23 |

**One caveat:** single-run (5 in-process seeds, *not* restart-averaged); the
HNSW index + RRF fusion-weight selection sit near a noise floor (cf. *The FID
Lottery*) — the swept "best" hybrid config flips between runs, so treat sub-0.05
gaps as ties; `vector_only` is the one stable strong mode. This is *retrieval*
quality + token efficiency, **not** an LLM-judged QA-accuracy or leaderboard
claim (45-record LongMemEval_M, not _S; QA accuracy needs a generative LLM not
run here). Full tables + JSON: [`bench/RESULTS.md`](bench/RESULTS.md) and the
dated [`bench/locomo/results/`](bench/locomo/results/) report. Reproduce:
`ollama pull nomic-embed-text && cargo run --release -p mnemo-locomo-bench --bin semantic_recall_bench`.

**Indirect-query (implicit-association) retrieval + the orientation cache** — a
*different axis* from gold recall above: does the memory layer surface a decisive
fact when the query shares **no wording** with it and only bridges through world
knowledge? On a 30-row source-cited corpus (12 domains, `nomic-embed-text` 768-dim),
every fact is directly retrievable (`direct` recall@5 ≈ 1.00) but an **indirect**
query misses it ~13% of the time at k=5 (the blind spot). mnemo's opt-in,
constant-token **orientation cache** — warmed by the fact's own prior access —
surfaces the decisive entity in its bounded map ~93% of the time, lifting combined
surfacing to **1.00 @5** and recovering the full ≈ +0.13 gap **via the map, not by
re-ranking retrieval**. This is *retrieval surfacing*, **not** an LLM-answer score:
it is **not** a reproduction of InMind ([arXiv:2607.24368](https://arxiv.org/abs/2607.24368),
the framing) and **not** comparable to its 84.0% / 14.4% (which score an LLM's
answers with an in-context arm). 30 rows ⇒ wide Wilson CIs. Full write-up:
[`docs/benchmarks/implicit-association.md`](docs/benchmarks/implicit-association.md).
Reproduce: `ollama pull nomic-embed-text && cargo run --release -p mnemo-locomo-bench --bin implicit_association`.

### STATE-Bench — agentic enterprise-task memory (entry in progress)

[Microsoft **STATE-Bench**](https://github.com/microsoft/STATE-Bench) (MIT, pinned
at [`4efcbf2d`](https://github.com/microsoft/STATE-Bench/commit/4efcbf2d4fe60df04878859b692d9391f3d5b33a),
v0.8.1) measures whether an *agent* completes multi-step **enterprise workflows**
(travel / customer-support / shopping) better **with** memory — a *different axis*
from the retrieval numbers above (task completion, not gold recall). mnemo is the
**on-prem / embedded / auditable** entry: it plugs into the Agent Learning Track's
read-only `retrieve_learnings` hook through the public Python SDK, using the *same*
embedded DuckDB store that carries the hash-chained, tamper-evident audit log — so
this is evidence **for** the regulated-AI wedge, not a repositioning.

The harness is built and the mnemo half is smoke-tested offline
([`bench/state_bench/`](bench/state_bench/)). A **score is pending hosted-model
access**: STATE-Bench hard-locks its user simulator + judge to **GPT-5.4** and needs
an agent model (published baseline: GPT-5.1-no-memory, ~50–60% pass@1 —
[leaderboard](https://microsoft.github.io/STATE-Bench/leaderboard/)). We do **not**
publish a partial or faked number; when models are available,
[`bench/state_bench/run_state_bench.sh`](bench/state_bench/run_state_bench.sh) is
turnkey and fills
[`bench/state_bench/results/state_bench.md`](bench/state_bench/results/state_bench.md).
Not a "state of the art" claim.

### Where mnemo sits vs. the published systems (two different axes)

These are **not directly comparable** — they measure different things on different
data, and we publish only the row we actually ran. mnemo's number is a *retrieval*
metric (did the gold memory land in the top-k); Mem0/Letta publish *end-to-end QA
accuracy* (did an LLM, reading the retrieved memories, answer the question correctly).
mnemo has **not** run the end-to-end QA-accuracy pipeline — that needs a generative
LLM + judge, which this harness does not include.

| system | what's measured | metric | score | source |
|---|---|---|---:|---|
| **mnemo** (this repo, `vector_only`) | **retrieval** — gold recall@1 over a real local embedder | recall@1 (LongMemEval_M slice, n=23) | **0.739** [0.535, 0.875] | [`baseline.json`](docs/benchmarks/baseline.json), measured 2026-06-29 @ `640b7b1` |
| Mem0 | end-to-end QA accuracy (different axis) | LLM-judged answer accuracy (LongMemEval) | 93.4% | [reported](https://mem0.ai) |
| Letta | end-to-end QA accuracy (different axis) | LLM-judged answer accuracy (LoCoMo) | ~83% | [reported](https://docs.letta.com) |

**Read this honestly:** the 0.739 is *not* a win over 93.4% — they are on different
axes (recall vs. QA accuracy) and different datasets, so a head-to-head number does
not exist yet. What the mnemo row claims, and all it claims, is that the retrieval
layer surfaces the right memory ~74% of the time at k=1 with a real embedder, fully
reproducible from the command above. Closing the gap to a comparable QA-accuracy row
is tracked as the open end-to-end-eval work, not something we are reporting here.

For the long-form, axis-by-axis version of this against the three memory layers a
2026 search returns first, see
[`docs/comparisons/mem0-zep-letta.md`](docs/comparisons/mem0-zep-letta.md): where
mnemo is behind (QA accuracy, temporal knowledge graph) and where it is ahead
(on-prem, offline-verifiable tamper-evident audit).

### BEAM-style multi-hop / open-domain (reproduced vs. self-reported)

A separate **deterministic** number over mnemo's default hybrid recall
(`strategy="auto"`), offline embedder, no LLM — 100 queries × 5 pooled repeats/subtask,
top-5, seed `0xbea320262026`, via [`beam_bench`](bench/locomo/src/bin/beam_bench.rs):

| subtask | **reproduced** (this fixture) | self-reported (upstream) |
|---|---:|---:|
| `multi_hop` (answer only via a graph edge) | **0.6%** (3/500) [95% 0.2–1.7%] | — (BEAM reports one overall score) |
| `open_domain` (gold among same-schema distractors) | **68.6%** (343/500) [95% 64.4–72.5%] | Hindsight BEAM **64.1%** @ 10M tokens ([source](https://hindsight.vectorize.io/blog/2026/04/02/beam-sota)) |

**Not a ranking.** The reproduced figures are on a *small synthetic fixture* with a
lexical offline embedder and no LLM judge; upstream 64.1% is the real 10M-token,
LLM-graded BEAM. Self-reported memory scores are a vendor-run **upper bound** (not
independently reproduced) — so the columns are **not comparable**. The low `multi_hop`
figure is honest, not a bug: default `auto` RRF barely surfaces an answer reachable
only through a graph edge (the `graph` / `reconstruct` strategies target multi-hop).
No "first"/"best" claim. Full note + reproduce command: [`bench/RESULTS.md`](bench/RESULTS.md).

**Phase-aware cost attribution + agent-memory characterization scorecard (bench-only).** Anchored on [arXiv:2606.06448](https://arxiv.org/abs/2606.06448) (*Agent Memory: Characterization and System Implications of Stateful Long-Horizon Workloads*). The new [`phase_cost`](bench/locomo/src/bin/phase_cost.rs) bin splits every benchmark scenario's cost into the paper's **three phases** — **construction** (remember-path: embedding calls, prefill tokens, write latency), **retrieval** (recall-path: ANN + BM25 + graph + RRF latency, query tokens), and **generation** (downstream, *estimated* — mnemo does not generate) — and emits a per-phase table (tokens, wall-ms, $-estimate at configurable per-1K rates) per scenario. The `--scorecard-2606-06448` flag instead renders mnemo's PASS / PARTIAL / FAIL position against the paper's 10 §5 recommendations (quoted verbatim) as a 10-row table; mnemo currently scores **5 PASS · 5 PARTIAL · 0 FAIL** (PASS on the latency / feasibility / compaction recommendations R5/R7/R8/R9/R10; PARTIAL on the operator-side lifecycle-policy ones R1–R4/R6). Run via `cargo run --release --bin phase_cost -p mnemo-locomo-bench` (add `-- --scorecard-2606-06448` for the scorecard). **This is bench-only** — no access protocol (MCP / REST / gRPC / pgwire) and no retrieval default is touched; token counts are `ceil(chars/4)` estimates and the generation phase is never an LLM call. Sample per-phase table (default rates, 64 records / 16 queries, `NoopEmbedding`):

```
### Scenario `long-context` (64 records, 16 queries)

| Phase | Tokens | Wall-ms | $-estimate |
|---|---:|---:|---:|
| construction | 15360 | 1551.42 | 0.001306 |
| retrieval    |    96 |  157.76 | 0.000002 |
| generation   | 13792 |     n/a | 0.003912 |
| **total**    | 29248 |       — | 0.005220 |
```

Construction and generation dominate; retrieval is near-free — exactly the lifecycle-cost picture the paper insists operators measure (Recommendation 2). See [`bench/locomo/src/phase_cost.rs`](bench/locomo/src/phase_cost.rs) for the cost model + the full "what this is NOT" block.

**Embedding-backend selection bench + SLA-aware recommender (v0.4.9).** Anchored on [arXiv:2605.23618](https://arxiv.org/abs/2605.23618) (GE2 vs local encoders — quality + latency). New crate [`bench/embeddings`](bench/embeddings) measures every configured backend (Noop + bench-local hashing baseline always; OpenAI when keyed; ONNX when configured + feature-gated) for nDCG@10, recall@10, p50/p95 embed latency, and throughput at batch 1/8/32 on a 50-doc / 10-query labeled fixture; the recommender picks the highest-nDCG backend whose p95 ≤ the SLO and reports the nDCG gap vs the best-quality backend. Run with `mnemo bench embeddings --slo-ms <N>` or `cargo bench -p mnemo-embeddings-bench`. See [`bench/embeddings/README.md`](bench/embeddings/README.md) for the full "what this bench is NOT" block.

**First public LoCoMo number (v0.4.1, P0-1)** — full report at
[`docs/benchmarks/locomo-2026-04-28.md`](docs/benchmarks/locomo-2026-04-28.md).
mnemo joins the public LoCoMo board alongside MemMachine (84.87%,
2026-04-24) and Memori (81.95%, 2026-04-24); the harness ships at
[`bench/locomo`](bench/locomo) with a dual-judge variance gate
(GPT-5.1 + Claude-3.7 Sonnet) and runs nightly via
[`.github/workflows/locomo-nightly.yml`](.github/workflows/locomo-nightly.yml).
mnemo trades raw overall score for **temporal-slice strength + ~96% per-turn token cost** —
see the report for the honest pitch.

**Auto-Dreamer-shaped offline consolidation bench (v0.4.8).** Added 2026-05-25 at [`bench/locomo/src/bin/auto_dreamer_consolidation.rs`](bench/locomo/src/bin/auto_dreamer_consolidation.rs). Mirrors Auto-Dreamer's "smaller active bank, equal-or-better recall" axis against mnemo's existing `run_decay_pass` + `run_consolidation` path. Emits a Markdown report + a JSON summary (`active_bank_ratio`, `recall_pre`, `recall_post`) so the headline is citable. Defaults: 8 sessions × 25 facts × 5 trials. Run via `cargo run --release --bin auto_dreamer_consolidation -p mnemo-locomo-bench`. See [`bench/locomo/README.md`](bench/locomo/README.md#auto-dreamer-offline-consolidation--auto_dreamer_consolidation) for the full "what this bin is NOT" block.

**LongMemEval_M provenance overhead bench (v0.4.0-rc3, B3).** A
self-contained 45-record synthesized dataset ships at
[`crates/mnemo-core/benches/data/longmemeval_m.jsonl`](crates/mnemo-core/benches/data/longmemeval_m.jsonl)
(override with `MNEMO_LONGMEMEVAL_PATH=<path>` for the published
gated dataset). The
[`longmemeval_bench`](crates/mnemo-core/benches/longmemeval_bench.rs)
criterion target runs two arms — `recall_no_provenance` and
`recall_with_provenance` — so the per-recall HMAC-receipt overhead
is measurable in CI:

```bash
cargo bench -p mnemo-core --bench longmemeval_bench
```

## Documentation

- **mdBook**: `docs/` directory — run `mdbook serve docs` for local browsing
- **Compliance**: SOC 2 controls mapping and HIPAA safeguards at `docs/src/compliance/`
- **REST API**: `docs/src/rest-api.md`
- **Tool reference**: `docs/src/tools/` (one page per MCP tool)
- **Hardened MCP launcher**: [`docs/src/integrations/mcp-server.md`](docs/src/integrations/mcp-server.md) — manifest schema, threat model, systemd unit example
- **Time-travel debugger**: [`examples/time-travel-debugger/index.html`](examples/time-travel-debugger/index.html) — vanilla-JS UI that diffs recall results between two `as_of` timestamps; serve any way you like (`python3 -m http.server`)
- **LoCoMo report**: [`docs/benchmarks/locomo-2026-04-28.md`](docs/benchmarks/locomo-2026-04-28.md) — first public mnemo number alongside MemMachine + Memori with the honest temporal-slice + per-turn-token pitch
- **Grafana dashboard**: [`dashboards/mnemo-grafana.json`](dashboards/mnemo-grafana.json) — schemaVersion 39, drop straight into Grafana 11.5; covers recall p50/p99, tool-catalog drift, HMAC continuity, code-mode token reduction, baseline anomalies
- **Benchmarks**: `docs/benchmarks/`

## License

Apache-2.0
