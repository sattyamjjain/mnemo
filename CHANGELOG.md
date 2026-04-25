# Changelog

All notable changes to Mnemo are documented in this file.

## [0.4.0-rc1] - 2026-04-25

### Highlights

Release candidate stacking on top of v0.3.4. Lands three of the four
follow-on tasks from the 2026-04-25 prompt: the Graphiti-style
temporal-edge crate (Task A4 minimal), the Letta Conversations-style
shared-memory adapter (Task A5), and a partial close on the golden
DuckDB fixtures front (Task A7). Task A6 (Mem0g graph-extraction
toggle) waits for v0.4.0 final because it depends on Task A4's LLM
extractor leaving stub state — see deferred section.

### Added

- **`mnemo-graph` crate** (Task A4 minimal). New workspace member with
  a `TemporalEdge { src, dst, relation, valid_from, valid_to,
  confidence, recorded_at }` model, an async `GraphStore` trait, a
  `DuckGraphStore` impl creating `graph_nodes` + `graph_edges` tables
  with indexes on `(src, valid_from)` and `(dst)`, and a
  `graph_expand(seed, depth, as_of)` BFS that respects bitemporal
  validity. The 5 unit tests in
  `crates/mnemo-graph/tests/temporal_walk.rs` cover the headline
  bitemporal-supersession property: an `as_of` query *between* a
  fact and its supersession returns the original answer; an `as_of`
  query *after* returns the new one.
- **`MnemoLettaShared` adapter** (Task A5). New
  `python/mnemo/letta_adapter.py` implementing
  `attach`/`detach`/`list_participants`/`read`/`write` over Mnemo
  memories tagged `conversation:<id>` and `participant:<agent_id>`.
  Cross-participant writes within a 60-second window surface via
  `overlapping_writes_within()` for operator inspection; conflict
  resolution itself happens at recall time via Mnemo's existing
  evidence-weighted scoring. Example at
  `examples/letta_shared_conversation.py`.
- **Golden fixture v0.3.4** (Task A7 partial). Generator at
  `crates/mnemo-core/examples/gen_golden_fixture.rs`; committed
  fixture at `crates/mnemo-core/tests/golden/v0_3_4.mnemo.db`;
  round-trip test at
  `crates/mnemo-core/tests/migration_roundtrip.rs` asserting the
  fixture opens, gets stamped to `CURRENT_PERSISTENCE_VERSION = 4`,
  and round-trips exactly 5 records. v0.1.1 / v0.3.0 historical
  fixtures still missing — see [issue #38](https://github.com/sattyamjjain/mnemo/issues/38)
  comment for the gap analysis (the corresponding git tags don't
  actually exist on this repo).

### Changed

- Workspace version bumped 0.3.4 → 0.4.0-rc1.
- `Cargo.toml` workspace members extended with `crates/mnemo-graph`.

### Tests

- **+5** new Rust integration tests in
  `crates/mnemo-graph/tests/temporal_walk.rs` — supersession
  correctness, confidence-ordered outgoing edges, BFS depth bound,
  idempotent edge-close, extract-stub.
- **+11** new Python tests in `python/tests/test_letta_adapter.py` —
  attach/detach idempotency, participants metadata not duplicated,
  cross-participant overlap detection, content/source validation.
- **+1** new Rust integration test in
  `tests/migration_roundtrip.rs` — fixture round-trip + persistence
  stamp.
- 100 Python pass + 5 skipped (4 OpenAI-gated pre-existing + 1
  live-R2). All Rust crates green; `mnemo-graph` adds 5 unit-test
  passes to the count.

### Deferred to v0.4.0 final

- **Task A4 — full LLM-driven `TemporalEdge::extract`.** v0.4.0-rc1
  ships the `graph-extract` feature gate but the extractor itself
  returns an empty `Vec`. The prompt + ICL examples are still being
  tuned; shipping a half-tuned extractor would put bad edges in
  everyone's graphs.
- **Task A4 — `hybrid_rrf` 4th-signal integration.** The retrieval
  path doesn't yet fuse graph-expanded nodes into RRF; that
  integration needs the extractor to be live first to surface enough
  edges for the signal to matter.
- **Task A4 — MCP / REST / gRPC `graph_expand` tools.** The crate
  exposes the function; binding it to the wire-protocol surfaces is
  small additive work for v0.4.0 final.
- **Task A6 — Mem0g `with_graph_extraction(enabled, model)` toggle.**
  Skipped today because the underlying extractor is a stub. Lands
  with the extractor in v0.4.0 final.
- **Task A7 — historical fixtures `v0_1_1.mnemo.db` /
  `v0_3_0.mnemo.db`.** Blocked by absent git tags. See [#38 comment](https://github.com/sattyamjjain/mnemo/issues/38#issuecomment-4319897458).

### Sources

- [Graphiti repo (getzep)](https://github.com/getzep/graphiti)
- [Graphiti paper (arXiv:2501.13956)](https://arxiv.org/abs/2501.13956)
- [Letta — Letta-Code release (2026-04-06)](https://www.letta.com/blog/letta-code)
- [Mem0g paper (arXiv:2504.19413)](https://arxiv.org/abs/2504.19413)

## [0.3.4] - 2026-04-25

### Highlights

Patch release shipping the **v0.3.4 floor** from the 2026-04-25 prompt:
the public benchmark page laid out for Letta-parity comparison, the
Anthropic raw-API memory-tool 6-op server (`memory_20250818`), and a
Cloudflare R2 workspace backend that closes one third of issue #39.
Tasks A4–A7 (Graphiti, Letta-shared, Mem0g, golden DuckDB fixtures)
fold into the v0.4.0-rc1 stack landing by 2026-04-28.

### Added

- **`MnemoMemoryToolServer`** ([`python/mnemo/anthropic_memory_tool.py`])
  — full client-side handler for Anthropic's `memory_20250818` tool
  surface. Maps the six commands (`view`, `create`, `str_replace`,
  `insert`, `delete`, `rename`) onto Mnemo memories with the
  spec-pinned return strings, line-numbered file views, and recursive
  directory listing semantics. `managed_agents_beta=True` flips the
  `anthropic-beta: managed-agents-2026-04-01` header through
  `MnemoMemoryToolServer.beta_header()`. Path-traversal protection is
  required-and-enforced: every input must canonicalise under
  `/memories`, with `..` and URL-encoded sequences rejected
  pre-normalisation. Doc page at
  `docs/src/integrations/anthropic-memory-tool.md`.
  Source: [Anthropic memory-tool docs][memtool].
- **`CloudflareR2Workspace`** ([`python/mnemo/openai_sandbox/r2_workspace.py`])
  — R2-flavoured subclass of `S3Workspace`. Sets `endpoint_url=
  https://{account_id}.r2.cloudflarestorage.com`, `region="auto"`,
  `addressing_style="virtual"`. RemoteSnapshotSpec output carries
  `backend="r2"` so `MnemoSnapshotStore` dispatches correctly. Live-R2
  test gated on `R2_ACCOUNT_ID` / `R2_ACCESS_KEY_ID` /
  `R2_SECRET_ACCESS_KEY` / `R2_BUCKET` env vars; otherwise the moto
  S3 emulator stands in.
- **`docs/benchmarks/2026-04-25-mnemo-v0.3.4.md`** — canonical
  benchmark page with Letta-parity reference rows ([Hindsight 91.4 /
  89.61][hindsight], [Letta-Filesystem 74.0][letta], full-context
  72.9 floor) plus blank mnemo rows the nightly workflow populates on
  its first authenticated run. Wired into README "Benchmarks"
  section. Tracking issue **#44** for the first authenticated run.
- New extras `mnemo[anthropic-memory-tool]` (pulls `anthropic>=0.40`)
  and `mnemo[openai-sandbox-r2]` (pulls `boto3>=1.34`,
  `cryptography>=42`).

### Changed

- **`S3Workspace`** ([`python/mnemo/openai_sandbox/s3_workspace.py`]) —
  lift `endpoint_url`, `region`, `addressing_style`,
  `signature_version` into the constructor. All default to `None` so
  AWS-S3 behaviour is unchanged for existing call-sites; subclasses
  (`CloudflareR2Workspace`) read from these in `_build_default_client`.
  Spec output now uses `self.backend_name` (defaults `"s3"`,
  R2 sets `"r2"`) so `RemoteSnapshotSpec.backend` is correct out of
  the box.
- Issue **#39** rescoped to GCS + Azure Blob only after R2 landed in
  this release.

### Tests

- **+32 unit tests** in `python/tests/test_anthropic_memory_tool.py` —
  all six ops, every documented error string, path-traversal rejection
  (`..`, URL-encoded), beta-header toggle, and a fixture round-trip
  test that replays the canonical request shapes from the docs page
  through `MnemoMemoryToolServer.handle`.
- **+5 unit tests** in `python/tests/test_r2_workspace.py` — moto
  round-trip with `backend="r2"` spec assertion, S3-spec rejection,
  `account_id` validation, and a live-R2 opt-in test.
- All 91 Python tests pass + 5 skipped (4 OpenAI-gated pre-existing,
  1 live-R2). No Rust changes; Rust tests untouched at the v0.3.3
  count.

### Deferred to v0.4.0-rc1

- **Task A4** — Graphiti-style temporal-edge crate (`mnemo-graph`).
  Bitemporal `valid_from`/`valid_to`, `graph_expand` integrated into
  `hybrid_rrf` as a fourth signal, MCP/REST/gRPC tool surfaces.
- **Task A5** — Letta `Conversations`-style shared-memory adapter
  (`MnemoLettaShared`).
- **Task A6** — Mem0g-parity `with_graph_extraction(enabled, model)`
  toggle on `MnemoMem0Compat`.
- **Task A7** — Golden DuckDB persistence fixtures (issue #38).

### Out of scope today

- DuckLake v1.0 storage backend evaluation (issue #41) — bump
  `duckdb = "1.4" -> "1.5.2"` in a separate PR.
- TypeScript 6.0 migration (PR #26 held; tracked in #40).

### Sources

- [Anthropic memory-tool docs][memtool]
- [Anthropic — Claude Opus 4.7 release post](https://www.anthropic.com/news/claude-opus-4-7)
- [Letta — Letta-Code release](https://www.letta.com/blog/letta-code)
- [Letta — Benchmarking AI Agent Memory][letta]
- [Hindsight benchmarks][hindsight]
- [OpenAI — next evolution of the Agents SDK](https://openai.com/index/the-next-evolution-of-the-agents-sdk/)
- [Cloudflare R2 pricing & API](https://developers.cloudflare.com/r2/pricing/)

[memtool]: https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/memory-tool
[hindsight]: https://benchmarks.hindsight.vectorize.io
[letta]: https://www.letta.com/blog/benchmarking-ai-agent-memory

## [0.3.3] - 2026-04-24

### Highlights

Patch release focused on the three v0.3.2-deferred items named as the
v0.3.3 target (Tasks A + B + G of the 2026-04-24 prompt). Four Rust and
three TypeScript Dependabot PRs absorbed; TS 6.0 (#26) held for a
separate validation pass. No runtime API removed; every new knob is
opt-in and defaults to the v0.3.2 behaviour.

Six GitHub issues filed (#36–#41) tracking: Hindsight SOTA gap, full
MINJA-procedure harness, golden DuckDB fixtures, R2/GCS/Azure
workspace backends, TS 6.0 migration, and DuckDB 1.5.2 + DuckLake v1.0
evaluation.

### Added

- **Embedding z-score outlier detector** (Task A — closes v0.3.2
  deferred item). `crates/mnemo-core/src/anomaly/outlier.rs` with
  Mahalanobis-proxy scoring over a diagonal-covariance per-agent
  baseline trained via Welford's algorithm. `PoisoningPolicy` struct
  in `query/poisoning.rs` with `with_outlier_threshold(z)` enabling
  the gate; off by default, pinned `is_outlier = false` below
  `MIN_BASELINE_SAMPLES = 30`. `OUTLIER_SCORE_CONTRIBUTION = 0.5`
  added to anomaly score on fire so one outlier alone crosses the
  `is_anomalous >= 0.5` bar.
- **`embedding_baseline` storage table** (DuckDB + PostgreSQL JSONB).
  `StorageBackend::{get,insert_or_update}_embedding_baseline`.
  `CURRENT_PERSISTENCE_VERSION` bumped 3 → 4; pre-existing v0.3.2
  files auto-create the table on open.
- **`mnemo baseline --train --agent-id <id>`** CLI subcommand.
- **LLM-as-judge scorer** (Task B — closes v0.3.2 deferred item).
  `python/mnemo/benches/judge.py` with `LlmJudge` + `JudgeVerdict`;
  default model `claude-haiku-4-5-20251001`, override via
  `MNEMO_JUDGE_MODEL`. YES/NO/UNSURE contract with UNSURE counted as
  miss. Judge failures surface as `JudgeUnavailableError` so the
  runner falls back to `--judge=exact` with a warning rather than
  silently degrading.
- **`--judge=exact|llm`** flag on `mnemo.benches.locomo_runner`.
- **PyMnemoClient full-text default.** `python/src/lib.rs::MnemoClient::new`
  now attaches a persistent Tantivy full-text index by default
  (kwarg `with_full_text=True`). Fixes the v0.3.0–0.3.2 bug where
  `strategy="hybrid_rrf"` silently collapsed to vector-only because
  `full_text` was never wired at the Python boundary. New kwarg
  `with_noop_embedding=True` makes the Noop fallback explicit: set
  to `False` and the constructor raises rather than silently
  zero-vectoring.
- **Nightly benchmark regression gate.** `.github/workflows/benchmarks-nightly.yml`
  + `.github/scripts/check_bench_regression.py` fail CI on >3pp
  recall@10 drop vs `docs/benchmarks/baseline.json`. First-run
  exception: empty baseline lets the first authenticated run seed
  the reference point without a false-positive failure.
- **Security workflow.** `.github/workflows/security.yml` runs
  `cargo audit` + `cargo deny check advisories` on push / PR /
  nightly. Thirteen RustSec advisories catalogued with paragraph-level
  rationales in `.cargo/audit.toml` + `deny.toml`; the gate lights
  up on any NEW advisory not already documented.
- **`**/node_modules/` in `.gitignore`** — was missing, would have
  made any legitimate `git add sdks/typescript/` pull the entire npm
  install tree.

### Changed

- Dependabot batch absorbed:
  - `sha2` 0.10 → 0.11 (PR #28).
  - `criterion` 0.5 → 0.8 (PR #13).
  - `rand` 0.9 → 0.10 (PR #12) with a one-line `use rand::Rng`
    migration in `mnemo-compliance::audit::WorkspaceSigner::generate_ephemeral`.
  - `ndarray` 0.16 → 0.17 (PR #11), feature-gated under `onnx`.
  - `@modelcontextprotocol/sdk` 1.26.0 → 1.29.0 (PR #31).
  - `@types/node` 20.19.32 → 25.5.2 (PR #30).
  - `ts-jest` 29.4.6 → 29.4.9 (PR #29).
- `sdks/typescript/jest.config.js` now carries the standard
  NodeNext-style `.js` moduleNameMapper. Pre-existing breakage: the
  whole TS test suite failed to even load on main because Jest could
  not resolve `import ... from "./types.js"` against `types.ts`.
- PR #27 (the original rmcp 0.14 → 0.16 attempt) closed unmerged
  back in v0.3.2. The rmcp 1.3 landing happened via the workspace
  dep bump in commit `d4bad6b` as part of PR #35. This CHANGELOG
  entry exists because the v0.3.3 prompt asked for the path to be
  documented here — rmcp sits at 1.3 today; workflow captured.

### Tests

- **+6 unit tests** in `anomaly::outlier::tests` — train-from-records,
  no-embedding early-exit, in-distribution-not-flagged, far-OOD-flagged,
  noisy-baseline pin, dim-mismatch passthrough.
- **+1 integration test** `test_zscore_outlier_catches_semantic_drift`
  — asserts (1) no-baseline passthrough, (2) in-distribution probe
  not flagged, (3) 50σ-drift probe flagged with the z-score reason
  string surfaced.
- **+11 Python unit tests** in `python/tests/test_judge.py` —
  YES/NO/UNSURE parse, bullet-prefix tolerance, unparseable-line
  fallback, no-memories short-circuit, SDK-exception path,
  prompt-shape contract, content truncation, env-driven model
  override, frozen-dataclass contract.
- Full suite: Rust 170 pass (77 unit + 52 integration + all other
  crates) / Python 54 pass + 4 skipped (OpenAI-gated).

### Benchmarks

- `docs/benchmarks/2026-04-24-poisoning-outlier.md` — methodology
  doc for Task A. Publishes correctness of the detector (unit +
  integration green) but **declines** to publish TPR/FPR labelled
  as "MINJA" because the paper ships a procedure, not a corpus.
  Full attack-success-rate harness tracked as issue #37.
- `docs/benchmarks/2026-04-24-mnemo-v0.3.3.md` — Task B scaffolding
  + plan. Numeric recall@10 / MRR / latency for LoCoMo-MC10 and
  LongMemEval are deferred to the first nightly run authenticated
  with `ANTHROPIC_API_KEY` + `OPENAI_API_KEY` + `HF_TOKEN`; the
  code path is ready.

### Deferred to v0.3.4 / v0.4.0

- Graphiti-style temporal edge layer (Task C). Tracked separately.
- DuckLake v1.0 opt-in storage backend (Task D). Issue #41.
- R2 / GCS / Azure workspace backends (Task E). Issue #39.
- Anthropic Claude Opus 4.7 raw-API memory-tool adapter (Task F).
- Golden DuckDB fixtures `v0_1_1.mnemo.db` / `v0_3_0.mnemo.db`
  (carried forward from v0.3.2). Issue #38.
- Transitive fixes for the 13 ignored RustSec advisories — each
  owner-pinned to the next respective dep-bump PR (see
  `.cargo/audit.toml`).
- TypeScript 5.9 → 6.0 (PR #26 held). Issue #40.

## [0.3.2] - 2026-04-21

### Highlights

Closes every v0.3.1-deferred task: real MINJA poisoning numbers, real
S3 workspace backend, persistence format stamp, and the long-awaited
rmcp 0.14 → 1.3 upgrade with MCP resource exposure.

### Added

- **MINJA / InjecMEM indirect-injection detector** — new signal on
  `check_for_anomaly`: self-referential instruction markers
  ("remember this", "in the future, always", 13 total) fire only when
  the record arrived via `SourceType::Retrieval|Import` or a
  `source:web|document|email|third_party|retrieved` tag. Legitimate
  "please remember …" from user input is not flagged.
- **Quarantine replay** — `engine.replay_quarantine(agent_id, since)`
  returns every quarantined record with id / agent / content / reason /
  source_type / tags / created_at, chronologically ordered.
- **Public MINJA-style numbers** at
  `docs/benchmarks/2026-04-21-poisoning.md`: TPR 0.960, FPR 0.000, F1
  0.980 against a 50-prompt in-repo fixture modelled on
  arXiv:2503.03704. Clears both brief bars (TPR ≥ 0.85, FPR ≤ 0.05).
- **`mnemo.openai_sandbox` subpackage**
  (`pip install mnemo[openai-sandbox-s3]`):
    - `LocalSnapshotSpec` / `RemoteSnapshotSpec` — the GA
      `SnapshotSpec` split.
    - `WorkspaceSigner` + `dump_workspace` / `load_workspace` —
      Ed25519-signed manifest, per-file SHA-256 digests, symlink
      preservation (walks with `PurePosixPath`, records
      `{source, target}` pairs).
    - `S3Workspace` — real `boto3`-backed implementation of the workspace
      put / get / delete contract (`s3://<bucket>/<key_prefix>/files/...`).
    - Tamper detection fails closed on both manifest tamper (Ed25519
      `InvalidSignature`) and per-file tamper (`ValueError`).
- **Persistence format stamp** — new `mnemo_meta` table carries a
  `persistence_version` row (currently `3`). `run_migrations` stamps
  fresh files on first open; legacy v0.1.1 / v0.3.0 / v0.3.1 files get
  stamped the first time a v0.3.2 reader opens them.
  `CURRENT_PERSISTENCE_VERSION` exported for downstream tooling.
- **MCP resources** — the rmcp 1.3+ `list_resources` / `read_resource`
  handlers surface the 50 most recent memories as
  `mem://<uuid>` resources with `text/markdown` MIME. The server now
  advertises the `resources` capability as well as `tools`.

### Changed

- **rmcp 0.14 → 1.3** (satisfied by 1.5 on the current lockfile). The
  `ServerInfo` / `Implementation` / `ReadResourceResult` shapes moved to
  `#[non_exhaustive]` in the upstream crate; `MnemoServer::get_info`
  now builds `ServerInfo` through `Default::default()` + field
  assignment + `Implementation::from_build_env()` with the name and
  version overridden. Closes the PR #27 deferral.
- Two new `EventType` variants — `ReflectionCompleted`,
  `DreamReportIngested` — were already added in v0.3.1; no change here.

### Tests

158 Rust tests, 0 failed, including the new MINJA bench, quarantine
replay, persistence version stamp tests, and the resource-surface
storage contract test. 43 Python tests (5 new S3 workspace tests
including a moto-backed round-trip) — 43 pass, 4 skipped gracefully
when `OPENAI_API_KEY` is absent.

### Deferred to v0.3.3

- **Embedding z-score outlier detector** (part of Task 3) — needs a
  baseline-mean training pass on the corpus. Queued alongside the
  benchmark-harness `--train-baseline` step.
- **LLM-as-judge scoring** for LongMemEval's inferential gold answers.
  Will re-run the 2026-04-21 benchmark and lift the zero-recall floor.
- **R2 / GCS / Azure workspace backends**. Stubs remain in place behind
  the matching `mnemo[openai-sandbox-<backend>]` extras.
- **Golden DuckDB fixtures** (`v0_1_1.mnemo.db`, `v0_3_0.mnemo.db`).
  Generating a deterministic 0.1.1 file needs a pinned historical
  build; held for a dedicated follow-up.

## [0.3.1] - 2026-04-21

### Highlights

Honesty pass on top of v0.3.0: first public benchmark numbers, Auto Dream
cadence coordination, typed error surface for the Python client, and the
five documentation pages the v0.2.0/v0.3.0 acceptance checklists kept
promising. Four tasks from the v0.3.1 brief remain deferred to v0.3.2 —
listed below.

### Added

- **First public LoCoMo / LongMemEval numbers**
  (`docs/benchmarks/2026-04-21-mnemo-v0.3.0.md`). The harness runs; the
  numbers are floor values because two v0.3.0 bugs surfaced during the
  run (the Python `MnemoClient` does not attach a full-text index, and
  the default `NoopEmbedding` collapses semantic retrieval to noise).
  Report documents both root causes and opens four tracking items.
- **Auto Dream cadence coordination**. New `ReflectionMode::Coordinated`
  gate on `engine.run_reflection_pass_with_mode(agent_id, mode, force)`
  honours the same 24 h / 5-record cadence Auto Dream uses. Gate
  decisions surface as `SkipReason::{TooSoon, NotEnoughNewRecords}` on
  the returned report.
- **Auto Dream organization-report ingestion**. `parse_organization_report`
  parses the standard trailer (`Consolidated: N / Removed: M /
  Re-indexed: K`); `ingest_dream_reports` walks agent memories, emits
  one `EventType::DreamReportIngested` event per trailer, and marks
  `metadata.dream_report_ingested_at` for idempotency.
- **Typed `mnemo.availability` module** — `is_native_available()`,
  `native_build_hint()`, `installed_adapters()`. Replaces the opaque
  `AttributeError` adapters used to produce when the PyO3 extension
  wasn't built with a clean `MnemoClientUnavailable` error carrying the
  build hint.
- **`python -m mnemo doctor`** subcommand — prints Python + platform,
  native-extension status, and an adapter probe table. Exits 0 when
  the core client is available, 1 otherwise.
- **Five documentation pages** finally on `main`:
  `docs/src/integrations/claude-agent-sdk.md`,
  `integrations/openai-agents-ga.md`, `concepts/memory-tiers.md`,
  `compliance/dpdpa.md`, `compliance/eu-ai-act.md`. Wired into
  `docs/SUMMARY.md`. The memory-tiers page explicitly flags that
  `MemoryTier` is a type alias over `MemoryType`, not a separate field.

### Changed

- Two new `EventType` variants: `ReflectionCompleted` and
  `DreamReportIngested`. Both additive; hash-chain-linked.
- `claude_agent_sdk`, `openai_sessions`, and `openai_sessions_ga` adapter
  constructors raise `MnemoClientUnavailable` instead of a generic
  `ImportError`.
- The four integration tests in `test_claude_agent_sdk.py` that need
  real embeddings now skip when `OPENAI_API_KEY` is unset, rather than
  failing opaquely under `NoopEmbedding`.

### Deferred to v0.3.2

Documented in the v0.3.1 roadmap; not regressions from v0.3.0.

- **Task 3 — MINJA poisoning benchmark + quarantine replay.** The
  poisoning detector exists in `mnemo-core` but has no published TPR
  / FPR numbers against the MINJA fixture.
- **Task 4 — Real S3 snapshot backend + `SnapshotSpec` split.** v0.3.1
  ships the local workspace backend; S3/R2/GCS/Azure remain stubs that
  raise `NotImplementedError` pointing at the matching `mnemo[...]`
  extras.
- **Task 5 — Persistence format stability + migration tests.** Adding
  `persistence_version` to the `mnemo_meta` table and landing golden
  v0.1.1 / v0.3.0 DuckDB fixtures is queued.
- **Task 8 — Merge rmcp 1.3 (PR #27) + expose MCP resources.** Still
  open; rebase needs a fresh look.

## [0.3.0] - 2026-04-20

### Highlights

Auto-Dream-aware consolidation, Letta-style memory tiers, DPDPA +
EU AI Act compliance primitives, pgvector CVE-2026-3172 fix, and a
public LongMemEval / LoCoMo benchmark harness. Rolled up on top of
v0.2.0 (which was merged to main the same day).

### Added

- **Letta-style memory tiers** (`MemoryTier` type alias for the existing
  `MemoryType` enum; Working / Procedural / Semantic / Episodic). The
  engine now applies tier-specific behaviours on write: Working memories
  auto-expire after `ttl_working_seconds` (default 3600s) when no explicit
  ttl is given, and Procedural memories are clamped to the
  `procedural_importance_floor` (default 0.8) so system prompts never
  fall below recall visibility. New builder knobs
  `with_ttl_working_seconds` and `with_procedural_importance_floor`.
- **Auto-Dream-compatible reflection pass** —
  `engine.run_reflection_pass(agent_id)` performs date absolutization
  (regex rewrites `"yesterday"`, `"last week"`, `"N days ago"`, etc. to
  ISO-8601 anchored on `created_at`), accepts external rewrites
  (`metadata.dreamed_at`) and re-embeds, consolidates semantically
  near-duplicate records (`cosine ≥ 0.92`) into the newer record with
  merged tags + summed access_count, auto-resolves low-importance
  conflicts via `KeepNewest`, and archives stale low-importance
  records. Emits `ReflectionReport` with per-phase counts.
- **OpenAI Agents SDK GA snapshot store** —
  `mnemo.openai_sessions_ga.MnemoSnapshotStore` implements
  `save_snapshot` / `load_snapshot` / `list_snapshots` / `resume` plus
  `SnapshotRef` with a `snapshot://<session>/<ts>` URI. Pluggable
  `WorkspaceStorage` supports local FS today and stubs S3/R2/GCS/Azure
  behind the matching `mnemo[openai-sandbox-<backend>]` extras. Payloads
  above `inline_threshold_bytes` (default 64 KiB) offload to workspace;
  Mnemo keeps pointer + SHA-256 and verifies integrity on load.
- **DPDPA consent manager adapter** in the new `mnemo-compliance` crate
  — `ConsentSource` trait, `HttpConsentManager` (generic HTTP binding
  with optional bearer auth), `StaticConsentSource` (tests / single-
  tenant self-hosting). `ConsentState` carries scope list, expiry, and
  consent-token hash. `ComplianceError::ConsentDenied` surfaces cleanly.
- **EU AI Act audit export** — `export_audit_log(events, format, signer)`
  with two formats: `NdjsonSigned` (one JSON line per event plus a
  detached Ed25519 signature chain covering `SHA256(index ∥ prev_hash
  ∥ event_json)`; canonicalised through `serde_json::Value` so signer
  and verifier agree on bytes) and `EuAiOfficeCsv` (the AI Office GPAI
  template columns with RFC4180 escaping). `verify_ndjson_signed`
  walks the chain and rejects tampered rows with the offending index.
- **Benchmark harness** — `mnemo.benches.locomo_runner` (with CLI)
  runs `recall@5`/`recall@10`/MRR/p50/p95/p99 across
  `auto`/`vector_only`/`hybrid_rrf`/`graph_boosted` strategies and
  emits a Markdown report + JSON sidecar under `docs/benchmarks/`.
  Real dataset loaders stubbed behind the `mnemo[benchmark]` extra;
  first live numbers published in v0.3.0-rc2.

### Changed

- `pgvector` upgraded from 0.4 → 0.8.2 to pick up the fix for
  **CVE-2026-3172** (buffer overflow in parallel HNSW builds). Also
  enables `hnsw.iterative_scan` for strict-order filtered recall — the
  migration SQL will adopt it once PostgreSQL backends regenerate
  indexes.

### Carried forward from the unreleased v0.2.0

The full T1–T6 v0.2.0 feature set is included (Claude Agent SDK
adapter, OpenAI preview `Session` store, TTL sweeper,
GDPR-safe `forget_subject`, `replay(as_of=...)`, recall
`ScoreBreakdown` / `explain`). v0.2.0 was merged to main earlier today
via admin merge; the tag itself is skipped.

### Deferred to v0.3.0-rc2

- **rmcp 0.14 → 1.3 + MCP resource exposure** (prior T7). PR #27 stays
  open; the API migration is its own release.
- **DuckDB 1.4 → 1.5.2 + DuckLake opt-in backend** (Task 12b). Ships
  behind the `storage-ducklake` feature flag once the sorted-table +
  bucket-partitioning API lands.
- **First published LongMemEval / LoCoMo numbers**. The harness is
  shippable today; the datasets come with the `mnemo[benchmark]` extra.

## [0.2.0] - 2026-04-20

### Highlights

Claude Opus 4.7 + OpenAI Agents SDK first-class support, GDPR-safe subject
erasure, time-travel replay, and retrieval provenance.

### Added

- **Claude Agent SDK adapter** (`mnemo.claude_agent_sdk.MnemoClaudeMemory`).
  Exposes the full Mnemo MCP tool surface to `ClaudeAgentOptions.mcp_servers`
  and optionally materializes recalled memories into Markdown files with YAML
  frontmatter. A `watchdog` observer mirrors file edits, deletes, and
  frontmatter changes back into Mnemo so Opus 4.7's Auto Memory workflow and
  the persistent database stay in sync.
- **OpenAI Agents SDK `Session` store** (`mnemo.openai_sessions.MnemoSessionStore`).
  Implements the `get_items`/`add_items`/`pop_item`/`clear_session` protocol
  introduced in the 2026-04-15 release, storing each turn as a
  session-tagged episodic memory with a monotonic index so conversations
  survive process restarts.
- **TTL sweeper** (`engine.run_ttl_sweep`). Hard-deletes every memory whose
  `expires_at` is in the past and emits a `MemoryExpired` audit event per
  deletion, with correct hash chain linkage. The `mnemo` CLI gains
  `--ttl-sweep-interval` / `MNEMO_TTL_SWEEP_INTERVAL` that drives the sweeper
  as a background tokio task.
- **GDPR / DPDPA-aligned subject erasure** (`engine.forget_subject`). Finds
  memories tagged `subject:<id>` and either redacts the content (default,
  preserves the hash chain for audit) or hard-deletes them. Exposed via
  MCP (`mnemo.forget_subject`), REST (`POST /v1/forget_subject`), and gRPC
  (`ForgetSubject`). A new `ForgetStrategy::Redact` variant is also
  accepted wherever the standard `mnemo.forget` strategy parsing runs.
- **Point-in-time replay** (`ReplayRequest.as_of`). When set, the engine
  synthesizes a virtual checkpoint from the memories and events that
  existed at that timestamp and returns the reconstructed state. Exposed
  via MCP, gRPC (`ReplayRequest.as_of`), REST, and a new `as_of` kwarg on
  the PyO3 `replay` method.
- **Ranking-signal provenance on recall** (`RecallRequest.explain`). When
  `true`, each `ScoredMemory` carries a `ScoreBreakdown` reporting the
  per-signal contributions (vector, BM25, graph, recency) and the final
  RRF rank. Wired through MCP, REST (`?explain=true`), gRPC (`ScoreBreakdown`
  message + `ScoredMemory.score_breakdown`), and the PyO3 `recall(..., explain=True)`
  kwarg.
- `EventType::MemoryExpired` and `EventType::MemoryRedact` variants with
  snake_case `Display`/`FromStr` support, so the audit trail can
  distinguish natural expiration and subject redaction from ordinary
  deletes.
- Examples: `examples/claude_agent_sdk_example.py`,
  `examples/openai_agents_snapshot_example.py`.

### Changed

- `RecallRequest` gains `explain: Option<bool>`.
- `ReplayRequest` gains `as_of: Option<String>`.
- `ForgetStrategy` gains a `Redact` variant.
- `ScoredMemory` gains `score_breakdown: Option<ScoreBreakdown>` (skipped
  during serialization when absent — existing JSON consumers unaffected).
- Python `mnemo/__init__.py` now tolerates a missing native `_mnemo`
  extension at import time so adapter modules can be imported before
  `maturin develop` runs.

### Tests

All 36 integration tests, 70 mnemo-core unit tests, and the MCP / pgwire /
REST / admin / gRPC suites pass. Four new tests cover TTL sweep semantics,
GDPR-safe redaction (hash chain preservation), point-in-time replay, and
score-breakdown provenance. The Python adapters ship with 21 tests
(pure-Python + integration-gated) that run under `pytest python/tests/`.

### Deferred to 0.2.0-rc2 / 0.3.0

- `mnemo.reflect` Auto Dream equivalent (reflection-pass consolidation).
- rmcp 0.14 → 1.3 upgrade (PR #27) and MCP resource exposure — the API
  migration warrants a dedicated release.

## [0.1.0] - 2026-02-07

### Initial Release

Mnemo is an MCP-native memory database that gives AI agents persistent, searchable, and secure long-term memory.

### Highlights

- **10 MCP tools** for AI agents: remember, recall, forget, share, checkpoint, branch, merge, replay, delegate, and verify
- **Hybrid search** combining semantic vectors, BM25 keyword matching, knowledge graph signals, and recency scoring via Reciprocal Rank Fusion
- **Two storage backends**: embedded DuckDB for single-agent use and PostgreSQL with pgvector for distributed multi-agent deployments
- **SDKs** for Python (with OpenAI Agents, Mem0, LangGraph, and CrewAI adapters), TypeScript, and Go
- **Multiple access protocols**: MCP (stdio), REST API, gRPC, and PostgreSQL wire protocol

### Features

- **Memory lifecycle management** -- five forgetting strategies (soft delete, hard delete, decay, consolidation, archive), TTL-based expiration, and automatic decay passes
- **Security and integrity** -- AES-256-GCM at-rest encryption, SHA-256 hash chain integrity verification, RBAC with ACL-based permission filtering, memory poisoning detection, and delegation with depth-limited transitive permissions
- **Conflict resolution** -- automatic detection of contradictory memories with newest-wins, highest-importance, manual, and evidence-weighted resolution strategies
- **Branching and replay** -- checkpoint agent state, branch timelines, merge branches, and replay event history with hash chain verification
- **Causal debugging** -- trace event causality chains with configurable direction (up/down/both) and event-type filtering
- **Point-in-time queries** -- recall memories as they existed at any historical timestamp using `as_of`
- **Observability** -- OTLP span ingestion with OpenTelemetry GenAI semantic conventions, admin dashboard with agent statistics

### Infrastructure

- 9-crate Rust workspace with full CI (format, clippy, test, build, security audit)
- Helm chart for Kubernetes deployment with S3 cold-storage support
- Docker and Docker Compose configurations
- mdBook documentation site

---

## [0.1.1] - 2026-02-07

### Security

- **Fix SQL injection in PostgreSQL backend** -- replaced string-interpolated embedding values with parameterized `pgvector::Vector` bindings via sqlx
- **Add authentication to pgwire server** -- cleartext password authentication before connection acceptance; default bind changed from `0.0.0.0` to `127.0.0.1`
- **Harden CORS configuration** -- replaced permissive CORS with configurable origin allowlist via `MNEMO_CORS_ORIGINS` environment variable, defaulting to localhost only
- **Fix delegation authorization bypass** -- delegation endpoint now verifies the caller has `Delegate` permission on each target memory before creating delegations
- **Upgrade pyo3 to 0.24** -- fixes buffer overflow in `PyString::from_object` (RUSTSEC-2025-0020)
- **Upgrade tantivy to 0.25** -- resolves transitive `lru` crate unsoundness
- **Add constant-time hash comparison** -- all hash verification now uses `subtle::ConstantTimeEq` to prevent timing side-channel attacks
- **Sanitize error responses** -- internal error details are logged server-side; clients receive generic error messages
- **Add request body size limits** -- REST API enforces a 2 MB maximum request body to prevent denial-of-service via oversized payloads
- **Add prompt injection detection** -- memory content is now scanned for 11 common prompt injection patterns during anomaly scoring

### Improvements

- **Add CI security scanning** -- new cargo-audit job in GitHub Actions plus Dependabot for Cargo, npm, and GitHub Actions dependencies
- **Add agent_id input validation** -- agent identifiers are now validated for length (max 256 characters) and allowed characters (alphanumeric, hyphens, underscores, dots)
- **Add sync_metadata table to PostgreSQL migrations** -- ensures sync watermark operations work correctly in distributed deployments
- **Generate TypeScript SDK lockfile** -- `package-lock.json` committed for reproducible builds and `npm audit` support

### Documentation

- Remove hardcoded passwords from deployment examples -- Docker, Kubernetes, and PostgreSQL docs now use environment variable references
- Add CONTRIBUTING.md with contribution guidelines
- Add project memory configuration for development tooling
