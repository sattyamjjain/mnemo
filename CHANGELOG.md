# Changelog

All notable changes to Mnemo are documented in this file.

## [Unreleased]

### Added (2026-07-22) — memory-poisoning defense benchmark on a real embedder (v0.5.15)

**`bench(security)`: memory-poisoning (MINJA/consolidation) defense benchmark on
a real embedder — ASR + 95% CI + benign-FPR; refuse-to-score-on-noop.**

- **New bin [`poisoning_real_bench`](bench/poisoning/src/bin/poisoning_real_bench.rs)**
  + harness [`real_embedder_bench`](bench/poisoning/src/real_embedder_bench.rs) in
  the existing `mnemo-poisoning-bench` crate. Exercises the **shipped** detector
  (`check_for_anomaly` → `quarantine_memory` on the `remember` write path +
  `recall`'s quarantined-skip, incl. the `PoisoningPolicy` embedding z-score lane)
  through a **real semantic embedder** (default local ONNX MiniLM, no API key;
  `--embedder openai|ollama` also wired). Per attack: detector **ASR** (poison
  survives to recall) + **Wilson 95%**, plus the **benign false-positive rate**,
  over 3 seeds. **Refuses to score under a no-op embedder** (`run_real_bench`
  guards; unit test `refuses_noop_embedder`; CI covers the harness on the offline
  `DeterministicEmbedding`).
- **Attack patterns** (roadmap #37): MINJA canonical + evasive, and
  consolidation redirects (off-distribution-trigger + in-distribution).
- **Honest headline (ONNX `all-MiniLM-L6-v2`, n=90/attack):** the lexical /
  self-referential lane drops **canonical MINJA 100% → 0%** at **0/300 benign
  false-quarantine**; the embedding **z-score lane does not generalise to a dense
  embedder** — poison sits ~1.5σ from benign (below the 3σ gate, 0% flagged), so
  marker-stripped + consolidation redirects survive (ASR 100%). A z-score
  diagnostic (poison vs benign z, gate `baseline_n`) proves the gate is engaged.
  This **corrects** the hash-embedder sibling bench's rosier z-score reading.
- Raw JSON (sorted keys, no wall-clock): [`bench/results/poisoning_real.json`](bench/results/poisoning_real.json);
  methodology + honest limitations: [`docs/BENCH_POISONING.md`](docs/BENCH_POISONING.md).
- **README** security/integrity wedge now points at the real-embedder measurement.
- No version change — rides the same **unreleased 0.5.15** (not yet on crates.io).

### Added (2026-07-21) — first real-embedder LoCoMo retrieval benchmark (v0.5.14 → v0.5.15)

**`bench(locomo)`: mnemo's first retrieval numbers produced by a real semantic
embedder, with a 95% confidence interval and a hard anti-no-op guard.**

- **New bench binary [`locomo_v1_bench`](bench/locomo/src/bin/locomo_v1_bench.rs).**
  Runs the bundled 45-record LongMemEval_M slice through the real recall path
  (in-memory DuckDB + USearch HNSW + Tantivy BM25, RRF fusion) under a **real
  semantic embedder** and reports gold-document **recall@{1,5,10}** with a **Wilson
  95%** interval, **MRR**, **p50/p95** query latency, and **index build time**, per
  strategy (`lexical` / `semantic` / `auto`), averaged over 3 seeds.
  - **Default embedder is local ONNX** (`all-MiniLM-L6-v2`, 384-dim) — reproducible
    by anyone with **no API key**; `--embedder openai` (`OPENAI_API_KEY`) and
    `--embedder ollama` are also wired. The bench is **never** gated behind a paid
    embedder.
  - **Hard guard** [`guard_real_embedder`](bench/locomo/src/real_embedder.rs): the
    runner **refuses to emit any score** if the resolved embedder is not
    semantic-capable (i.e. the zero-vector no-op), naming the embedder it found. A
    silently-noop benchmark is worse than no benchmark. Unit test `refuses_noop_embedder`
    pins it.
- **Headline (ONNX `all-MiniLM-L6-v2`, n=45, mean of 3 seeds — _preliminary_):**
  `semantic` **recall@1 0.689 [0.543, 0.805]**, recall@10 0.911, MRR 0.770; `auto`
  0.615 / 0.889; `lexical` 0.422 / 0.689. Raw deterministic JSON (sorted keys, no
  wall-clock stamp) at [`bench/results/locomo_v1.json`](bench/results/locomo_v1.json);
  full writeup + limitations at [`docs/benchmarks/locomo-v1.md`](docs/benchmarks/locomo-v1.md).
- **`crates/mnemo-core/src/embedding/onnx.rs`:** migrated the ONNX embedder to
  `ort` 2.0.0-rc.11 (session behind `Arc<Mutex<_>>` for `&mut run`, `Tensor::from_array`
  inputs, `try_extract_array`) so the `--features onnx` default path builds and
  produces verified-sane (L2-normed, semantically separated) embeddings.
- **Honest scope:** retrieval quality only (not LLM-judged QA); **no** head-to-head
  vs Mem0/Letta/Zep (not run here); **DuckDB backend only** (Postgres/pgvector
  semantic path not exercised); n=45 → wide, overlapping CIs, labelled `preliminary`.
- **README:** the intro's LoCoMo claim now carries the real-embedder number + a link,
  distinguishing it from the byte-reproducible hash-embedder floor.
- Version bump **0.5.14 → 0.5.15**.

### Security (2026-07-20) — AgentAuditKit MCP static scan in CI + pre-commit (no version bump)

CI / dev-tooling only; **no version bump** (no engine/protocol/crate change).

- **chore(security): dogfood [AgentAuditKit](https://github.com/sattyamjjain/agent-audit-kit)
  (deterministic, offline MCP/agent-config scanner, 262 rules) on the mnemo repo.**
  New `agent-audit-kit` job in [`.github/workflows/security.yml`](.github/workflows/security.yml)
  (pinned `@v0.3.52`) scans mnemo's MCP-server surface, agent configs, and
  supply-chain manifests for secrets / tool-poisoning / auth-bypass /
  path-traversal / supply-chain CVEs — complementing `cargo-audit` + `cargo-deny`
  (Rust crate advisories) on the *MCP / agent* attack surface `mnemo-mcp`
  exposes. Uploads SARIF to the Security tab + posts a PR comment; also wired as
  an opt-in [`.pre-commit-config.yaml`](.pre-commit-config.yaml) hook.
  - **Observe-first gate:** `fail-on: critical` via
    [`.agent-audit-kit.yml`](.agent-audit-kit.yml); highs/mediums are reported
    but non-blocking, to be triaged in the Security tab before tightening to
    `high`.
  - **Baseline established by running it first (v0.3.52):** the one noisy rule
    `AAK-AGENT-001` (60 false-positive criticals on `CLAUDE.md`, which legitimately
    documents build/test commands) is excluded with a written rationale; the
    remaining 51 findings (9 high / 41 medium / 1 low) stay visible — including
    the legit `AAK-GHA-IMMUTABLE-001` (pin Actions to SHAs). The two products
    stay **separate repos**: mnemo (runtime tamper-evident audit) + AgentAuditKit
    (static pre-deploy scan) are complementary, not merged.

### Added (2026-07-20) — STATE-Bench entry harness (number pending model access; no version bump)

Bench harness + docs only; **no version bump** (no engine/protocol/crate change,
no benchmark number yet). This lands the *integration*, not a result.

- **bench(state-bench): mnemo's entry on Microsoft STATE-Bench (Agent Learning
  Track).** New [`bench/state_bench/`](bench/state_bench/) — a **Python-native
  driver** (not a Rust crate: STATE-Bench is Python/`uv` + a `StateBenchAgent`
  subclass, so a Rust crate would reimplement the whole harness). mnemo plugs into
  the read-only `retrieve_learnings(query, top_k) -> list[str]` hook via the
  **public Python SDK** (`MnemoClient.recall`), backed by an embedded DuckDB store
  built from the train trajectories (`build_learnings`). **No `mnemo-core` change.**
  - **Resolved, pinned, cited:** [`microsoft/STATE-Bench`](https://github.com/microsoft/STATE-Bench)
    @ `4efcbf2d4fe60df04878859b692d9391f3d5b33a` (v0.8.1, MIT); baseline
    GPT-5.1-no-memory ~50–60% pass@1 ([leaderboard](https://microsoft.github.io/STATE-Bench/leaderboard/)).
  - **Number is PENDING hosted-model access, not faked.** STATE-Bench is an
    *agentic* enterprise-task benchmark (task completion, not retrieval): it
    hard-locks its user simulator + judge to **GPT-5.4** and needs an agent model
    (gpt-5.1-class). Those are unreachable from the build environment
    (no OpenAI/Azure keys; only a local embedder). Per the honest-benchmark rule we
    publish **no partial or fabricated number** — the harness is built and the
    mnemo half smoke-tested offline; a real run is turnkey via
    [`run_state_bench.sh`](bench/state_bench/run_state_bench.sh) once models exist.
  - **Honest framing:** the score is dominated by the agent model; mnemo is one
    read-only memory hook. So it is an *agent+memory-hook delta* on an agentic
    benchmark — the **on-prem / embedded / auditable** entry nobody has posted, and
    evidence *for* the regulated-AI wedge (the same store carries the hash-chained
    audit log), **not** a retrieval score and **not** a "state of the art" claim.
    README benchmark section gains the entry; the regression gate
    (`check_bench_regression.py`, dataset-scoped `recall@10` for locomo/longmemeval)
    is out of scope by construction and unchanged.

### Added (2026-07-19) — v0.5.14, DPDP Rules processing-log retention-conformance profile

Workspace `0.5.13 → 0.5.14` (patch bump — an additive `mnemo-compliance` surface
+ a `StorageBackend` capability method + a CLI command + a bench; no breaking
API change).

- **feat(compliance): processing-log retention-conformance profiles.** New
  [`mnemo_compliance::RetentionProfile`](crates/mnemo-compliance/src/retention.rs)
  expresses a per-obligation retention **floor** (configurable via
  `with_floor_days`) and *verifies* — over before/after `AgentEvent` snapshots —
  that no deletion / compaction / cold-tier path dropped or rewrote a log row
  inside the floor, and that **traffic/processing metadata** (DPDP names "personal
  data, traffic data and logs" separately) was retained. Defaults:
  **DPDP Rules 2025 → 365 days**, **EU AI Act Art.19/26(6) → 180 days**,
  **HIPAA §164.312(b)/§164.316(b)(2) → six years**. Matches the pure-function-over-
  `&[AgentEvent]` shape of the existing Art.12 `export_audit_log` surface.
  - **Fail loud, never silent.** `RetentionProfile::assert_backend_can_retain`
    returns the new typed
    [`ComplianceError::RetentionFloorUnsupported { backend, floor_days }`](crates/mnemo-compliance/src/error.rs)
    (naming the backend) when the active backend cannot guarantee an append-only
    log — the same posture as `mnemo_core::error::Error::EmbedderNotConfigured`
    (v0.5.13). Backed by a new default `StorageBackend::events_are_append_only()`
    capability (`true` for DuckDB — no `DELETE`/`UPDATE` on `agent_events` — and
    PostgreSQL — plus a `prevent_event_modification` trigger).
  - **The enumeration is real.** Every mnemo-core path that could plausibly drop
    an event — `forget` (SoftDelete/HardDelete/Redact/Archive incl. cold-tier),
    `run_ttl_sweep`, `run_decay_pass`, `run_consolidation` — edits *memory content*
    and **appends** an audit event; none removes one. The `agent_events` log is
    append-only by construction; this is the DPDP *personal data* (erasable) vs
    *traffic data and logs* (retained) split.
  - **CLI.** `mnemo compliance retention --profile <dpdp|eu-ai-act-art19|hipaa>
    [--floor-days N]` prints the profile and gates it against the active backend's
    append-only guarantee (fails loud on a backend that cannot honour the floor).
  - **Bench.** New `publish = false`
    [`bench/retention_conformance`](bench/retention_conformance) drives every
    deletion path end-to-end and emits a byte-stable machine-readable artifact
    (profile, floor, one row per path, pass/fail) —
    [`results/retention_conformance.md`](bench/retention_conformance/results/retention_conformance.md)
    (+ `.json`). Sibling of `bench/audit_conformance` (tamper-evidence). Contract
    pinned by `bench/retention_conformance/tests/conformance.rs`.
  - **Docs.** README gains a **Compliance profiles** table (profile → obligation →
    floor → commencement → primary-source URL), using conformance-check language
    only — no certification or compliance claim. DPDP commencement is **2027-05-13**
    (Gazette G.S.R. 846(E), 2025-11-13; 18-month transition); EU AI Act high-risk
    dates are **2027-12-02** (stand-alone Annex III) / **2028-08-02** (Annex I
    embedded) per the Digital Omnibus (Council final green light 2026-06-29).

### Fixed (2026-07-18) — v0.5.13, semantic recall fails loud instead of silent-empty

Workspace `0.5.12 → 0.5.13` (patch bump — a **correctness/safety** fix to the
recall path; no dependency change).

- **fix(recall): semantic recall hard-errors when no real embedder is configured,
  instead of silently returning empty.** With the no-op embedder every query
  embeds to an all-zero vector, so `strategy` ∈ {`semantic`, `hybrid`, `auto`,
  `graph`, `domain_scoped`} (and the typed `RetrievalMode` equivalents) would feed
  a degenerate vector to the index and silently return an empty or meaningless
  result set. These paths now return a typed
  [`Error::EmbedderNotConfigured { requested, backend }`](crates/mnemo-core/src/error.rs)
  — *"semantic recall requires a configured embedder (OpenAI HTTP or local ONNX);
  the noop embedder returns no vectors — refusing to silently return empty."*
  - **The non-semantic path is untouched.** `strategy="lexical"` (BM25) and
    `strategy="exact"` (filter) need no embedder and keep working; `remember` /
    CRUD / ACL / audit are unaffected.
  - **Mechanism.** A new default trait method `EmbeddingProvider::is_semantic_capable()`
    (`true` for OpenAI/ONNX/any real provider; overridden to `false` on
    `NoopEmbedding`) drives a guard in
    [`recall::execute`](crates/mnemo-core/src/query/recall.rs) that fires before the
    query is embedded. `StorageBackend::backend_name()` (`"duckdb"` / `"postgres"`)
    names the backend in the error. This complements the existing Postgres
    `Error::BackendUnsupported` fail-loud (an *unwired index*); this fix covers an
    *absent embedder* on either backend.
  - **New public `DeterministicEmbedding`** — a deterministic, offline
    bag-of-words hashing embedder (real, non-zero vectors; `is_semantic_capable()`)
    for tests, examples, and demos that need the vector path without an API key or
    model. **Not** a production-quality semantic model.
  - **Docs.** README gains a **supported-embedder matrix** (which embedders
    actually produce semantic results) naming DuckDB (or PostgreSQL) **+ a real
    embedder** (OpenAI or on-prem ONNX) as the supported semantic path, and the
    no-op default as a hard-error.
  - **Tests.** New [`crates/mnemo-core/tests/semantic_recall_hard_error.rs`](crates/mnemo-core/tests/semantic_recall_hard_error.rs)
    proves (a) semantic recall under the no-op embedder → typed error, (b) lexical
    recall under the no-op embedder → still returns results, (c) semantic recall
    with a real embedder → returns results. Existing Noop-based suites that
    exercised recall were migrated to `DeterministicEmbedding` (or, for the
    evidence-scorer retrieval-fallback suites, to `strategy="lexical"`); the
    conflict-detection tests that intentionally use degenerate identical vectors
    keep a no-op engine.

### Added (2026-07-16) — Art.12 audit-log tamper-evidence benchmark + `mnemo-db` defensive crate

- **feat(compliance): adversarial audit-log tamper-evidence benchmark.** New
  `publish = false` bench [`bench/audit_tamper`](bench/audit_tamper) builds a
  **real** `agent_events` hash chain through the shipped `remember()` path,
  exports it, and applies four post-hoc attacks — **delete** (mid-chain),
  **reorder** (swap two events), **forge** (integrity field `content_hash`), and
  **truncate** (tail) — scoring each with mnemo's shipped `verify_event_chain`
  (the verifier `verify_event_integrity` runs). Reports a **detection rate** with
  a **Wilson 95%** interval per attack, plus a **benign control**. Result
  (deterministic, offline, byte-stable): delete / reorder / forge-`content_hash`
  each **200/200 (100.0%)** [Wilson 95% 98.1%–100.0%]; **0/72** benign
  false-positives; and **honest 0/200** on payload-only forge + tail truncation —
  two disclosed gaps whose shipped mitigations (memory-record content is
  hash-bound; Postgres `prevent_event_modification` trigger; signed checkpoints)
  are named, not oversold. Contract pinned by `bench/audit_tamper/tests/tamper.rs`.
  - Repro: `cargo run --release -p mnemo-audit-tamper-bench` →
    [`bench/audit_tamper/results/audit_tamper.md`](bench/audit_tamper/results/audit_tamper.md).
  - Narrative:
    [`docs/benchmarks/audit-log-tamper-evidence.md`](docs/benchmarks/audit-log-tamper-evidence.md)
    (cites EU AI Act Art.12 record-keeping, Art.19(1)/Art.26(6) ≥6-month
    retention, and the Art.99(4) **€15M / 3%-of-turnover** penalty tier).
  - Wired into [`docs/POSITIONING.md`](docs/POSITIONING.md) as the Art.12
    tamper-evidence proof point (new thesis-table row + repro command + penalty
    citation).
- **chore(trust): reserve the `mnemo-db` crate name on crates.io.** New
  dependency-free, `publish = true` pointer crate
  [`crates/mnemo-db`](crates/mnemo-db) whose docs redirect Rust users to
  `mnemo-core` + `mnemo-mcp` (the unqualified `mnemo` name on crates.io is an
  unrelated project). It is explicitly **distinct from the PyPI `mnemo-db`
  package**, which is the real Python SDK. `mnemo-db` is removed from the
  README-guard `KNOWN_NON_CRATE` allowlist (it is now a real workspace member),
  and [`release-crate.yml`](.github/workflows/release-crate.yml) gains it as a
  leaf in the gate + coordinated dry-run + idempotent publish loop — so on an
  unchanged workspace version the four compliance-line crates 404-gate and only
  `mnemo-db` is newly published. **No version bump** (docs + bench + a new
  never-before-published crate published at the current `0.5.12`).

### Docs (2026-07-13) — contributor IP + regulated-AI README wedge

Docs/governance only; **no version bump** (no engine, protocol, crate, or
compliance-module change; workspace stays at `0.5.12`).

- **docs(governance): add DCO+CLA; narrow README positioning to regulated-AI
  memory.**
  - **Contributor IP hygiene.** [`CONTRIBUTING.md`](CONTRIBUTING.md) now requires
    a per-commit **Developer Certificate of Origin** sign-off (`git commit -s`,
    full DCO 1.1 text inline); a new self-contained
    [`.github/workflows/dco.yml`](.github/workflows/dco.yml) enforces it on every
    PR (matching `Signed-off-by` ↔ author, no third-party action); a new
    [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) carries
    the sign-off checkbox + Summary/Test-Plan sections. New [`CLA.md`](CLA.md)
    adds the standard Apache-style **ICLA + CCLA** for substantial contributions.
    The project stays **Apache-2.0**; `LICENSE` is unchanged; neither the DCO nor
    the CLA transfers copyright.
  - **README wedge.** The top-of-README tagline moves from the generic
    "MCP-native memory database for AI agents" to the survivor position —
    **on-prem, MCP-native, cryptographically-auditable memory for regulated AI
    (EU AI Act Art.12 · India DPDP · HIPAA)** — while keeping the
    REMEMBER/RECALL/FORGET/SHARE primitives and the DuckDB-embedded (+ your
    PostgreSQL) wedge intact, and calling out the hash-chained `agent_events`
    audit log + `mnemo-compliance` as the differentiator. Links the already-
    shipped [`docs/POSITIONING.md`](docs/POSITIONING.md) (unchanged).

### Distribution (2026-07-13) — v0.5.12, crates.io compliance line

Workspace `0.5.11 → 0.5.12` (patch bump — a **distribution-only** change: no
engine, protocol, or bench code change; the bump exists so the compliance line
gets a clean, previously-unpublished crates.io version and a fresh release tag).

- **distribution: 0.5.x compliance line published to crates.io.** The minimal
  importable set to run on-prem, hash-chain-audited memory —
  [`mnemo-core`](https://crates.io/crates/mnemo-core),
  [`mnemo-attention-state`](https://crates.io/crates/mnemo-attention-state),
  [`mnemo-compliance`](https://crates.io/crates/mnemo-compliance), and
  [`mnemo-mcp`](https://crates.io/crates/mnemo-mcp) — now carries full
  `[package]` metadata (description, `license = "Apache-2.0"`, repository,
  homepage, keywords, categories, and a per-crate README) and publishes via a new
  tag-triggered [`release-crate.yml`](.github/workflows/release-crate.yml).
  - `mnemo-attention-state` is in the set because `mnemo-mcp` hard-depends on it
    (introduced after the last 0.4.4 crates.io publish); publish order is
    `core → attention-state → compliance → mcp`.
  - The workflow gates on **only the publish closure** (fmt + clippy + tests for
    those four crates), so the pre-existing golem-wit WASM link failure that
    reddens the workspace-wide build cannot block a release; it version-gates
    each crate (404 check) for idempotent re-runs, and dry-runs before uploading.
  - Internal path deps in the root `Cargo.toml` carry `version` alongside `path`
    (bumped `0.5.0 → 0.5.12` in lockstep) so `cargo publish` accepts them.
  - No crate name collision: `mnemo-core`/`mnemo-mcp`/`mnemo-compliance` are
    already owned by this project on crates.io (last at 0.4.x); no rename needed.
  - README gains an **Install from crates.io** section leading with the offline
    hash-chain verify API, pointing at [`docs/POSITIONING.md`](docs/POSITIONING.md).

### Docs (2026-07-08) — compliance-axis positioning one-pager

Docs-only; **no version bump** (no code, bench, or protocol change).

- **docs: POSITIONING.md — compliance-audit-axis comparison vs Mem0/Letta/native
  memory, wired to shipped bench numbers.** New
  [`docs/POSITIONING.md`](docs/POSITIONING.md) converts the already-published
  benchmarks into a single credibility one-pager: a capability table across
  on-prem/self-host, MCP-native primitives, cryptographic hash-chain audit log,
  memory-poisoning defense (shipped delta), and regulatory mapping (EU AI Act
  Art.12 2026-08-02 · India DPDP 2027-05-13 · OWASP ASI06). Every mnemo cell
  cites a reproducible bench already in the repo — LongMemEval semantic recall@1
  0.739, audit-conformance 100%/256-trial tamper detection (Wilson-95), poisoning
  defense delta MINJA +100 / AgentPoison +96.5 pts with 0/200 benign FPR, and the
  byte-stable LoCoMo reproduction — and the page states plainly where mnemo does
  **not** lead (recall/QA quality vs Mem0's funded team). No invented numbers; no
  gating; Apache-2.0. README gains a top-of-file link to it.

### Added (2026-07-07) — v0.5.11, memory-poisoning defense-delta benchmark

Workspace `0.5.10 → 0.5.11` (patch bump — a new bench crate + tests + docs; no
engine/protocol API change). Offline, Apache-2.0, **no managed-cloud dependency
added to core**.

- **bench(security): memory-poisoning defense delta.** New crate
  [`bench/poisoning`](bench/poisoning/) (`cargo run --release -p
  mnemo-poisoning-bench`) measures the **Attack Success Rate (ASR)** of two
  named, published attacks with mnemo's shipped poisoning-quarantine defense
  **OFF vs ON** — the **delta** is the headline. It toggles the *real* defense
  (`check_for_anomaly` → `quarantine_memory` on the `remember` write path +
  recall's `quarantined` skip, plus the opt-in `PoisoningPolicy` z-score gate) —
  **not** provenance HMAC (which is per-read receipts, not a retrieval filter),
  stated honestly in the crate docs.
  - **Attacks:** MINJA-style memory injection
    ([arXiv:2503.03704](https://arxiv.org/abs/2503.03704), indirect-ingest poison
    with self-referential bridging phrasing) and an AgentPoison-style low-rate
    trigger (single novel-token poison among 1001 benign, **0.0998% < 0.1%** of
    the store).
  - **Observed (seed `0x901504202607`, 200 trials/attack, top-5):** MINJA
    canonical ASR 100%→0% (**delta +100 pts**, lexical lane); AgentPoison
    100%→3.5% (**delta +96.5 pts**, z-score gate); **benign control 0/200
    false-quarantine**. Evasive MINJA (markers stripped) stays 100%→100% — a
    disclosed lexical-lane blind spot, not hidden.
  - **Deterministic + byte-stable:** fixed corpus, deterministic hashed embedder,
    exact brute-force vector index (the reference mnemo's approximate HNSW
    tracks), neutralised recency lane; every ASR carries a Wilson 95% interval.
    Tests in [`bench/poisoning/tests/`](bench/poisoning/tests/) gate the delta,
    the 0% benign control, and byte-stability. Observed numbers only, never a
    claimed one; no "best"/"first" claim.
  - Reuses the shared `mnemo_locomo_bench::stats::wilson_95` helper.
- **chore(python): close the PyPI publish gap.** The `mnemo-db` PyPI SDK version
  was stale at `0.4.9` while the workspace moved to `0.5.x` — a stale published
  artifact undercuts the benchmark-credibility story. Bumped
  `python/pyproject.toml` + `python/mnemo/__init__.py` to `0.5.11` (PyO3 wheel
  verified to build against the current core with `maturin build --release`); the
  push-to-`main` `pypi-publish` workflow publishes `mnemo-db 0.5.11`.

### Added (2026-07-06) — v0.5.10, claimed-vs-observed LoCoMo reproduction

Workspace `0.5.9 → 0.5.10` (patch bump — a new bench bin + a shared loader + a
byte-stable test + docs; no engine/protocol API change). Offline, Apache-2.0,
**no managed-cloud dependency added to core**.

- **bench(locomo): claimed-vs-observed reproduction.** New
  [`reproduction_bench.rs`](bench/locomo/src/bin/reproduction_bench.rs)
  (`cargo run --release -p mnemo-locomo-bench --bin reproduction_bench`) re-runs a
  LoCoMo single-hop split under mnemo's disclosed **offline hybrid-recall harness**
  (`strategy="auto"`: BM25 + graph + RRF fusion; fixed seed; Wilson-95) and tables
  mnemo's **observed** number against vendors' **published, cited, not-re-run**
  claims (Mem0 92.5; Zep 84→58.44 corrected; MemPalace 100→60.3 R@10 corrected;
  Supermemory ~99 self-reported PoC) — riding the 2026 memory-benchmark
  reproducibility crisis. Only mnemo's row is reproducible here; the report is
  explicit that the claimed figures are **not a ranking** (retrieval vs
  end-to-end QA, different scale/judge). No "best"/"first" claim.
  - **Reproducible by disclosure.** The report
    ([`bench/locomo/results/reproduction_2026-07-06.md`](bench/locomo/results/reproduction_2026-07-06.md))
    is **byte-stable** — two runs `diff` identically — via two *disclosed*
    methodological choices: an **exact brute-force cosine** vector index (the
    deterministic reference mnemo's approximate USearch HNSW tracks) and a
    **neutralised recency lane** (a batch-seeded corpus has no recency signal, so
    the wall-clock lane is pinned to a constant). A new
    [`bench/locomo/tests/reproduction_byte_stable.rs`](bench/locomo/tests/reproduction_byte_stable.rs)
    gates the byte-stability. Observed (offline hashed embedder, n=45 single-hop):
    recall@1 **24.4%** [Wilson 95% 14.2%, 38.7%], recall@3 37.8%, recall@5 46.7%
    (2/45 queries errored in the BM25 lane on natural-language punctuation and are
    disclosed + counted as misses).
  - **Real-embedder path** gated behind `--ollama-model` (fail-loud, never a
    silent number), matching the sibling benches.
  - **Refactor:** extracted the shared LoCoMo fixture loader into
    `mnemo_locomo_bench::dataset` (`LongMemRecord` + `load_dataset` +
    `default_dataset_path` + `dataset_sha`), reused by `reproduction_bench`.
- **docs:** `bench/RESULTS.md` gains a claimed-vs-observed section; `README.md`
  adds a "reproducible-by-disclosure" line to the regulated-AI block.

### Added (2026-07-05) — v0.5.9, regulated-memory audit-conformance artifact

Workspace `0.5.8 → 0.5.9` (patch bump — a new offline bench crate + compliance
docs + positioning; no engine/protocol API change). Apache-2.0, offline-
verifiable, **no managed-cloud dependency added to core**.


Workspace `0.5.8 → 0.5.9` (patch bump — a new offline bench crate + compliance
docs + positioning; no engine/protocol API change). Apache-2.0, offline-
verifiable, **no managed-cloud dependency added to core**.

- **bench: offline, deterministic audit-conformance proof.** New crate
  [`bench/audit_conformance/`](bench/audit_conformance/)
  (`cargo run --release -p mnemo-audit-conformance-bench`) proves — with no
  network and no LLM — that mnemo's memory-write log is tamper-evident and
  externally verifiable **without trusting the store**. It is a driver+reporter
  built **entirely on shipped `mnemo-core` primitives** (`hash::verify_chain`,
  `hash::verify_event_chain`, `MnemoEngine::verify_integrity`,
  `verify_event_integrity`) — it never re-implements cryptography.
  - **Six properties, all `PASS`:** the write chain verifies through the real
    `remember()` path; the append-only `agent_events` log verifies; a single-byte
    content mutation is caught **100% over 256 trials (Wilson 95% ≥ 98.5%)** and
    the first broken record is named; `forget` **appends** a signed
    `MemoryDelete` event and **retains** the original write row (append-only
    retention, not erasure); plus a fixed, **recomputable SHA-256 crypto vector**
    anyone can reproduce offline (`printf … | shasum -a 256`).
  - **Byte-stable report.** No timestamps or run-varying hashes in the body — two
    runs `diff` identically; the run prints the report's own SHA-256. Report at
    [`bench/audit_conformance/results/conformance.md`](bench/audit_conformance/results/conformance.md).
  - Reuses the shared `mnemo_locomo_bench::stats::wilson_95` helper (no per-bin
    copy). Registered in `[workspace] members`.
- **docs(compliance): regulatory mappings (honest, hedged, not legal advice).**
  [`docs/compliance/eu-ai-act-art12.md`](docs/compliance/eu-ai-act-art12.md)
  maps the append-only log + retention to EU AI Act Art.12 record-keeping and
  Art.26(6) six-month deployer log retention, with the hedge that the May-2026
  Digital Omnibus proposal may move high-risk dates toward Dec-2027.
  [`docs/compliance/dpdp-2027.md`](docs/compliance/dpdp-2027.md) maps to the
  India DPDP Rules 2025 obligations (full-compliance working date 2027-05-13),
  and states the DPDPA-erasure vs AI-Act-retention tension explicitly (mnemo
  ships both `HardDelete` and `Redact` and logs which was used).
- **docs(results): auditability comparison.** `bench/RESULTS.md` gains an
  auditability table (mnemo offline hash-chain verify vs Mem0 vs Zep
  cloud/managed audit), sourced from each vendor's docs +
  developersdigest.tech, with a dated hedge and no "best" claim.
- **docs(README): regulated-AI positioning block** — "on-prem, MCP-native,
  cryptographically-auditable memory for regulated AI (EU AI Act / DPDPA /
  HIPAA)", linking the bench and the two compliance docs.

### Added (2026-07-04) — v0.5.8, reproducible BEAM-style multi-hop/open-domain retrieval bench

Workspace `0.5.7 → 0.5.8` (patch bump — a new bench bin + a shared stats helper
+ docs; no engine/protocol API change).

- **bench(locomo): reproducible BEAM-style multi-hop/open-domain number over
  hybrid recall.** New bin
  [`bench/locomo/src/bin/beam_bench.rs`](bench/locomo/src/bin/beam_bench.rs)
  (`cargo run --release -p mnemo-locomo-bench --bin beam_bench`) runs two
  BEAM-style subtasks — multi-hop (answer reachable only via a `related_to`
  graph edge) and open-domain (gold among same-schema distractors) — over
  mnemo's default hybrid `auto`/RRF recall (semantic + BM25 + graph + recency),
  reporting per-subtask accuracy with a **Wilson 95%** interval.
  - **Deterministic + offline by default:** a hashed bag-of-tokens embedder (no
    network, no LLM), fixed seed `0xbea320262026`, 100 queries × 5 pooled
    repeats/subtask (repeats pooled to average the USearch HNSW approximate-NN
    noise floor into the CI — the same treatment `semantic_recall_bench`
    documents). A real-embedder path is gated behind `--ollama-model` and fails
    loud if Ollama is unreachable — never a silent/unreproducible number.
  - **Result (this run):** `multi_hop` **0.6%** (3/500, [0.2%, 1.7%]),
    `open_domain` **68.6%** (343/500, [64.4%, 72.5%]). The low multi-hop figure
    is reported as-is — default `auto` RRF barely surfaces a graph-only answer
    against lexically-equivalent distractors; `graph`/`reconstruct` are the
    multi-hop tools.
  - **Honest framing (no over-claim):** `bench/RESULTS.md` + README add a BEAM
    row with an explicit **reproduced (this fixture) vs. self-reported
    (upstream)** column and a note that self-reported memory scores (e.g.
    Hindsight BEAM 64.1% @ 10M tokens) are a vendor-run **upper bound**, not
    independently reproduced, and **not comparable** to a small synthetic
    fixture. No "first"/"best" claim.
  - **Refactor:** extracted the shared `wilson_95` CI helper into
    `mnemo_locomo_bench::stats` (reused by `beam_bench` and `asi06_resistance`
    instead of a per-bin copy). Also corrected `bench/RESULTS.md`'s stale
    backend note (pgvector semantic recall is implemented as of v0.5.7, #99).

### Fixed (2026-07-04) — v0.5.7, real pgvector ANN search on the Postgres backend ([#99])

Workspace `0.5.6 → 0.5.7` (patch bump — implements a previously-stubbed backend
capability; no public API change to the `VectorIndex` trait).

- **fix(postgres): implement pgvector ANN search — semantic/hybrid/graph recall
  now returns results on the Postgres backend ([#99]).** `PgVectorIndex::search`
  / `filtered_search` previously returned a typed `BackendUnsupported` error
  (they had been `Ok(vec![])` before 2026-06-23). They now run a real
  cosine-distance ANN query (`embedding <=> $1 … ORDER BY … LIMIT k`) against the
  `idx_memories_embedding_hnsw` HNSW index (`vector_cosine_ops`), returning the
  stored memory ids + distances in the same `(id, distance)` shape as USearch —
  so recall's `score = 1.0 - distance` conversion is identical across backends.
  - **Permission-safe.** `filtered_search` mirrors the USearch backend's
    iterative oversample-then-filter (3× → double until `limit` accessible hits
    or the table is exhausted), so scoped/filtered recall never under-returns.
  - **Wiring.** `PgVectorIndex::with_pool(pool, dims)` shares `PgStorage`'s
    `sqlx::PgPool` (new `PgStorage::pool()` / `dimensions()` accessors); the CLI
    now constructs the index with the pool. The synchronous `VectorIndex` trait
    is bridged to async `sqlx` via `block_in_place` + `Handle::block_on`, which
    requires the multi-threaded Tokio runtime (the CLI/server is `#[tokio::main]`).
  - **Still fails loud.** A pool-less index, or a genuinely-absent pgvector
    extension / `<=>` operator at runtime, returns the typed
    `Error::BackendUnsupported` — never a silent empty result.
  - **Test.** New `MNEMO_TEST_POSTGRES_URL`-gated integration test
    `crates/mnemo-postgres/tests/pgvector_ann.rs` (skips cleanly when unset):
    inserts 3 known-embedding memories, asserts `semantic` + `auto` return the
    nearest in rank order, and that a nearer *private* record owned by another
    agent is excluded (permission filter + oversample). README backend
    capability matrix updated: Postgres vector recall flips from ❌ to ✅.

[#99]: https://github.com/sattyamjjain/mnemo/issues/99

### Added (2026-07-02) — v0.5.6, first memory-poisoning resistance micro-bench + OWASP ASI06 mapping

Workspace `0.5.5 → 0.5.6` (patch bump — a new bench bin + a security doc + one
README row; no new detector, no engine/protocol API change).

- **bench(security): publish mnemo's first memory-poisoning *resistance* number
  (OWASP ASI06).** New bin
  [`bench/locomo/src/bin/asi06_resistance.rs`](bench/locomo/src/bin/asi06_resistance.rs)
  (`cargo run --release -p mnemo-locomo-bench --bin asi06_resistance`) quantifies
  how well the **existing** poisoning defense (`check_for_anomaly` → `quarantine`
  → recall skips quarantined) resists a query-only MINJA-style attack
  ([arXiv:2503.03704](https://arxiv.org/abs/2503.03704)). This adds **no**
  detector — it measures the one already shipped. DEFENDED vs UNDEFENDED isolates
  exactly one variable (the `quarantined` flag on a byte-identical record).
  - **Result (200 deterministic trials/class, top-5, seed `0xa510062026`):**
    canonical MINJA (bridging markers) → **100.0% resistance, Wilson 95%
    [98.1%, 100.0%]**, 200/200 quarantined; the same poison is recalled 100% of
    the time in an undefended store.
  - **Honest limitation, published alongside:** a marker-free *evasive*
    paraphrase → **0.0%** resistance [0.0%, 1.9%] — the always-on lexical layer
    keys on bridging phrasing; the opt-in embedding z-score baseline gate
    (`PoisoningPolicy::with_outlier_threshold`) is the intended defense there and
    is **not** exercised in this single-embedder run.
- **docs: [`docs/security/ASI06.md`](docs/security/ASI06.md)** maps mnemo's
  REMEMBER anomaly scan / quarantine / RECALL quarantine-filter / hash-chain to
  OWASP **ASI06 (Memory & Context Poisoning)**, states the number + full
  methodology + limitations (query-only MINJA variant, not a full adversarial
  suite, single embedder). README enforcement-table poisoning row updated to
  cite the published number.

### Fixed (2026-07-03) — v0.5.5, workspace-member drift ([#74])

Workspace `0.5.4 → 0.5.5` (patch bump — docs + a CI fence + version stamps; no
dependency, engine, or protocol API change).

- **chore(workspace,docs): close the phantom-crate drift ([#74]).** Seven
  `mnemo-*` crate names asserted by the daily-prompt ledger
  (`mnemo-envelope`, `mnemo-aas01`, `mnemo-mgt`, `mnemo-bench-cf`,
  `mnemo-langgraph`, `mnemo-purview`, `mnemo-toolhive`) have **no source tree
  and are not `[workspace] members`**. None were stubbed — each is an
  external-system adapter with no consumer, and an empty shell is exactly the
  drift the repo already *retired* `mnemo-langgraph` for. Instead every
  reference is now truthful:
  - **New single source of truth**
    [`docs/roadmap/planned-crates.md`](docs/roadmap/planned-crates.md) — all
    seven listed as **Planned / not built** (or **Retired**, for the
    `mnemo-langgraph` Rust shell superseded by the Python `MnemoCheckpointer`).
  - **Residual shipment-assertions corrected.** `docs/src/integrations/mcp-server.md`
    no longer says the `mnemo-envelope` exporter "lands in v0.4.3";
    `docs/comparisons/cloudflare-agent-memory.md` no longer says bench numbers
    "ship in v0.4.3 as the `mnemo-bench-cf` crate" — both now say **not built**
    and link the roadmap. The already-honest "Parked"/"Retired"/"has not been
    built" notes elsewhere are unchanged.
  - **CI fence against recurrence.** New
    `crates/mnemo-cli/tests/readme_crate_claims_are_real.rs` fails the build if
    a `mnemo-*` name in `README.md` is neither a real workspace member (matched
    live against member dir basenames + declared package names) nor on an
    explicit allowlist of non-crate references (PyPI/npm dist names, JSON
    filenames, labelled sketches, prose hypotheticals). Mirrors the AAK
    rule-count fence + the existing `readme_no_marketing_phrases` lint.
- Version stamps bumped to `0.5.5`: `Cargo.toml`, `version_metadata` test,
  `docs/compat/version-skew-matrix.md`.

[#74]: https://github.com/sattyamjjain/mnemo/issues/74

### Landing trace (2026-07-07)

This `[Unreleased]` accumulator sits on `main` at
[`d764de6`](https://github.com/sattyamjjain/mnemo/commit/d764de6) (the v0.5.10
claimed-vs-observed LoCoMo cut). It now also carries the **v0.5.11**
memory-poisoning defense-delta benchmark above, landing via branch
`feat/poisoning-defense-bench` (push-to-`main`, tagged `0.5.11`; the workspace
version bump `0.5.10 → 0.5.11` triggers the crates.io publish of changed crates
— the bench crate is `publish = false`). It also carries the **2026-07-08
compliance-axis positioning one-pager** (`docs/POSITIONING.md`) above — a
docs-only change landing via branch `docs/positioning-compliance-axis`
(push-to-`main`, **no version bump**, no crate republish). And it carries the
**v0.5.12** crates.io compliance-line distribution change above — landing via
branch `chore/crates-io-0.5.x`, workspace bump `0.5.11 → 0.5.12`, tagged
`v0.5.12` to fire the new tag-triggered `release-crate.yml` (publishing
`mnemo-core` → `mnemo-attention-state` → `mnemo-compliance` → `mnemo-mcp`). The
prior `v0.5.11` tag already points at the poisoning cut (`3d21e63`) and predates
`release-crate.yml`, so a fresh `v0.5.12` tag is what carries the workflow.
It also carries the **2026-07-13 contributor-IP + regulated-AI README wedge**
governance change above (DCO + CLA + PR template + README tagline) — a
docs/governance-only change landing via branch `docs/cla-and-positioning`
(push-to-`main`, **no version bump**, no crate republish). It also carries the
**2026-07-16 Art.12 audit-log tamper-evidence benchmark + `mnemo-db` defensive
crate** change above — landing via branch
`feat/audit-log-tamper-evidence-bench` (push-to-`main`, **no version bump**); the
new `mnemo-db` pointer crate is published at the current `0.5.12` via
`release-crate.yml`'s idempotent loop (the four compliance-line crates 404-gate
as already-present). It also carries the **2026-07-18 semantic-recall
fail-loud correctness fix** above — landing via branch
`fix/semantic-recall-hard-error` (push-to-`main`, workspace bump
`0.5.12 → 0.5.13`); this is an engine (`mnemo-core`) change, so a `v0.5.13` tag
republishes the compliance line via `release-crate.yml`. Finally it carries the
**2026-07-19 DPDP retention-conformance profile** above — landing via branch
`feat/dpdp-retention-conformance` (push-to-`main`, workspace bump
`0.5.13 → 0.5.14`); a `v0.5.14` tag republishes the compliance line
(`mnemo-core` + `mnemo-compliance` changed). Finally it carries the
**2026-07-20 STATE-Bench entry harness** above — landing via branch
`bench/state-bench` (push-to-`main`, **no version bump**, no crate change; a
Python-native bench driver whose *number* is pending hosted-model access). Earlier
cuts `v0.5.4` (`04a1145`) through `v0.5.10` remain documented in the sections
below.

## [0.5.4] — 2026-06-29

First GitHub Release cut since `v0.4.15`. The `v0.5.0 → v0.5.4` tags were pushed
(and auto-published to crates.io) but never got a GitHub Release; this section
consolidates that 0.5.x work under the current `v0.5.4` version, plus the
benchmark + release-drift work below. No new version bump — the workspace was
already at `0.5.4`; this cuts the release rather than bumping past it.

### Added (2026-06-29) — first authenticated benchmark baseline + two-axis parity table

- **bench: publish the first authenticated retrieval baseline (real local embedder,
  not `NoopEmbedding`).** Ran `mnemo-locomo-bench :: semantic_recall_bench` against a
  real `nomic-embed-text` (768-dim, via Ollama) over the bundled LongMemEval_M slice
  and recorded the scored result — recall@k / MRR per mode, embedder config, swept
  hybrid weights, commit SHA, and date — into
  [`docs/benchmarks/baseline.json`](docs/benchmarks/baseline.json) so the nightly
  regression gate has real numbers to compare against. Headline: `vector_only`
  **recall@1 = 0.739 (MRR 0.805)**, measured 2026-06-29 @ `640b7b1`. Reproduce:
  `ollama pull nomic-embed-text && cargo run --release -p mnemo-locomo-bench --bin semantic_recall_bench`.
- **docs: two-axis parity table in the README Benchmarks section.** Places mnemo's
  **measured retrieval** row (recall@1, the axis we actually ran) next to the
  **reported end-to-end QA-accuracy** numbers for Mem0 (93.4% LongMemEval) and Letta
  (~83% LoCoMo), with a bold caveat that these are **not directly comparable** —
  different metrics, different datasets — and that mnemo has **not** run the
  end-to-end QA-accuracy pipeline (no generative LLM in this harness). No win is
  claimed that was not measured; only the real retrieval row is published.
- **docs(drift, [#74]): removed the phantom `mnemo-bench-cf` references.** The
  Cloudflare-vs-mnemo bench crate was scoped but never built; the README now says so
  explicitly (not a workspace member, numbers not run) instead of implying it ships.

[#74]: https://github.com/sattyamjjain/mnemo/issues/74

### Security (2026-06-27) — v0.5.4 cut, bearer-token auth + truth-in-advertising

Workspace `0.5.3 → 0.5.4` (patch bump — adds a network auth floor, no breaking API change).

- **security: bearer-token auth on REST/gRPC; align README security claims with
  wired behavior; typed errors for unsupported features.**
  - **Network auth floor.** REST (`mnemo-rest`) and gRPC (`mnemo-grpc`) now read
    `MNEMO_AUTH_TOKEN`; when set, every REST request (except `GET /v1/health` +
    CORS preflight) must send `Authorization: Bearer <token>` → `401`, and every
    gRPC RPC must send matching `authorization` metadata → `UNAUTHENTICATED`.
    Constant-time compare via the new `mnemo_core::auth::bearer_token_matches`.
    When unset, both servers run open **and log a warning on startup** — never
    silently unauthenticated. New `router_with_auth(engine, Option<String>)` on
    both crates; `router()` reads the env var.
  - **Truth-in-advertising.** Audited every README security claim against the
    live code path. Corrected three over-claims to match reality and added a
    "Security: what is and isn't enforced today" table:
    - **MCP role-filter** — manifest block is *parsed + validated*, but **not
      invoked at tool dispatch**; the README no longer implies tool calls are
      filtered, and the CLI now logs a **warning** (was an info "loaded" line).
    - **MCP tool-catalog attestation** — pin is *parsed + validated*, but
      serve-time attestation is **not enforced**; CLI logs a warning.
    - **DPDPA consent-token-per-write** — `ConsentTokenGuard` is a **library**;
      the core `engine.remember` performs **no** consent check. README no longer
      claims it "refuses every remember."
  - **Typed errors for unsupported features** carry over the structured
    `Error::BackendUnsupported` variant (v0.5.3) — no security feature silently
    no-ops; loaded-but-unenforced features warn loudly.
  - Tests: `mnemo_core::auth` unit tests + REST auth integration tests
    (reject-missing / reject-wrong / accept-correct / health-exempt / open-mode).

### Fixed (2026-06-23) — v0.5.3 cut, typed `BackendUnsupported` for Postgres semantic recall

Workspace `0.5.2 → 0.5.3` (patch bump — error-type hardening + docs, no API break).

- **fix: Postgres `semantic_recall` now returns a typed `BackendUnsupported`
  error instead of silently returning empty; document DuckDB as the supported
  semantic backend.** The pgvector ANN path (`semantic` / `auto` / `graph` /
  `domain_scoped` / `reconstruct`) already failed loud, but with a generic
  `Error::Index(String)`. It now returns the structured
  `Error::BackendUnsupported { backend: "postgres", capability:
  "semantic_recall", detail }` so callers can match on `backend` / `capability`
  programmatically instead of string-sniffing the message; `detail` keeps the
  actionable guidance + tracking link ([#99]).
  - **New typed variant** `Error::BackendUnsupported` in
    `crates/mnemo-core/src/error.rs` (additive; the gRPC/REST error mappers
    fall through their existing wildcard arms → 500/internal).
  - **README backend capability matrix**: an explicit per-capability does/does-NOT
    table (DuckDB ✅ vs Postgres ❌-on-vector); crate-level doc note on
    `mnemo-postgres`.
  - **Test** `ann_search_fails_loud_not_silent_empty` upgraded to assert the
    structured variant (`backend`/`capability`), not just `is_err()`.

[#99]: https://github.com/sattyamjjain/mnemo/issues/99

### Added (2026-06-22) — v0.5.2 cut, real-embedder memory-quality result + Postgres semantic stub hard-errors

Workspace `0.5.1 → 0.5.2` (patch bump — bench + docs + a credibility-bug confirmation, no API change).

- **feat(bench): published real-embedder memory-quality result ([`bench/RESULTS.md`](bench/RESULTS.md)).**
  One honest, reproducible number from `semantic_recall_bench` run with a
  **real** local semantic embedder (`nomic-embed-text`, 768-dim, via Ollama —
  never `NoopEmbedding`) over the bundled LongMemEval_M slice: held-out
  semantic **recall@1 = 0.739 (MRR 0.805)**, with the default `auto` RRF
  fusion reported as-is (0.435 recall@1 — not cherry-picked).
  - **Engram-style token efficiency** (arXiv:2606.09900, lean-slice-vs-full-history,
    cited as a reference point not a parity claim): a lean top-5 retrieved
    slice costs **~89% fewer context tokens** than the full history. Added as
    a deterministic, no-LLM section + JSON field to the bench
    (`bench/locomo/src/bin/semantic_recall_bench.rs`).
  - **Honest caveats baked in:** single-run (5 in-process seeds, not
    restart-averaged); HNSW + RRF-weight selection sit near a noise floor (FID
    Lottery) so the swept "best" hybrid config flips run-to-run — `vector_only`
    is the one stable strong mode. This is retrieval quality + token
    efficiency, NOT end-to-end QA accuracy (which needs a generative LLM, not
    run here), and LongMemEval_M (45 q) not _S.
- **fix(postgres): semantic-recall stub hard-errors, confirmed + documented.**
  The pgvector ANN path returns a clear `Err` ("pgvector ANN search is not
  implemented…") instead of silently returning empty results
  (`crates/mnemo-postgres/src/pgvector_index.rs`, test
  `ann_search_fails_loud_not_silent_empty`). README documents **DuckDB +
  USearch as the supported semantic backend**; an unwired path must never
  return empty.
- **docs:** README Benchmarks section links `bench/RESULTS.md` (one number, one
  caveat, one source).

### Added (2026-06-21) — v0.5.1 cut, active-reconstruction recall strategy (MRAgent, arXiv:2606.06036)

Workspace `0.5.0 → 0.5.1` (patch bump — additive recall option, no breaking change).

- **feat(core): active-reconstruction recall strategy (MRAgent, arXiv:2606.06036).**
  Adds an opt-in `reconstruct` recall strategy
  (`RetrievalMode::Reconstruct` / `strategy = "reconstruct"`). Instead of
  returning only the top-k snippets, it retrieves candidates via the default
  hybrid RRF, walks the existing memory-graph `related_to` edges to gather
  linked/causal context, and synthesises a deterministic **belief-state
  node** returned ALONGSIDE the raw hits — MRAgent's cue → linked-context →
  reconstruct pattern.
  - **Additive, no pivot.** REMEMBER/RECALL/FORGET/SHARE are untouched; the
    `memories` top-k is exactly what `auto` returns, and the belief node is
    a new optional `RecallResponse.reconstruction` field. The default read
    path is unchanged.
  - **Deterministic** (rule-based synthesis, no LLM): same inputs → same
    belief node. `ReconstructedBelief { cue, summary, source_ids,
    linked_context_ids, confidence }`.
  - **Surfaced as a strategy parameter across all four protocols** (no new
    tool): MCP `strategy: "reconstruct"`, REST `?strategy=reconstruct`,
    gRPC `RecallRequest.strategy` (+ new `Reconstruction` message on
    `RecallResponse`), and pgwire `SELECT ... /*+ reconstruct */`.
  - **A/B bench** (`bench/locomo/src/bin/reconstruct_ab.rs`): measures
    gold-coverage@k of `reconstruct` vs. default RRF on an adversarially
    multi-hop fixture so the MRAgent "up-to-23%" claim can be checked on
    mnemo itself (fixture result: coverage@5 0.083 → 0.208). Framed honestly
    as a mechanism check, not an absolute-number claim.
  - **Tests** (`crates/mnemo-core/tests/reconstruct.rs`): belief node carries
    graph-linked context disjoint from sources; typed mode parity; default
    path unchanged; empty-corpus belief.

### Added (2026-06-21) — v0.5.0 cut, topic-document consolidation (Infini-Memory, arXiv:2606.10677)

Workspace `0.4.15 → 0.5.0` (minor bump — new public primitive).

- **feat(core,mcp): topic-document consolidation primitive (Infini-Memory,
  arXiv:2606.10677).** Adds `MnemoEngine::consolidate` and the MCP tool
  `mnemo.consolidate` — a caller-driven primitive that groups a chosen set of
  member memories into one revisable **topic document** ("each topic document
  serves as a semantic unit for collecting related evidence, preserving
  metadata, and revising facts over time"). It is the interactive, by-id
  sibling of the offline `run_consolidation` tag-cluster pass.
  - **Deterministic + protocol-agnostic.** New module
    [`crates/mnemo-core/src/query/consolidate.rs`](crates/mnemo-core/src/query/consolidate.rs)
    with `ConsolidateRequest { memory_ids, topic_name, agent_id?, summary?,
    supersede?, thread_id?, metadata? }` and `ConsolidateResponse`. Absent a
    caller `summary`, the body is a stable join of member contents ordered by
    `(created_at, id)`. Additive engine wrapper — no existing primitive
    changes.
  - **Evidence + provenance preserved.** The topic document records
    `consolidated_from` plus each member's source / timestamp / confidence in
    metadata, and writes `topic_document --consolidated_from--> member`
    relations so the set is retrievable as a unit. Members are permission-gated
    (`check_permission(Read)`) and decrypted on read; a missing/denied/deleted
    member aborts the whole op (nothing partial written).
  - **Fact revision keeps history.** `supersede` makes the new document
    version *N+1* (`prev_version_id → old`); the old document is **retained**
    (not soft-deleted — that would orphan the memory hash chain) and marked
    `Consolidated` with a `superseded_by` pointer. Reuse the same `topic_name`
    so the current-fact resolver (keyed on `topic`) collapses to the current
    view.
  - **Auditability — no dropped provenance.** Two new hash-chained
    `EventType` variants: `MemoryConsolidated` (every consolidation) and
    `MemoryRevised` (on supersede). `verify_integrity` and
    `verify_event_integrity` both stay valid after consolidation + revision.
  - **Surfaces.** MCP `mnemo.consolidate`
    ([`crates/mnemo-mcp/src/tools/consolidate.rs`](crates/mnemo-mcp/src/tools/consolidate.rs)),
    REST `POST /v1/consolidate`, gRPC `rpc Consolidate` (12 RPCs total).
    **pgwire is not extended** — it stays SQL-only (`SELECT`/`INSERT`/`DELETE`
    → recall/remember/forget); consolidation is a primitive-RPC operation, not
    a SQL statement.
  - **Tests** ([`crates/mnemo-core/tests/consolidate.rs`](crates/mnemo-core/tests/consolidate.rs)):
    consolidate-as-unit + relations, provenance metadata, revision-keeps-history,
    hash-chain integrity after consolidation/revision, permission gating,
    empty/missing rejection, and `EventType` serde round-trip.

### Fixed (2026-06-14) — Postgres semantic recall fails loud instead of silent-empty

- **fix(postgres): `PgVectorIndex` ANN search now errors instead of silently
  returning empty.** On the PostgreSQL backend, `semantic` / `auto` (hybrid) /
  `graph` / `domain_scoped` recall previously returned `Ok(vec![])` because
  `PgVectorIndex::search` / `filtered_search` were no-op stubs — making recall
  look like it legitimately found nothing, the most dangerous failure mode for
  a memory database. Both now return a clear `Error::Index` naming the
  limitation, the DuckDB alternative, and the tracking issue. Embeddings are
  still persisted to the pgvector column; only ANN *search* is unimplemented.
  The README now documents DuckDB as the supported vector backend, and real
  pgvector ANN is tracked in
  [#99](https://github.com/sattyamjjain/mnemo/issues/99). Adds a unit test
  asserting the fail-loud behaviour. **No change to the DuckDB path or any
  public API.**

### Added (2026-06-13) — real-embedder retrieval benchmark (`semantic_recall_bench`, bench-only)

- **bench(locomo): real-embedder retrieval-quality bench.** New
  [`semantic_recall_bench`](bench/locomo/src/bin/semantic_recall_bench.rs)
  bin measures mnemo's recall path with a **real local semantic embedder**
  (`nomic-embed-text`, 768-dim, via Ollama) instead of the degenerate
  `NoopEmbedding` the sibling scaffolds use. Metric = gold-document
  recall@1/@3/@5 + MRR, with a deterministic held-out tune/eval query
  split, an auditable `hybrid_weights` / `rrf_k` sweep on the tune split,
  and 5-seed averaging for stable numbers. Report + JSON at
  `bench/locomo/results/semantic_recall_2026-06-13.md`.
  - Held-out eval (mean of 5 seeds): `vector_only` recall@1 **0.739** / MRR
    **0.805** clearly leads; mnemo's default `auto` (RRF) fusion
    *underperforms* on recall@1 (0.452); a vector-dominant weight config
    (`[6,1,0,0]` k=30) recovers most of the gap (0.696) — a real,
    actionable finding that the default `auto` weights are worth revisiting
    for paraphrase-heavy single-fact recall.
  - **Bench-only**: no engine API, access protocol, or retrieval default is
    changed; not the official LLM-judged LongMemEval / LoCoMo QA score
    (gated; #44).

## [0.4.15] — 2026-06-13

The v0.4.10 → v0.4.15 accumulator (tags + GitHub Releases through `v0.4.15`
already exist). Sectioned here on the 2026-06-29 cut so the `[0.5.4]` section
above stays scoped to the 0.5.x line.

### Added (2026-06-13) — v0.4.15 cut, domain-scoped recall (MASDR-RAG, arXiv:2606.11350)

Workspace `0.4.14 → 0.4.15`. Pinned `cargo_pkg_version_matches_v0_4_15`
test and `docs/compat/version-skew-matrix.md` updated.

- **feat(recall): domain-scoped recall mode (anti vector-search-dilution,
  MASDR-RAG 2606.11350).** Adds `RetrievalMode::DomainScoped`, a recall
  mode that restricts the candidate set to a **metadata-defined
  sub-corpus before the dense similarity step**, then runs a single
  vector pass — so off-domain-but-semantically-similar records cannot
  dilute the top-k as the corpus scales.
  - **Diff-compatible:** additive enum variant (→ new `"domain_scoped"`
    strategy) plus an optional `RecallRequest.domain_scope` kwarg
    (`DomainScope { org_id, namespace, doc_class, tags }`,
    `#[serde(default)]`). No existing caller breaks; legacy `strategy`
    and typed `mode` paths are unchanged. A non-empty `domain_scope`
    selects the mode automatically even when `mode` is unset.
  - **Backend-agnostic + RBAC-gated:** the predicate resolves the
    sub-corpus id-set through the existing storage layer (DuckDB +
    PostgreSQL) and is composed with the permission filter, so the ANN
    sees only `(accessible ∩ in-domain)` ids.
  - **MCP surface:** `mnemo.recall` gains a `domain_scope` object
    (`crates/mnemo-mcp/src/tools/recall.rs`); named `domain_scope` (not
    `scope`) because `scope` already filters visibility.
  - **Dilution eval** (`crates/mnemo-core/tests/domain_scoped_dilution.rs`):
    on a corpus growing 50 → 1,000 docs, flat semantic P@10 collapses
    0.100 → 0.000 while domain-scoped holds at 1.000 — asserts the gap at
    the largest size is ≥ 0.05 (it is ~1.0). Plus `DomainScope::matches`
    + serde unit tests in `retrieval.rs`.

### Added (2026-06-11) — v0.4.14 cut, experience-memory tier (DocTrace, arXiv:2606.10921)

Workspace `0.4.13 → 0.4.14`. Pinned `cargo_pkg_version_matches_v0_4_14`
test and `docs/compat/version-skew-matrix.md` updated.

- **Experience-memory tier — cached plan replay (`REMEMBER_PLAN` /
  `RECALL_PLAN`).** DocTrace's two-tier idea (arXiv:2606.10921) as a
  mnemo **mode, not a new store**: tier 1 is the raw memory store; tier 2
  caches a *successful* retrieval/reasoning plan and replays it when a
  structurally-similar query recurs.
  - **New ops on the existing engine surface** (`MnemoEngine::remember_plan`
    / `recall_plan`, module `crates/mnemo-core/src/query/experience.rs`) —
    diff-compatible additions, no change to existing signatures and no new
    `MemoryType` variant.
  - `REMEMBER_PLAN` persists `{query-signature, steps, chunk ids, outcome
    score}` **only** when the outcome clears the success threshold (0.5) —
    failures are never cached. `RECALL_PLAN` returns the best stored plan
    whose signature Jaccard-matches above a threshold (default 0.7), else
    a miss.
  - **Backend-agnostic + RBAC/consent-gated for free:** plans are ordinary
    `MemoryRecord`s (reserved `__experience_plan__` tag + payload in
    `metadata`), so both the DuckDB and PostgreSQL backends work unchanged
    and scope/ACL visibility is enforced exactly like any record. Plan
    records are excluded from ordinary `recall`.
  - **Gated, default-off:** `MnemoEngine::with_experience_memory()` (or the
    `MNEMO_EXPERIENCE_MEMORY=1` env on the CLI/server). With the mode off,
    `remember_plan` errors and `recall_plan` misses, so default behaviour
    is unchanged.
  - **MCP surface:** `mnemo.remember_plan` + `mnemo.recall_plan`
    (`crates/mnemo-mcp/src/tools/experience.rs`).
  - **Tests** (`crates/mnemo-core/tests/experience_memory.rs`):
    store-on-success, replay-on-similar, no-replay-on-dissimilar,
    failures-not-cached, RBAC (private invisible / public visible), and
    mode-off inertness; plus signature/Jaccard unit tests.

### Added (2026-06-09) — agent-controlled memory mode (AutoMEM, arXiv:2606.04315)

- **Agent-controlled memory mode over the MCP tool surface.** Four new
  MCP tools let the agent manage a flat store it curates, so the *agent*
  (not an ingestion heuristic) decides what persists. Anchored on
  [arXiv:2606.04315](https://arxiv.org/abs/2606.04315) (*AutoMEM*).
  - `mnemo.mem_write` / `mnemo.mem_read` / `mnemo.mem_revise` /
    `mnemo.mem_forget` — **thin compositions over the verified
    `remember` / `recall` / `forget` primitives** plus a reserved
    `agent-managed` tag (`crates/mnemo-mcp/src/tools/agent_managed.rs`).
    No new engine enum or method. `mem_revise` = soft-`forget` old +
    `remember` corrected (newest wins); `mem_read` is `recall` scoped to
    the reserved tag.
  - **The default `mnemo.recall` pipeline is unchanged** and remains the
    fallback for single-shot queries; the agent-managed path is additive
    and for long-horizon write-control.
  - **Crossover eval** at
    `crates/mnemo-core/tests/agent_managed_crossover.rs` reproduces the
    paper's single-shot-vs-long-horizon framing on a multi-session
    fixture (12 tracked facts × 3 revisions + 12 incidental details),
    holding retrieval to BM25 to isolate write-control:
    - **fixed-pipeline wins single-shot** incidental recall (1.000 vs
      0.000 — it ingested everything the agent skipped);
    - **agent-managed wins long-horizon** current-fact F1 (1.000 vs
      0.500 — it revised in place, so no stale versions dilute
      precision).
  - MCP contract test (`crates/mnemo-mcp/tests/mcp_test.rs`) verifies the
    tag-scoping invariant (mem_read sees only agent-managed entries; the
    default pipeline still sees everything) and revise supersession.
  - Workspace version unchanged.

### Added (2026-06-08) — budgeted evidence retention (EMBER, arXiv:2606.05894)

- **`RecallRequest.retained_token_budget: Option<usize>` — opt-in
  budgeted evidence retention.** Extends the existing recall surface
  (no new enum); anchored on
  [arXiv:2606.05894](https://arxiv.org/abs/2606.05894) (*EMBER —
  Efficient Memory By Evidence Retention*).
  - When `Some(budget)`, the engine packs the recalled hits into at most
    `budget` retained tokens as verbatim **evidence capsules** (a short
    verbatim excerpt + a **retrieval key** that recovers the full
    record), ranked by a v0 **recoverability heuristic**
    (`recency × retrieval-hit-rate`) — a stand-in for EMBER's learned
    writer. New module `crates/mnemo-core/src/query/retained.rs`.
  - **Purely additive:** the `memories` list is unchanged; capsules ride
    in the new `RecallResponse.retained_evidence`
    (`RetentionReport { capsules, retained_tokens, candidates_examined,
    dropped, … }`). The default read path (no budget) is unaffected.
  - **Eval harness** at
    `crates/mnemo-core/tests/budgeted_evidence_retention.rs` reports
    recall@budget (and F1) on a LongMemEval-style fixture (60 gold facts)
    at a fixed 8192-token budget for budgeted-capsules vs
    naive-truncation: **1.000 vs 0.750** recall, budgeted using ~4.4K of
    the 8192 tokens — so the knob's value is measurable.
  - No protocol surface (MCP / REST / gRPC / pgwire) and no core
    retrieval default is changed; the field is `#[serde(default)]` so
    existing wire payloads deserialize unchanged. Workspace version
    unchanged.

### Added (2026-06-07) — bench-only, no version bump

- **bench/locomo: phase-aware cost attribution (construction/retrieval/generation)
  + 2606.06448 recommendations scorecard.** New `phase_cost` bin + reusable
  `mnemo_locomo_bench::phase_cost` module, anchored on
  [arXiv:2606.06448](https://arxiv.org/abs/2606.06448) (*Agent Memory:
  Characterization and System Implications of Stateful Long-Horizon
  Workloads*).
  - **Phase attribution:** splits every benchmark scenario's cost into the
    paper's three logical phases — **construction** (remember-path:
    embedding calls, prefill tokens, write latency), **retrieval**
    (recall-path: ANN + BM25 + graph + RRF latency, query tokens), and
    **generation** (downstream, *estimated* — mnemo does not generate). Emits
    a per-phase Markdown table (tokens, wall-ms, $-estimate at configurable
    per-1K rates) per scenario via `render_phase_table`.
  - **Scorecard:** `--scorecard-2606-06448` renders mnemo's PASS / PARTIAL /
    FAIL position against the paper's 10 §5 recommendations (quoted verbatim
    in `RECOMMENDATIONS`) as a 10-row table — currently **5 PASS · 5 PARTIAL
    · 0 FAIL**.
  - **Bench-only guardrail:** wired through the existing `mnemo-locomo-bench`
    bench entry point only; no access protocol (MCP / REST / gRPC / pgwire)
    and no retrieval default is touched. Token counts are `ceil(chars/4)`
    estimates and the generation phase is never an LLM call.
  - Workspace version unchanged (bench crate is `publish = false`); README
    bench section updated with a sample per-phase table.

### Added (2026-06-04) — v0.4.13 cut, AMP / memorywire interop adapter

Workspace `0.4.12 → 0.4.13`. Pinned `cargo_pkg_version_matches_v0_4_13`
test and `docs/compat/version-skew-matrix.md` updated.

> Note: the request that drove this cut referenced a "v0.4.4 cycle",
> but the canonical workspace manifest was already at 0.4.12. Per the
> "bump per the canonical Cargo manifest" instruction, this lands as
> 0.4.13 rather than downgrading.

- **New `mnemo-amp` crate — AMP / memorywire wire-format interop
  adapter.** Implements the AMP surface (5 operations × 4 memory types)
  as a `MemoryStore`-conformant layer over a real `MnemoEngine`, so any
  AMP-speaking client can drive mnemo's embedded DuckDB backend
  unchanged. Added to the workspace members + dep aliases.
  - **Wire format (`wire.rs`):** `AmpOp` (`remember` / `recall` /
    `forget` / `merge` / `expire`), `AmpMemoryType` (`episodic` /
    `semantic` / `procedural` / `working`), `AmpEnvelope` request,
    `AmpResult` response, and `schema()` returning a **JSON-Schema
    2020-12** document that pins the 5-op × 4-type surface with
    per-op conditional `required` lists.
  - **Store (`store.rs`):** `MemoryStore` async trait + `MnemoAmpStore`
    impl. `remember` → `engine.remember`; `recall` → `engine.recall`
    (top-k, default 5); `forget` → `engine.forget`. **`merge` and
    `expire` are thin compositions over existing primitives** — not
    assumed engine methods. `merge` folds N records into one
    consolidated record (`remember` + `SourceType::Consolidation`) and
    retires the originals (`forget` + `Consolidate`); it is explicitly
    *not* `engine.merge`, which is a branch-timeline merge. `expire`
    sets `expires_at` + runs `run_ttl_sweep` (there is no
    `engine.expire`).
  - **Router (`router.rs`):** `AmpRouter` single- and fan-out-backend
    entry; writes broadcast to every backend, recall fuses with RRF.
    Ships `rrf_fuse` (Reciprocal Rank Fusion) and `max_fuse` (max-score)
    combiners.
  - **HITL (`approval.rs`):** `ApprovalHook` trait + `AutoApprove` /
    `ClosureApprove` impls. Long-term (`semantic` / `procedural`)
    writes are diffed (`WriteDiff`) and gated before commit; on
    approval a `Decision` event is emitted through mnemo's existing
    hash-chained audit log, so the approve trail is tamper-evident.
    Short-term tiers bypass the gate.
- **Conformance suite (deterministic).** Mirrors the paper's
  cross-adapter checks: **recall@5** on a small labelled corpus driven
  end-to-end through the AMP surface over the embedded DuckDB backend,
  and **RRF-holds-under-rank-0-injection vs max-fusion** (RRF keeps the
  genuinely-relevant item on top; max-fusion is fooled by an
  adversarial rank-0 injection). 14 tests total (9 unit across
  wire/approval/router + 5 integration in
  `crates/mnemo-amp/tests/conformance.rs`) plus an `amp_conformance`
  smoke binary (`cargo run --release --bin amp_conformance -p
  mnemo-amp`) that runs all 5 ops + the fusion check and exits non-zero
  on any failure.
- **Docs:** README gains an AMP row in both the Access-Protocols table
  and the integrations list; `docs/src/integrations/mcp-server.md`
  gains an "AMP / memorywire conformance" section.

No managed-cloud dependency added; the `REMEMBER` / `RECALL` /
`FORGET` / `SHARE` primitive names are untouched; the embedded DuckDB
default is intact.

### Added (2026-06-02) — v0.4.12 cut, cost-aware answer-impact-scored recall

Workspace `0.4.11 → 0.4.12`. Pinned `cargo_pkg_version_matches_v0_4_12`
test and `docs/compat/version-skew-matrix.md` updated.

- **New `mnemo_core::query::evidence` module — cost-aware evidence
  budget.** An opt-in per-query budget that runs over the
  already-ranked recall candidate set and returns the smallest prefix
  that clears a configurable sufficiency bar, capped by an optional
  `max_evidence`. Purely subtractive: it only ever returns a prefix of
  the ranked order, so it cannot reorder or silently lower the
  retrieval's top-k ordering (enforced by an in-module property test).
  - `EvidenceBudget { max_evidence: Option<usize>, stop_when_sufficient:
    bool, sufficiency_threshold: f32, scorer: ScorerKind }` —
    serializable config, attached via the new additive
    `RecallRequest.evidence_budget: Option<EvidenceBudget>` field.
  - `stop_when_sufficient` returns early once the running per-chunk
    score sum clears `sufficiency_threshold`, so callers fetch the
    smallest set that clears the bar instead of front-loading.
- **New `EvidenceScorer` trait — pluggable answer-impact relevance
  signal.**
  - `CosineScorer` (default) — cosine of candidate vs query embedding,
    falling back to the fused retrieval score when embeddings are
    absent or degenerate (e.g. `NoopEmbedding`).
  - `DeltaScorer` — answer-impact scorer that rates a chunk by whether
    adding it to the already-selected evidence would change a
    downstream answer. The judgement is an **injectable closure**
    (`DeltaScorer::new(|ctx| …)`) so the core stays model-agnostic; the
    real LLM callback is supplied by the caller.
    `DeltaScorer::stub()` is a deterministic marginal-novelty heuristic
    for tests / offline use.
  - Attach a custom scorer via the new
    `MnemoEngine::with_evidence_scorer(Arc<dyn EvidenceScorer>)`
    builder. When a budget selects `ScorerKind::Delta` but no scorer is
    attached, the path falls back to cosine rather than erroring.
- **`RecallResponse.evidence_selection: Option<EvidenceSelectionReport>`
  diagnostics** (scorer name, examined vs returned counts, cumulative
  score, `stopped_early` / `capped` flags). Present iff the caller set
  `evidence_budget`. The budget is applied BEFORE `touch_memory`, so
  trimmed-away evidence is not mark-accessed (cost-aware on the write
  side too).
- **Tests:** 7 unit tests in `evidence.rs` (cap respected; early-stop
  at threshold ×2; scorer-trait swappable; injectable closure honoured;
  no-budget passthrough; property: larger budget is a prefix-superset)
  + 6 end-to-end integration tests in
  `crates/mnemo-core/tests/cost_aware_recall.rs` (cap, early-stop,
  no-budget passthrough, delta-scorer-attached, delta-without-scorer
  cosine fallback, prefix-invariant through the engine). The
  integration suite doubles as the public-API smoke test: it imports
  `EvidenceScorer` / `CosineScorer` / `DeltaScorer` from the built
  crate and exercises both scorers through `engine.recall`.

The default read path is unchanged — no `evidence_budget` means the
legacy front-loaded top-`limit`. No managed-cloud dependency added; the
`REMEMBER` / `RECALL` / `FORGET` / `SHARE` primitive names are
untouched; the embedded DuckDB default is intact.

### Added (2026-06-02) — v0.4.11 cut, MemFail per-operation fault-isolation harness

Workspace `0.4.10 → 0.4.11`. Pinned `cargo_pkg_version_matches_v0_4_11`
test and `docs/compat/version-skew-matrix.md` updated.

- **New `mnemo_core::eval::memfail` module** that decomposes each
  end-to-end recall into the three operation seams mnemo exposes —
  `remember` (store), `run_consolidation` (summarize), `recall`
  (retrieve) — and ships three adversarial probe sets engineered so a
  failed assertion is attributable to exactly one stage. Prior-art
  anchor: MemFail's per-operation eval decomposition; mnemo's harness
  targets the real MCP-native primitives, not invented seams.
  - **Store probes** check storage directly (no recall ranking, no
    consolidation): content round-trip + hash, batch atomicity,
    tag round-trip.
  - **Summarize probes** inspect post-consolidation state via direct
    storage reads: cluster emitted, needle string preserved verbatim
    in the `[Consolidated from N memories] …` bundle, originals
    flipped to `ConsolidationState::Consolidated`.
  - **Retrieve probes** assume store has passed in the same run, so
    any failure points at recall: direct-hit by needle string,
    tag-filter scoping.
- **`run_stale_context_fixture` (canonical MemFail "isolate the
  operation" case).** Writes the same fact twice (older write at high
  importance, newer write at low importance), asks the default hybrid
  ranker, and confirms it returns the older / stale record on top.
  Store + summarize probes pass — both records are present in storage
  with intact content hashes and no consolidation has run — so the
  harness attributes the failure to **retrieve**, not summarize. The
  v0.4.7 current-fact-resolver (`fact_key` post-processor on
  `RecallRequest`) is the documented opt-in mitigation; this fixture
  asserts the *attribution shape*, not the retriever's quality.
- **Integration test `crates/mnemo-core/tests/memfail_isolation.rs`**
  exercises the harness end-to-end against an in-memory engine and
  asserts (a) every stage probe passes on a well-formed engine and
  (b) the stale-context fixture lands on
  `Stage::Retrieve`, not `Stage::Summarize`.
- **Module-level unit tests** in `eval/memfail.rs` independently
  exercise each per-stage probe runner against a fresh engine.

5 new test functions (3 module-level unit tests + 2 integration
tests) added under the workspace `cargo test` surface. No new public
trait, no protocol surface change, no managed-cloud dep, no change
to the `REMEMBER` / `RECALL` / `FORGET` / `SHARE` primitive names or
the embedded DuckDB default.

### Added (2026-05-30) — GEM trajectory-correctness audit

- **New `mnemo_compliance::trajectory_audit` function** that replays
  the hash-chained event log for an `(agent_id, thread_id?)` scope and
  computes four GEM-aligned health signals over the state
  trajectory (anchor: [arXiv:2605.26252](https://arxiv.org/abs/2605.26252)):
  - **(a) unregulated-growth** — active-bank size over time vs a
    configured ceiling, with the full per-event timeline returned.
  - **(b) missing-semantic-revision** — facts written under the same
    `fact_key` where older writes were never deleted or redacted,
    listed by `(fact_id, stale_memory_ids)`.
  - **(c) capacity-driven-forgetting** — `MemoryDelete` events whose
    `strategy` payload is missing or outside the five named
    strategies (`soft_delete` / `hard_delete` / `decay` /
    `consolidate` / `archive`).
  - **(d) read-only-retrieval** — scopes that only ever emit
    `MemoryRead` / `RetrievalQuery` / `RetrievalResult` and never a
    write-shaped event.
- **Surfaced through the three protocols that already expose
  `mnemo.verify`:**
  - `mnemo.trajectory_audit` MCP tool (mirrors `mnemo.verify`'s
    `(agent_id, thread_id)` shape; adds `active_bank_ceiling`,
    `fact_key`, `named_forget_strategies` knobs).
  - `POST /v1/compliance/trajectory_audit` REST handler.
  - `TrajectoryAudit` gRPC RPC (new RPC on the existing
    `mnemo.v1.MnemoService`; new `TrajectoryAuditRequest` /
    `TrajectoryAuditResponse` / `TrajectoryFinding` messages — same
    proto file, no new package).
- **Wiring change:** `mnemo-compliance` is now a workspace dep of
  `mnemo-mcp`, `mnemo-rest`, and `mnemo-grpc`. The crate was already
  in the workspace; this just adds the dep edge so the protocol
  crates can call into it. No version bump (mnemo is on a doc-only
  v0.4.4 cycle window; this lands under `[Unreleased]` only).
- **9 unit tests** in `crates/mnemo-compliance/src/trajectory.rs`
  exercise each signal independently (happy-path, breach, fail,
  supersession-then-revision, mixed strategies, agent filter,
  empty-window error). The compliance crate's existing
  `export_audit_log` / `verify_ndjson_signed` tests remain
  untouched.

### Landing trace (2026-05-26)

v0.4.9 cut today (workspace 0.4.8 → 0.4.9). Next cycle's accumulator
opens here. The Auto-Dreamer offline-consolidation bench landed as
commit
[`c34039c`](https://github.com/sattyamjjain/mnemo/commit/c34039c83d5fd313201c62fa10f24187786466f0)
(2026-05-26 admin-merge of PR #96); the embedding-backend selection
bench + SLA-aware recommender is the headline feature of this cut.

### Added (v0.4.10 work-in-progress, 2026-05-29)

- **Feedback-driven consolidation trigger metric.** New
  [`crate::query::maturity`](crates/mnemo-core/src/query/maturity.rs)
  module ships a per-cluster scalar maturity score combining four
  components — access-recency, retrieval-hit success, edge degree in
  the graph store, and pairwise embedding redundancy — with tunable
  weights and saturations. The new
  `ConsolidationPolicy::MaturityDriven(MaturityPolicy)` engine config
  gates `run_consolidation` on the score crossing a configurable
  threshold; the default `ConsolidationPolicy::FixedSize` preserves the
  v0.4.x unconditional-on-size behaviour byte-for-byte. The policy is
  inherited by the existing `forget` and `checkpoint` paths
  (opportunistic, best-effort, never propagates errors), so all four
  access protocols (MCP / REST / gRPC / pgwire) pick it up without a
  new MCP tool. Internal prior-art anchor only:
  [arXiv:2605.28773](https://arxiv.org/abs/2605.28773) (FluxMem) —
  mnemo's policy is a structural cousin, not a reproduction.
- **New `bench/locomo/src/bin/maturity_consolidation.rs` scenario.**
  LoCoMo-style synthetic trace mixing "mature" (backdated, hit,
  edge-rich) and "fresh" (zero-access, no-edge) clusters; runs both
  `FixedSize` and `MaturityDriven` arms and reports `active_bank_ratio`,
  `recall_post`, `clusters_consolidated`, `overreach` (fresh clusters
  consolidated), and a Pareto verdict on the user-specified
  (recall_retained × store_reduction) axes. Markdown + JSON summaries
  written to `bench/locomo/results/maturity_<date>.{md,json}`.
- **2026-05-29 bench result on the synthetic trace.** Maturity arm
  achieves equal recall (`1.0` vs `1.0`), zero overreach (`0` vs `3`
  median), and ~7.7× faster consolidation pass (`~17ms` vs `~133ms`),
  but consolidates fewer clusters, so `active_bank_ratio` is `0.625`
  vs the fixed arm's `0.25`. No Pareto win on the (recall × reduction)
  plane; selectivity win on overreach. **No release tag** until a
  scenario demonstrates a true Pareto improvement.

## [0.4.9] - 2026-05-26

Embedding-backend selection bench + SLA-aware recommender +
Auto-Dreamer-shaped offline consolidation bench. **Measurement and
recommendation only** — no retrieval-default change, no RRF-weights
change, no managed-cloud default. The embedded-first wedge stays.

### Added

- **New `bench/embeddings` crate (criterion + lib +
  `mnemo bench embeddings --slo-ms <N>` CLI subcommand).** Anchored
  on [arXiv:2605.23618](https://arxiv.org/abs/2605.23618) (GE2 vs
  local encoders — quality + latency). For every *configured*
  backend (Noop + a bench-local hashing baseline always;
  `OpenAiEmbedding` if `OPENAI_API_KEY` is set; `OnnxEmbedding` if
  `MNEMO_ONNX_MODEL_PATH` is set AND `mnemo-core` is built with the
  `onnx` feature), the bench measures nDCG@10, recall@10, p50/p95
  single-vector embed latency, and throughput at batch sizes
  1 / 8 / 32 on a 50-document / 10-query labeled fixture checked
  into `bench/embeddings/data/`. The recommender picks the
  **highest-nDCG backend whose p95 ≤ the SLO** and reports the
  explicit nDCG gap vs the absolute best-quality backend. No new
  production embedding backend was added — the bench-local
  `hashing-baseline` is a lexical floor that lives in
  `bench/embeddings/src/lib.rs`, not in `mnemo-core`. See
  [`bench/embeddings/README.md`](bench/embeddings/README.md) for
  the full "what this bench is NOT" block.

- **New `Command::Bench(BenchCommand)` clap variant on the `mnemo`
  binary.** Dispatches `mnemo bench embeddings --slo-ms <N>` to
  `mnemo_embeddings_bench::run_all` + `recommend` + `render_table`.
  No other CLI shape changes; existing subcommands
  (`baseline`, `mcp-server`, `eval`) are untouched.

- **Auto-Dreamer-shaped offline consolidation bench**
  ([`bench/locomo/src/bin/auto_dreamer_consolidation.rs`](bench/locomo/src/bin/auto_dreamer_consolidation.rs)).
  Exercises the engine's existing
  [`mnemo_core::query::lifecycle::run_decay_pass`](crates/mnemo-core/src/query/lifecycle.rs)
  + [`run_consolidation`](crates/mnemo-core/src/query/lifecycle.rs)
  path end-to-end on a synthetic multi-session trajectory (8 sessions ×
  25 facts × 5 trials by default) and reports
  `active_bank_ratio = post / pre` (expects `< 1.0`) and held-out
  `recall_post >= recall_pre`. Emits a Markdown report
  (`bench/locomo/results/auto_dreamer_<YYYY-MM-DD>.md`) plus a JSON
  summary (`auto_dreamer_<YYYY-MM-DD>.json`) carrying
  `active_bank_ratio`, `recall_pre`, `recall_post`, and the
  offline-pass elapsed time. No new public API surface.

### Landing trace (2026-05-23)

v0.4.8 cut today (workspace 0.4.7 → 0.4.8). Next cycle's accumulator
opens here. The v0.4.7 land was commit
[`df84482`](https://github.com/sattyamjjain/mnemo/commit/df84482)
(2026-05-22 admin-merge of PR #88 — MINTEval interference scenario
+ current-fact resolver).

## [0.4.8] - 2026-05-23

PEEK-anchored orientation cache (constant-token "context map")
recall mode. Adds an opt-in post-processor over the standard recall
result set that maintains a per-namespace, fixed-token-budget map of
entities + `UPPER_SNAKE` constants + fenced schema fragments
distilled from each hit, and returns a bounded rendering alongside
`top-k`. Default read path is unchanged.

### Added

- **New `mnemo_core::query::orientation_cache` module.** Carries
  `OrientationCacheConfig { namespace, token_budget,
  include_in_response, distill }` + `OrientationCacheStore`
  (in-process `Arc<RwLock<HashMap<namespace, ContextMap>>>`) +
  `RenderedContextMap { namespace, entities, constants, schemas,
  token_estimate, budget, hit_count }` + a heuristic `distill()`
  + a priority-evictor `evict_to_budget()` + a one-shot
  `update_and_render()` driver. The Distiller extracts
  capitalized noun phrases (entities), `UPPER_SNAKE = value`
  / `UPPER_SNAKE: value` pairs (constants), and fenced ```` ``` ````
  blocks + `CREATE TABLE` / `interface` / `type` / `struct`
  declarations (schemas). The Evictor scores entries by
  `freq × recency × (1 - token_share)` and drops the lowest
  until under budget. **8 unit tests** cover entity / constant /
  schema extraction, namespace derivation + override, bounded
  rendering, eviction at small budget, namespace isolation,
  read-only config, and budget invariance across many updates.

- **New `RecallRequest.orientation_cache: Option<OrientationCacheConfig>`
  field.** Backwards-compatible additive field. When `Some` AND
  the engine has an `OrientationCacheStore` attached via
  `MnemoEngine::with_orientation_cache_store()`, the engine
  updates the per-namespace map from each hit and (when
  `include_in_response = true`) returns the bounded rendering in
  the response.

- **New `RecallResponse.orientation_cache: Option<RenderedContextMap>`
  field.** Surfaces the bounded map when the cache is enabled
  AND the config did not set `include_in_response = false`.

- **New `MnemoEngine.orientation_cache_store` +
  `with_orientation_cache_store()` builder.** Per-engine attach
  point for the in-process namespace-keyed store. Mirrors the
  existing `with_cache` / `with_encryption` pattern.

- **MCP `recall` tool param `orientation_cache`.** New
  `RecallOrientationCacheInput { namespace, token_budget,
  include_in_response, distill }` in
  [`crates/mnemo-mcp/src/tools/recall.rs`](crates/mnemo-mcp/src/tools/recall.rs)
  threaded through the MCP tool dispatch. The MCP response JSON
  carries a top-level `orientation_cache` field when the rendered
  map is present.

- **REST recall query params** `orientation_cache`,
  `orientation_namespace`, `orientation_token_budget`,
  `orientation_include_in_response`, `orientation_distill` on
  `GET /v1/memories`. Wires through to the config without
  changing the default query semantics.

- **gRPC `OrientationCacheRequest` + `OrientationCacheResponse` +
  `OrientationEntry`** added to `mnemo.proto`. New optional
  `RecallRequest.orientation_cache` field (proto field 14) +
  new optional `RecallResponse.orientation_cache` field (proto
  field 3). Wired in `crates/mnemo-grpc/src/lib.rs` recall
  handler.

- **pgwire `/*+ orientation_cache */` SQL hint comment.** The
  parser sets `SelectQuery.orientation_cache = true` when the
  query contains this directive; the server then attaches a
  default `OrientationCacheConfig::new()` to the underlying
  `RecallRequest`. Minimal first-cut surface (no namespace /
  budget overrides through SQL yet — REST / MCP / gRPC carry the
  full config knobs).

- **New `bench/locomo/src/bin/orientation.rs`** — PEEK-shaped
  repeated-context scenario. For each `K ∈ {3, 6, 10, 15}`, seeds
  30 shared-context facts referencing a fixed cast + issues `K`
  related recall calls per trial, comparing hybrid-only vs
  orientation-cache arms. Reports p50 payload tokens per call +
  p50 latency + top-1 hit parity. Writes
  `bench/locomo/results/orientation_<YYYY-MM-DD>.md`. Anchored
  on [arXiv:2605.19932](https://arxiv.org/abs/2605.19932) in
  the module doc-comment.

- **README "Repeated-context recall — orientation cache (v0.4.8)"
  subsection** under Access Protocols with primary-source link +
  pointer to the module + pointer to the bench scenario +
  explicit "not a learned summariser / not a context-window
  extender / not persisted" disclaimers.

- **`bench/locomo/README.md`** gains a row for the new
  `orientation` bin alongside the existing three.

### Changed

- **Workspace version 0.4.7 → 0.4.8.** Cargo.toml workspace +
  internal-crate dep pins; python/pyproject.toml;
  sdks/typescript/package.json; sdks/go/mnemo.go (`Version`
  const); python/mnemo/__init__.py `__version__`. Regression
  tests bumped: `cargo_pkg_version_matches_v0_4_8` (renamed from
  `_v0_4_7`) + `test_v0_4_8_pinned` (renamed from
  `_v0_4_7_pinned`).

- **~30 RecallRequest construction sites** across mnemo-core
  (engine + benches + integration tests), mnemo-grpc,
  mnemo-pgwire, mnemo-rest, mnemo-letta, mnemo-mcp tests,
  mnemo-cli, python/src/lib.rs, and bench/locomo bins updated
  to set `orientation_cache: None` (matches the additive-field
  pattern from v0.4.4's `mode` addition and v0.4.7's
  `current_fact_resolver` addition).

### Honest scope — what's NOT in v0.4.8

- **NOT a write-side memory consolidator.** The cache only
  summarises hits as they pass through recall; it does not
  rewrite, compact, or persist any memory record on disk.
- **NOT a learned summariser.** The Distiller is heuristic by
  choice — regex-free pure-Rust extraction of capitalized
  entities, `UPPER_SNAKE` constants, and fenced/declared schemas.
  An LLM-backed Distiller variant is parked for v0.5.x; treat the
  extracted entries as pointers, not paraphrases.
- **NOT a context-window extender.** The rendered map fits inside
  the recall response payload and is bounded by the caller's
  `token_budget` (default 512). The cache does not bypass any
  model context limit.
- **NOT a faithful PEEK reproduction.** PEEK uses a learned
  prefix encoder and a write-side update path. This module
  adopts the *orientation map + constant-token budget* shape
  only. The bench measures the *shape* of the savings, not the
  absolute number PEEK reports.
- **NOT persisted.** The store is in-process
  (`Arc<RwLock<HashMap<..>>>`). Restart drops it. Persistence
  to DuckDB / Postgres is a v0.5.x knob.
- **Token estimate is `(len / 4)`-heuristic, not `tiktoken-rs`.**
  Calibration against a real tokenizer is a follow-up for
  production sizing decisions.
- **pgwire surface is minimal.** Only the boolean hint
  `/*+ orientation_cache */` is parsed; namespace + budget
  overrides through SQL are deferred. Full-config knobs travel
  through MCP / REST / gRPC today.

## [0.4.7] - 2026-05-22

Interference bench scenario + current-fact resolver recall mode
(MINTEval-anchored). Adds an opt-in post-processor over the standard
recall result set that groups candidates by a caller-chosen
`fact_key` (typical: `"fact_id"`) and keeps the most-recent write
per group, with the older versions optionally returned as a
supersession chain. Default read path is unchanged.

### Added

- **New `mnemo_core::query::current_fact_resolver` module.** Carries
  `CurrentFactResolverConfig { fact_key, include_supersession_chain }`
  + `resolve()` + `ResolverOutput { kept, superseded }`. The resolver
  groups by JSON metadata pointer, picks the record with the latest
  `updated_at` (ties → higher score → higher UUID v7), and returns
  the older entries as a `SupersededRecord` chain. Records missing
  the `fact_key` pass through untouched. **6 unit tests**: most-recent
  wins, supersession chain when enabled, records-without-fact-key
  pass through, multi-group resolution, empty-candidate-set,
  integer fact-id support.

- **New `RecallRequest.current_fact_resolver: Option<CurrentFactResolverConfig>`
  field.** Backwards-compatible additive field on the existing
  request struct. When `Some`, the engine dispatches the resolver
  AFTER the normal hybrid recall completes. **The default read path
  is unchanged.**

- **New `RecallResponse.superseded: Option<Vec<SupersededRecord>>`
  field.** Surfaces the supersession chain when the resolver was
  enabled with `include_supersession_chain = true` AND any
  candidates were actually dropped. `SupersededRecord` carries
  `{id, fact_id, superseded_by, superseded_at, prior_updated_at}`
  so an auditor can reconstruct the timeline.

- **MCP `recall` tool param `current_fact_resolver`.** New
  `RecallCurrentFactResolverInput { fact_key, include_supersession_chain }`
  in [`crates/mnemo-mcp/src/tools/recall.rs`](crates/mnemo-mcp/src/tools/recall.rs)
  threaded through the MCP tool dispatch. The MCP response JSON
  carries a top-level `superseded` field when the chain is present.

- **REST recall query params** `current_fact_key` +
  `current_fact_include_chain` on `GET /v1/memories`. Wires
  through to the resolver config without changing the default
  query semantics.

- **New `bench/locomo/src/bin/interference.rs`** — MINTEval-shaped
  scenario. For each `K ∈ {1, 3, 5, 10}`, seeds 50 distractor
  facts + revises a target fact `K + 1` times under the same
  `fact_id`, then queries via both the default read path and the
  resolver arm. Reports current-fact-accuracy@K + supersession-chain
  length per K, p50 latency for each arm. Writes
  `bench/locomo/results/interference_<YYYY-MM-DD>.md`. Anchored
  on [arXiv:2605.18565](https://arxiv.org/abs/2605.18565) in the
  module doc-comment.

- **README "Memory under interference — current-fact resolver
  (v0.4.7)" subsection** under Access Protocols with primary-source
  link + pointer to the resolver module + pointer to the bench
  scenario + explicit "not a contradiction detector / not a
  write-side guard" disclaimers.

- **`bench/locomo/README.md`** gains a row for the new `interference`
  bin alongside the existing `mnemo-locomo` + `grep_vs_vector_replay`
  rows.

- **`tests/readme_no_marketing_phrases.rs` banlist extended** with
  four MINTEval overclaim phrasings: `MINTEval-compliant`,
  `interference-proof`, `supersession-perfect`, `MINTEval-resistant`.

### Changed

- **Workspace version 0.4.6 → 0.4.7.** Cargo.toml workspace +
  internal-crate dep pins; python/pyproject.toml;
  sdks/typescript/package.json; sdks/go/mnemo.go (`Version` const
  + package doc-comment); python/mnemo/__init__.py `__version__`.
  Regression tests bumped: `cargo_pkg_version_matches_v0_4_7`
  (renamed from `_v0_4_6`) + `test_v0_4_7_pinned` (renamed from
  `_v0_4_6_pinned`).

- **~20 RecallRequest construction sites** across mnemo-core,
  mnemo-grpc, mnemo-pgwire, mnemo-rest, mnemo-letta, mnemo-mcp
  tests, integration tests, benches, bench/locomo bins, and
  mnemo-cli updated to set `current_fact_resolver: None` (matches
  the additive-field pattern from v0.4.4's `mode` addition).

### Honest scope — what's NOT in v0.4.7

- **NOT a contradiction detector.** Two records with the same
  `fact_key` value are treated as versions of one fact; the
  resolver does NOT inspect content semantics. The operator picks
  `fact_key` to mean what they want.
- **NOT a write-side guard.** The resolver only re-ranks reads;
  contradictory writes are accepted by the existing engine path
  unchanged. Operators wanting write-side conflict prevention use
  the existing `crate::query::conflict` module.
- **NOT a gRPC proto extension.** The new field is wired through
  Rust + MCP + REST today. The gRPC proto and pgwire SQL surface
  carry `current_fact_resolver: None` as a padding default; the
  full grpc proto bump is deferred to v0.5.x.
- **NOT a faithful MINTEval reproduction.** The bench bin uses a
  synthetic distractor corpus + deterministic exact-content
  scoring. The official MINTEval metric (GPT-judge over a curated
  benchmark corpus) is gated behind the same secrets as
  [#44](https://github.com/sattyamjjain/mnemo/issues/44).
- **NOT a re-ranker for the underlying retrieval.** The resolver
  runs over whatever candidates the underlying `RetrievalMode`
  produced. It does not re-issue a query.

## [0.4.6] - 2026-05-21

Substrate-anchor release. Net-new v0.4.6 surface: a vertical-slice
implementation of the [`golem:vector@1.0.0`](https://github.com/golemcloud/golem-ai/issues/21)
WIT contract, two-crate host-runner architecture, with mnemo-core
on the host side because the engine's C++ deps (DuckDB + USearch)
cannot compile to `wasm32-wasip2`.

### Added

- **New `crates/mnemo-golem-wit` workspace crate.** WIT-bindings
  crate built with `cargo-component v0.21.1`. Implements 3 of 30
  upstream functions — `upsert-vector`, `search-vectors`,
  `delete-vectors` — each delegating to a host import. Compiles
  cleanly to `wasm32-wasip2`; the release artifact is ~73K at
  `target/wasm32-wasip1/release/mnemo_golem_wit.wasm`. WIT
  package is `mnemo:golem-vector@0.1.0` (namespaced under
  `mnemo:` to signal the subset, not the full upstream contract).

- **New `crates/mnemo-golem-host` workspace crate.** Rust host
  crate that owns an `Arc<MnemoEngine>` and supplies the WIT host
  imports. Ships:
  - `trait MnemoGolemProvider` — async Rust shape of the three
    host imports.
  - `struct MnemoGolemHost { engine }` — backs the trait with
    mnemo's `remember` / `recall` (semantic top-K) / `forget`
    (HardDelete) operations; maps the WIT `collection` parameter
    to mnemo's `agent_id` namespace.
  - **5 integration tests**: put → search round-trip,
    collection-scoping isolates writes, delete-removes-only-targeted-ids,
    upsert-rejects-empty-vector, search-rejects-empty-query.
  - **End-to-end example** at
    `examples/golem_agent_round_trip.rs` showing REMEMBER →
    RECALL → DELETE through a real `MnemoEngine` (3 upserts + 1
    search + 1 delete + 1 post-delete search).

- **New research-anchor doc**
  [`docs/research/golem-vector-wit-provider.md`](docs/research/golem-vector-wit-provider.md)
  documenting the architectural reality (DuckDB / USearch ↛ WASM),
  the two-crate host-runner split, the WIT subset shipped today,
  the wasmtime-component-loader wiring step explicitly deferred
  to v0.5.x, the per-function gap list (6 Collections + 8
  Vectors-extended + 5 Search-Extended + 3 Analytics + 5
  Namespaces + 4 Connection = **27 deferred**, **3 shipped** = 30
  upstream contract), and the explicit non-overclaim disclaimers
  (NOT a Golem-durability claim, NOT a multi-provider abstraction,
  NOT a real embedder integration, NOT a bounty-claimable
  submission for the full contract).

- **README "mnemo as a golem:vector provider (v0.4.6)" subsection**
  under Access Protocols with primary-source link to
  golemcloud/golem-ai#21 + pointer to both new crates + pointer to
  the research anchor + explicit honest framing of the deferred
  wasmtime wiring.

- **`tests/readme_no_marketing_phrases.rs` banlist extended** with
  five golem:vector overclaim phrasings: `Golem-durable by
  construction`, `golem:vector-compliant`, `Qdrant killer`,
  `Pinecone killer`, `WIT-component-perfect`.

### Changed

- **Workspace version 0.4.5 → 0.4.6.** `Cargo.toml` workspace +
  internal-crate dep pins; python/pyproject.toml; sdks/typescript
  package.json; sdks/go mnemo.go (`Version` const + package
  doc-comment); python/mnemo/__init__.py `__version__`. Regression
  tests bumped: `cargo_pkg_version_matches_v0_4_6` (renamed from
  `_v0_4_5`) + `test_v0_4_6_pinned` (renamed from `_v0_4_5_pinned`).

- **Workspace member list extended** with two new entries:
  `crates/mnemo-golem-wit` and `crates/mnemo-golem-host`.

### Honest scope — what's NOT in v0.4.6

- **NOT the full golem:vector contract.** 3 of 30 functions
  shipped; 27 deferred to v0.5.x with the per-interface rationale
  in the research doc.
- **NOT the wasmtime-component-loader wiring.** The Rust trait +
  mnemo-core integration ship today; the
  `wasmtime::component::Linker` + bindgen host bindings + async
  trampoline step is documented as a v0.5.x row.
- **NOT a Golem-durability claim.** Component runs on Golem the
  same way any guest does; mnemo does not introspect Golem's
  checkpoint protocol.
- **NOT a multi-provider abstraction.** mnemo is one provider;
  routing across Qdrant / Pinecone / Milvus / pgvector is out of
  scope.
- **NOT a real embedder integration.** Vectors arrive
  pre-computed via the WIT; today's slice uses `NoopEmbedding` for
  the test setup and demonstrates the wiring, not the embedder.
- **NOT a bounty-claimable submission for the full upstream
  contract.** The vertical slice + host-runner scaffold is the
  bounty's first deliverable shape; a v0.5.x follow-up closes the
  remaining 27 functions to be bounty-claimable.

## [0.4.5] - 2026-05-20

Substrate-anchor release. Net-new v0.4.5 surface: an attention-state
memory store, anchored on the
[arXiv 2605.18226 Context Memorization](https://arxiv.org/abs/2605.18226)
result (Okoshi et al., Institute of Science Tokyo + Imperial College
London, surfaced 2026-05-19). The paper names "a lightweight,
lookup-based memory of precomputed attention states" as the
substrate prefix-augmented inference reaches for; v0.4.5 ships that
substrate without claiming to implement the full Context
Memorization mechanism (the producer + consumer are external — see
the honest scope below).

### Added

- **New `crates/mnemo-attention-state` workspace crate.** Typed
  `AttentionStateStore` trait + `InMemoryAttentionStateStore`
  reference implementation + serializable `AttentionStateRecord`
  envelope (id / agent_id / prefix_hash / model / state_blob /
  blob_sha256_hex / ttl_seconds / created_at). Six unit tests cover
  put → get round-trip, get-miss, put-overwrites-existing,
  SHA-256-matches-input, agent-scoping-isolates-writes,
  delete_for_agent-removes-only-that-agents-records.

- **2 new MCP tools** on `MnemoServer`: `mnemo.attention_state.put`
  and `mnemo.attention_state.get`. Tools dispatch into the store
  when `MnemoServer::with_attention_state(...)` is configured at
  startup; **unconfigured calls return a spec-shaped error result,
  not a panic.** Blobs travel hex-encoded on the JSON-RPC wire to
  keep transport string-safe. Three integration tests in
  `crates/mnemo-mcp/tests/attention_state_tools.rs` exercise the
  store contract through the same `AttentionStateStore` trait the
  tools dispatch into.

- **New research-anchor doc**
  [`docs/research/context-memorization-2605.18226.md`](docs/research/context-memorization-2605.18226.md)
  documenting what the paper measures, where mnemo fits (store
  only), what this anchor is explicitly NOT (not a Context
  Memorization implementation, not an inference-runtime
  integration, not a RECALL fast-path, not a stability claim on
  blob format, not encrypted-at-rest at the storage trait, not a
  benchmark), the operator recipe for putting the substrate to
  work today, and the v0.4.4 vs v0.4.5 layering (RetrievalMode
  HarnessAware vs the new orthogonal attention-state store).

- **README "Attention-state-memory substrate (v0.4.5)" subsection**
  under Access Protocols with primary-source link to arXiv 2605.18226
  + pointer to the new crate + pointer to the new MCP tools +
  explicit honest framing of producer / consumer scope.

- **`tests/readme_no_marketing_phrases.rs` banlist extended** with
  four Context-Memorization overclaim phrasings:
  `Context-Memorization-compliant`, `attention-state-compatible`,
  `KV-cache-portable`, `prefix-cache by construction`.

### Changed

- **Workspace version 0.4.4 → 0.4.5.** Cargo.toml workspace +
  internal-crate dep pins; python/pyproject.toml; sdks/typescript
  package.json; sdks/go mnemo.go (`Version` const + package
  doc-comment); python/mnemo/__init__.py `__version__`. Regression
  tests bumped: `cargo_pkg_version_matches_v0_4_5` (renamed from
  `_v0_4_4`) + `test_v0_4_5_pinned` (renamed from `_v0_4_4_pinned`).

- **`mnemo-mcp` adds `hex = { workspace = true }` dependency.** The
  new MCP tool methods hex-encode / hex-decode the state blob at
  the JSON-RPC wire boundary.

### Honest scope — what's NOT in v0.4.5

- **NOT a Context Memorization implementation.** mnemo does not
  extract prefix attention states from any inference runtime. The
  producer is out of scope.
- **NOT an inference-runtime integration.** mnemo does not wire to
  vLLM, TGI, Triton, or any specific runtime. The mechanism is
  transport-agnostic.
- **NOT a RECALL fast-path.** Existing semantic + BM25 + graph +
  recency hybrid retrieval does NOT consult the attention-state
  store. Substrates sit orthogonal. Future v0.5.x row may explore
  the composition.
- **NOT a stability claim on the blob format.** The
  `AttentionStateRecord` schema is starter; pin v0.4.5 minor if
  relying on byte-level layout.
- **NOT encrypted-at-rest at the storage trait.** The in-memory
  reference store holds bytes as `Vec<u8>`. Encryption is the
  operator's responsibility at the tool / engine layer using the
  existing `mnemo-core::encryption::ContentEncryption` helper.
- **NOT a persistent backend.** v0.4.5 ships only
  `InMemoryAttentionStateStore`. A DuckDB / PostgreSQL backend is
  a future minor.
- **NOT a benchmark.** No bench harness compares attention-state
  lookup cost vs prefix recomputation.

### Also-landed in this cycle

- **(2026-05-18) — LangGraph 1.x checkpoint adapter wrap-up**
  shipped 2026-05-18 in commit
  [`0cf6f39`](https://github.com/sattyamjjain/mnemo/commit/0cf6f3939c92cbe494eb8b1118faf9595b74f427)
  before today's substrate row. `python/mnemo/checkpointer.py` adds
  **`MnemoCheckpointer`** as the canonical class name; the legacy
  `ASMDCheckpointer` is preserved as a back-compat alias so existing
  `from mnemo.checkpointer import ASMDCheckpointer` imports continue
  to work. The module docstring now documents the LangGraph 1.x
  ``BaseCheckpointSaver`` surface coverage explicitly: primaries
  (`get_tuple`, `put`, `delete_thread`) are implemented; `list` +
  `put_writes` are stubs with the contract recorded in the
  docstring. New tests in
  [`python/tests/test_langgraph_checkpointer.py`](python/tests/test_langgraph_checkpointer.py)
  cover put→get_tuple round-trip, thread isolation, branch
  round-trip, delete_thread, stub-method contracts, and the
  back-compat-alias identity (`ASMDCheckpointer is MnemoCheckpointer`).
  Tests use a `_FakeMnemoClient` shim so the suite does NOT spawn
  the mnemo binary. New
  [`examples/langgraph_checkpointer.py`](examples/langgraph_checkpointer.py)
  shows a 5-line `StateGraph` + `MnemoCheckpointer` integration.
  [`python/README.md`](python/README.md) integrations table swaps
  `ASMDCheckpointer` → `MnemoCheckpointer` with the back-compat
  alias annotated inline. `mnemo.availability` registers both names
  so the soft-import probe surfaces either.

  **Honest scope:** the wrap-up closed the parked
  `mnemo-langgraph` v0.4.4-backlog item via the existing Python
  adapter; **no new Rust crate shipped** because LangGraph is
  Python-only and a Rust `crates/mnemo-langgraph/` shell would
  have no downstream consumer. The Python adapter's `list` +
  `put_writes` stubs are unchanged — the v0.4.4-backlog inventory
  was moved from "ship the crate" to "implement `list` + per-thread
  `put_writes` enumeration" as a v0.5.x follow-up.

## [0.4.4] - 2026-05-17

Substrate-anchor release. Twelve days of `[Unreleased]` accumulator
(2026-05-05 → 2026-05-17) shipping the four substrate-composition
anchors of the cycle (Dreams curator, ARGUS read-side audit,
DELEGATE-52 outcome-diff, MCP 2026 Roadmap Enterprise-Readiness)
plus today's two-PR ship:

- **PR-A (bench scaffold)** — new `[[bin]] grep_vs_vector_replay` in
  `bench/locomo` routing a LongMemEval-shaped slice through
  `mnemo.recall` in three modes (`vector_only` / `bm25_only` /
  `rrf_hybrid`) and emitting a Markdown table per run. Reproduces
  the Sen et al. arXiv:2605.15184 experiment design against mnemo's
  own substrate. Operator-runnable today against the bundled
  45-record `longmemeval_m.jsonl`; the gated 116-question slice +
  GPT-judge-scored official metric require the same secrets as
  [#44](https://github.com/sattyamjjain/mnemo/issues/44).
- **PR-B (RetrievalMode typed enum)** — new `mnemo_core::retrieval`
  module landing `RetrievalMode` typed enum (`VectorOnly` / `Bm25Only`
  / `HybridRrf` / `Graph` / `HarnessAware { harness, format }`) + 5
  starter `HarnessAware` adapters (`ClaudeCodeEnvelope`,
  `CodexEnvelope`, `GeminiCliEnvelope`, `ChronosEnvelope`,
  `GenericEnvelope`). `RecallRequest.mode: Option<RetrievalMode>` is
  added as an **additive** field — the legacy
  `RecallRequest.strategy: Option<String>` stays in place and SDKs
  (Python / TypeScript / Go) continue to work unchanged. New
  research-anchor doc at
  [`docs/research/grep-vs-vector-2605.15184.md`](docs/research/grep-vs-vector-2605.15184.md).
  README "Why mnemo" gains a paragraph framing the
  `HarnessAware` lever against the paper's envelope-format finding.

### What this release is NOT

- Not a breaking change for SDK callers — `strategy: Option<String>`
  is preserved; new `mode` field is additive.
- Not a stability claim on the 5 `HarnessAware` adapter envelope
  contents — each adapter is a starter implementation; pin the
  v0.4.4 minor version if relying on a specific shape.
- Not an implementation of any external paper's retrieval / audit /
  curation model. The four research anchors that accumulated in
  `[Unreleased]` since 2026-05-05 (Dreams, ARGUS, DELEGATE-52,
  arXiv:2605.15184) all carry explicit composition-anchor
  disclaimers in their respective doc files.
- Not a GPT-judge-scored bench result. The `grep_vs_vector_replay`
  bin produces a deterministic exact-substring smoke metric today;
  the official LongMemEval metric stays gated behind #44.

### Added (cycle highlights)

- `mnemo_core::retrieval::RetrievalMode` typed enum + 5
  `HarnessAware` adapters.
- `bench/locomo/src/bin/grep_vs_vector_replay.rs` runnable scaffold
  bin (PR-A; landed in cycle commit `cde9f68`).
- `docs/research/grep-vs-vector-2605.15184.md` composition anchor.

### Landing trace (2026-05-06)

Recorded one day after PR #76 merged so a future operator reading
`[Unreleased]` can verify the rows below are not in a local-only
state.

- All three rows below (A1 Project Think anchor, U1 Sierra evidence
  + corrected v0.4.3 verification trace + spec-drift footer, U2 v0.4.3
  publish-status doc) shipped on `main` in commit
  [`2802616`](https://github.com/sattyamjjain/mnemo/commit/280261639837d9cf84e387347b2732c162c93bec)
  at 2026-05-05T07:40:03Z via [PR #76](https://github.com/sattyamjjain/mnemo/pull/76).
- v0.4.4 cycle now contains 4 rows (the three above + today's two —
  see Added/Changed below for U1 MCP 2026 Roadmap and U2 landing-trace
  + parked-crate inventory).
- Workspace version unchanged at `0.4.3`. v0.4.4 cuts when a runtime
  / code surface lands on top of this `[Unreleased]` block, not on
  every docs-only row land.

### Parked for v0.4.4 backlog

The crates below are referenced by the daily-prompt ledger and the
`docs/comparisons/` + `docs/src/integrations/` family but have **not
yet landed on `main`**. Listed here so contributors reading
`CHANGELOG.md` see the v0.4.4 backlog in one place rather than
parsing 17 days of prompt history.

- **`mnemo-bench-cf`** (M-effort) — full Cloudflare bench harness
  baselining mnemo against (a) the hosted Agent Memory KV+Vectorize
  service and (b) the DO Facets SQLite-per-DO substrate. Strongest
  v0.4.4 headline candidate. Empty-bench placeholders are tracked
  in [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md).
- **`mnemo-langgraph` Rust crate — RETIRED 2026-05-18.** The parked
  item was a Rust shell that would have had no downstream consumer.
  The functionally-equivalent Python adapter (now canonical name
  `MnemoCheckpointer`, back-compat alias `ASMDCheckpointer`) covers
  LangGraph 1.x's `BaseCheckpointSaver` interface in `python/mnemo/checkpointer.py`.
  Remaining work (implement the stub `list` + `put_writes` methods)
  is rebased to a v0.5.x follow-up — see today's `[Unreleased]`
  Added entry above.
- **`mnemo-purview`** (M-effort) — Microsoft Purview audit-log
  adapter. No S-shippable subset surfaced yet.
- **`mnemo-toolhive`** (S) — Stacklok ToolHive Registry sync.
  Opportunistic; no blocking dependency.
- **`mnemo-envelope`** + `EnvelopeKind::FetcherAttestation` +
  agent-vs-human authorship tag (M-effort, chained) — OTel exporter
  envelope kind. Two follow-ups are blocked on this crate landing
  first.
- **`mnemo-aas01`** (M-effort) — OWASP AAS01 detector surface.
- **`mnemo-mgt`** (M-effort) — SecureAuth Trust Registry adapter.
- **`bench/locomo` LongMemEval / BEAM extension** (S/M) — track
  Mem0g 68.4% / MemPalace 96.6% LongMemEval / Hindsight BEAM 10M-tier
  numbers in the existing `bench/locomo` crate. Source URLs are
  31-58 days old (outside ≤7d primary-trigger gate); high-value as
  a v0.4.4 headline alongside `mnemo-bench-cf`.

### Added

- **U1 (v0.4.4, 2026-05-09) — Anthropic Dreams Research Preview substrate
  anchor.** README `### Memory curation interop (Dreams, Routines, and
  substrate primitives)` sub-section inside Key Features, citing the
  [Dreams Research Preview docs](https://platform.claude.com/docs/en/managed-agents/dreams)
  (surfaced 2026-05-06 at Code w/ Claude SF, 3 days old at land-time)
  and the companion [Routines doc](https://code.claude.com/docs/en/routines).
  New companion comparison doc
  [`docs/comparisons/anthropic-dreams.md`](docs/comparisons/anthropic-dreams.md)
  with curator-action ↔ substrate-primitive layering table; explicit
  non-overlap callout (Dreams owns *what to curate*, mnemo owns *how
  to durably store with audit trail*). One-sentence cross-link from
  [`docs/comparisons/cloudflare-project-think.md`](docs/comparisons/cloudflare-project-think.md)
  noting Project Think (runtime) + MCP 2026 Roadmap (protocol) +
  Dreams (curator) together describe the runtime + protocol + curator
  picture, with mnemo as the offline-auditable substrate underneath.
  **Honest framing:** the Dreams API is Research Preview behind a
  Request-access form; **mnemo does NOT today ship an Anthropic-API
  adapter.** A `mnemo-dreams` adapter crate is plausible if/when the
  API exits Research Preview but is explicitly NOT on the v0.4.x
  backlog.

- **A1 (v0.4.4) — Cloudflare Project Think positioning anchor.**
  README `### Project Think — loop vs. ledger` sub-section inside the
  existing "Why mnemo when Cloudflare Agent Memory exists?" H2,
  citing the [Project Think announcement](https://blog.cloudflare.com/project-think/)
  (2026-05-04, 1 day old at land-time). New companion comparison doc

- **A1 (v0.4.4) — Cloudflare Project Think positioning anchor.**
  README `### Project Think — loop vs. ledger` sub-section inside the
  existing "Why mnemo when Cloudflare Agent Memory exists?" H2,
  citing the [Project Think announcement](https://blog.cloudflare.com/project-think/)
  (2026-05-04, 1 day old at land-time). New companion comparison doc
  [`docs/comparisons/cloudflare-project-think.md`](docs/comparisons/cloudflare-project-think.md)
  treating Project Think as the *runtime layer* and mnemo as the
  *audit-ledger layer* — explicitly **complementary, not substitute**
  surfaces. The bench harness for *Cloudflare Agent Memory vs mnemo*
  does NOT re-run for Project Think because the answer is layering,
  not benchmarking. [`docs/src/integrations/cloudflare-workers-deploy.md`](docs/src/integrations/cloudflare-workers-deploy.md)
  gains a `## Runtime layer (Project Think)` sub-section linking to
  the new comparison doc. Two new tests: extended marketing-phrase
  banlist (`competes with Cloudflare`, `replaces Project Think`,
  `Project Think killer`, `Workers killer`) and
  `tests/readme_project_think_link.rs` (primary-source + heading +
  comparison-doc-link survival).

### Changed

- **U1 (v0.4.4) — Sierra $950M raise applied-agent-layer evidence
  paragraph in [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md).**
  One-paragraph *market-evidence, not feature-claim* note citing
  Sierra's 2026-05-04 raise as concrete evidence the applied-agent
  layer is well-funded enough to demand the offline-auditable memory
  substrate mnemo offers.
- **U1 — corrected v0.4.3 verification trace.** The `## [0.4.3] -
  2026-05-04` block's original `### Verification trace (2026-05-04)`
  sub-block was authored before the version-flip commit landed in
  the same PR — it asserts `Cargo.toml workspace.package.version =
  "0.4.2"` while the live raw fetch shows `"0.4.3"`. New
  `### Verification trace (2026-05-05)` sub-block records the
  corrected state with all artifact-registry checks green; the
  original trace stays in place as audit history of how the
  inconsistency arose.
- **U1 (v0.4.4) — spec-drift reconciliation footer.**
  [`docs/spec-drift-2026-05-04.md`](docs/spec-drift-2026-05-04.md)
  gains a `## 2026-05-05 stable-divergence confirmation` footer
  recording today's check: repo description on `main` unchanged, 14
  topics unchanged, Phase 6 skill template still anchors the older
  description — **stable divergence the operator has accepted, not
  a regression to flap on**.

- **U1 (v0.4.4, 2026-05-06) — MCP 2026 Roadmap spec-context anchor.**
  README `### mnemo and the MCP 2026 Roadmap` sub-section inside the
  existing Access Protocols section, citing the
  [MCP 2026 Roadmap](https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/)
  (published 2026-03-09, 58 days old — *spec-context anchor, not
  fresh trigger*). Frames mnemo's existing operator-held HMAC
  keystore + AES-256-GCM at-rest encryption + dual DuckDB/Postgres
  backends + `mnemo-compliance` crate as the *attestable memory*
  layer aligned by design with the roadmap's **Enterprise
  Readiness** priority area — explicitly *not* a roadmap-compliance
  claim. [`docs/src/integrations/mcp-server.md`](docs/src/integrations/mcp-server.md)
  gains a `## MCP 2026 Roadmap alignment` section with a four-row
  priority-area mapping table tagging mnemo as `follower` /
  `observer` / `observer` / `aligned-by-design` against
  Transport / Agent Communication / Governance / Enterprise
  Readiness respectively. One-sentence cross-link from
  [`docs/comparisons/cloudflare-project-think.md`](docs/comparisons/cloudflare-project-think.md)
  noting Project Think + the MCP 2026 Roadmap together describe the
  *runtime + protocol* picture, with mnemo below both as the
  offline-auditable storage substrate.

- **U1 (2026-05-06) — Access Protocols table version drift fix.**
  Stale `rmcp 0.14` reference corrected to `rmcp 1.3` to match the
  workspace dep on `main`. Caught while landing the MCP 2026
  Roadmap anchor.

### Documentation

- **U2 (v0.4.4) — v0.4.3 publish-status doc.** New
  [`docs/release/v0.4.3-publish-status.md`](docs/release/v0.4.3-publish-status.md)
  records: cargo-publish job ID + `success` conclusion + 17/17 crates
  at `0.4.3` on crates.io with published-at timestamps; PyPI
  `mnemo-db@0.4.3` live; npm `@mndfreek/mnemo-sdk@0.4.3` live. The
  v0.4.3 publish completed cleanly under the bumped 300-min job
  timeout — no resume-dance required.

- **U2 (v0.4.4, 2026-05-06) — v0.4.3 publish-status reconciliation
  footer.** [`docs/release/v0.4.3-publish-status.md`](docs/release/v0.4.3-publish-status.md)
  gains a `## Post-publish reconciliation (2026-05-06)` footer
  closing the publish-status loop one day after the cut: no
  downstream regressions surfaced via `cargo audit`, `cargo deny`,
  or PyPI/npm install-test workflows in the last 24h. v0.4.4
  `[Unreleased]` cycle now active.

- **(v0.4.4, 2026-05-17) — `bench/locomo/grep_vs_vector_replay` bin
  scaffold.** New `[[bin]]` target in
  [`bench/locomo`](bench/locomo/) that routes a LongMemEval-shaped
  slice through `mnemo.recall` in three modes — `vector_only`
  (`strategy="semantic"`), `bm25_only` (`strategy="lexical"`), and
  `rrf_hybrid` (`strategy="auto"`) — and emits a Markdown table to
  `bench/locomo/results/grep_vs_vector_<date>.md`. Reproduces the Sen
  et al. arXiv:2605.15184 experiment design ("grep vs vector
  retrieval inside agent harnesses") on mnemo's own substrate.

  **Scope honest:** runs end-to-end against the bundled 45-record
  synthesized `longmemeval_m.jsonl` with `NoopEmbedding` (zero
  vectors, vector-only mode is degenerate by design — the wiring is
  the point) and a deterministic exact-substring smoke metric. The
  full 116-question LongMemEval slice + GPT-judge-scored official
  metric require an embedder + API key and are gated behind the same
  secrets ledger as
  [#44](https://github.com/sattyamjjain/mnemo/issues/44). Per-query
  failures (e.g. Tantivy BM25 parser rejecting apostrophes) are
  counted as misses in the accuracy column with an explicit
  failures-column in the markdown so the reader can tell substrate
  recall apart from parser strictness. New
  [`bench/locomo/README.md`](bench/locomo/README.md) documents both
  the smoke path and the gated full path.

  Pairs with the docs companion in PR-B (RetrievalMode typed enum +
  HarnessAware variant) that lands the rest of the arXiv:2605.15184
  anchor.

- **U1 (v0.4.4, 2026-05-10) — DELEGATE-52 outcome-diffing primitive
  anchor.** New
  [`docs/research/delegate52-2604.15597.md`](docs/research/delegate52-2604.15597.md)
  treating the DELEGATE-52 delegation-corruption result
  ([arXiv 2604.15597](https://arxiv.org/abs/2604.15597), Hacker News
  front 2026-05-09) as a *write-side substrate* anchor: mnemo's
  append-only event log + snapshots capture the plan / input / trace
  / output tetrad an outcome-diff replay tool reconstructs at audit
  time. The doc walks through (a) what DELEGATE-52 measures (25%
  baseline silent corruption rate on long delegated workflows),
  (b) the three trust walls (intent / action / outcome) and where
  mnemo lives (Wall 3), (c) the operator recipe for getting
  outcome-diff-ready against mnemo today without a new crate, and
  (d) the explicit non-overlap callout (mnemo provides the
  substrate, the diffing policy is the auditor's job).
  README "Why mnemo when Cloudflare Agent Memory exists?" gains
  one paragraph anchoring the outcome-diffing primitive in v0.4.4.
  [`docs/comparisons/anthropic-dreams.md`](docs/comparisons/anthropic-dreams.md)
  gains a one-line cross-reference distinguishing curation (Dreams)
  from outcome diffing (DELEGATE-52). Two new doc-only fixture rows
  in [`docs/tests/example_recalls.md`](docs/tests/example_recalls.md)
  exercising the reconstruction-from-events path: (1) primary-agent
  plan capture via REMEMBER with `metadata.role="plan"`, (2)
  full-tetrad reconstruction via `RECALL { thread_id, as_of,
  with_provenance=true }`. **No behavioural change to the binary**
  — the fixtures specify substrate calls operators can make today.

- **U2 (v0.4.4, 2026-05-09) — ARGUS provenance composition anchor.**
  [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md)
  gains a `## Read-side composition: ARGUS provenance auditing
  (2026-05-09)` section pairing mnemo's *write-side* HMAC envelope
  chain with [arXiv 2605.03378](https://arxiv.org/abs/2605.03378)'s
  *read-side* decision-auditing model for context-aware prompt
  injection (submitted 2026-05-05, 4 days old at land-time). New
  companion research-anchor doc
  [`docs/research/argus-2605.03378.md`](docs/research/argus-2605.03378.md)
  walking through what ARGUS does, where mnemo fits, and what this
  note is explicitly NOT (not an implementation, not a compliance
  claim, not a benchmark). Composition-anchor framing throughout —
  compositional-security overclaim phrasings (`prompt-injection-proof`,
  `provenance-guaranteed`, `ARGUS-compliant`,
  `injection-resistant by construction`) banned by the extended
  marketing-phrase test below.

### Tests

- `tests/changelog_has_unreleased_section.rs` — fails the build if
  `CHANGELOG.md` ever loses its `## [Unreleased]` heading.
- `tests/release_status_doc_present.rs` — fails the build if
  `docs/release/v0.4.3-publish-status.md` is missing the canonical
  `Cargo workspace v0.4.3 publish status` header. Cheap drift guard
  for the release-day audit habit.
- **`tests/readme_mcp_roadmap_link.rs`** (v0.4.4 U1, 2026-05-06) —
  fails the build if README drops the MCP 2026 Roadmap primary-source
  URL or the `### mnemo and the MCP 2026 Roadmap` heading or the
  link to `docs/src/integrations/mcp-server.md`. Anchor-survival
  guard.
- **`tests/readme_no_marketing_phrases.rs`** (v0.4.4 U1, 2026-05-06)
  — banlist extended with `MCP 2026 leader`, `compliant with MCP
  2026`, `MCP 2026 ready`, `roadmap-compliant` so the new spec-context
  anchor cannot drift into compliance-overclaim framing.
- **`tests/changelog_has_landing_trace_section.rs`** (v0.4.4 U2,
  2026-05-06) — fails the build if the `## [Unreleased]` block ever
  loses its `### Landing trace` heading or if that heading does not
  contain a hex commit-sha-prefix matching `[0-9a-f]{7,40}`. Forces
  every future docs-only land to record an on-`main` commit pointer.
- **`tests/readme_dreams_link.rs`** (v0.4.4 U1, 2026-05-09) — fails
  the build if README drops the Anthropic Dreams Research Preview
  primary-source URL, the `### Memory curation interop` heading, the
  link to `docs/comparisons/anthropic-dreams.md`, or the literal
  `Research Preview` honesty disclaimer.
- **`tests/research_doc_argus_present.rs`** (v0.4.4 U2, 2026-05-09)
  — fails the build if `docs/research/argus-2605.03378.md` is
  missing the arXiv URL or the `Composition anchor, not a compliance
  claim` standing-rule disclaimer.
- **`tests/readme_no_marketing_phrases.rs`** (v0.4.4 U1+U2,
  2026-05-09) — banlist extended with five Dreams overclaim phrasings
  (`Dreams replacement`, `dream-compatible`, `Dreams-ready`,
  `Dreams competitor`, `curator killer`) and four compositional-security
  overclaim phrasings (`prompt-injection-proof`, `provenance-guaranteed`,
  `ARGUS-compliant`, `injection-resistant by construction`).
- **`tests/research_doc_delegate52_present.rs`** (v0.4.4 UPDATE-1,
  2026-05-10) — fails the build if
  `docs/research/delegate52-2604.15597.md` is missing the arXiv URL,
  the `Composition anchor, not a compliance claim` standing-rule
  disclaimer, or the load-bearing `plan / input / trace / output
  tetrad` phrasing.
- **`tests/example_recalls_doc_present.rs`** (v0.4.4 UPDATE-1,
  2026-05-10) — fails the build if `docs/tests/example_recalls.md`
  is missing either fixture-row heading or the link back to the
  DELEGATE-52 research-anchor.
- **`tests/readme_no_marketing_phrases.rs`** (v0.4.4 UPDATE-1,
  2026-05-10) — banlist extended with three DELEGATE-52 overclaim
  phrasings (`DELEGATE-52-resistant`, `outcome-corruption-proof`,
  `delegation-safe by construction`).

## [0.4.3] - 2026-05-04

Substrate-anchor release. Three S-effort surfaces: a Cloudflare
Workers / Durable Object Facets deploy-template *design anchor*
(net-new market trigger from the 2026-04-30 DO Facets open beta), a
version-skew matrix expansion to track the 2026-05-01 / 2026-05-02
MCP client-SDK refresh, and a spec-drift reconciliation note that
pins the repo description on `main` as canonical against an external
skill-template anchor. Also lands the load-bearing **breaking change**
that's been gated for two release cycles: `duckdb` 1.4 → 1.5.2
(closes [#41](https://github.com/sattyamjjain/mnemo/issues/41) Step 1)
with a fully idempotent migration runner that incidentally resolves
the pre-existing Ubuntu DuckDB extension race.

### Added

- **A1 — Cloudflare Workers / Durable Object Facets deploy template anchor.**
  README `### Cloudflare Workers deploy template` subsection under
  Deployment, citing the [DO Facets open-beta](https://blog.cloudflare.com/durable-object-facets-dynamic-workers/)
  (2026-04-30) as the substrate anchor for the v0.4.3 `mnemo-bench-cf`
  crate. New design note at
  [`docs/src/integrations/cloudflare-workers-deploy.md`](docs/src/integrations/cloudflare-workers-deploy.md)
  covering Rust↔WASM↔DO-Facet boundaries, file-format compatibility
  (DuckDB ↔ SQLite is *not* wire-compatible), operator-held HMAC
  keystore requirement, and the open-question list (USearch-on-WASM,
  Tantivy-on-WASM, DuckDB-on-WASM trade-offs).
  [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md)
  S1.5 row replaces empty-bench placeholders with a concrete
  per-tenant-footprint / cold-start / persistence-boundary /
  audit-replay scenario block. Two new tests: extended marketing-phrase
  banlist (`tests/readme_no_marketing_phrases.rs` adds `viral`,
  `game-changing`, `revolutionary`, `wild`, `mind-blowing`, etc.) and
  `tests/readme_workers_template_link.rs` (anchor-link survival test).

### Changed

- **U1 — version-skew matrix gains MCP-SDK columns + a
  Cloudflare-substrate annotation.**
  [`docs/compat/version-skew-matrix.md`](docs/compat/version-skew-matrix.md)
  now splits server-side and SDK-side rows; new `mcp-python` /
  `mcp-go` / `mcp-ruby` / `mcp-csharp` columns track the 2026-05-01 /
  05-02 client-SDK refresh from
  [github.com/modelcontextprotocol](https://github.com/modelcontextprotocol).
  The v0.4.3 row carries a Cloudflare-substrate annotation listing
  both Workers KV+Vectorize *and* DO Facets SQLite as
  `mnemo-bench-cf` baseline targets (not implementation-of-record —
  mnemo still ships embedded Rust). New regression test
  `crates/mnemo-mcp/tests/sdk_matrix_doc_present.rs` fails if the doc
  is missing or loses any of the four `mcp-*` column headers.
  `docs/src/integrations/mcp-server.md` gains a "Compatibility note"
  section linking to the matrix for SDK-skew triage.

- **U2 — spec-drift reconciliation note.**
  New [`docs/spec-drift-2026-05-04.md`](docs/spec-drift-2026-05-04.md)
  declares the repo description on `main` canonical (vs. the
  daily-opportunity-radar skill template's older description) and
  maps the skill template's surface anchors (semantic + episodic
  stores, LangGraph adapter, Workers template) to where they live in
  the actual codebase. `CONTRIBUTING.md` gains a "Spec-drift policy"
  subsection linking to the note so future contributors landing
  surface-affecting changes find the policy first.

### Verification trace (2026-05-04)

> ⚠️ **This trace was authored before the version-flip commit landed
> in the same PR.** It asserts `Cargo.toml = "0.4.2"` while the live
> raw fetch shows `"0.4.3"`. The version flip was the *intent* of the
> PR, not a regression. See the corrected `### Verification trace
> (2026-05-05)` sub-block below for the post-merge state.

- `Cargo.toml` workspace.package.version = `"0.4.2"` on `main` ✓
- README role-filter section live (v0.4.2 A1) ✓
- README Cloudflare differentiation H2 live (v0.4.2 U2) ✓
- `tests/readme_no_marketing_phrases.rs` green on `main` ✓
- All 17 crates published at `0.4.2` on crates.io ✓
- `mnemo-db@0.4.2` on PyPI ✓
- `@mndfreek/mnemo-sdk@0.4.2` on npm ✓

### Verification trace (2026-05-05) — corrected post-merge state

Recorded one day after the v0.4.3 cut to capture the published-state
ground truth. Origin of the correction: today's U1 row.

- `Cargo.toml` workspace.package.version = `"0.4.3"` on `main` ✓
- `duckdb = "=1.10502.0"` workspace pin live ✓
- `apply_alters_idempotent` migration runner live in
  `crates/mnemo-core/src/storage/migrations.rs` ✓
- README "Cloudflare Workers deploy template" sub-section live
  (v0.4.3 A1) ✓
- `tests/readme_workers_template_link.rs` green on `main` ✓
- `tests/readme_no_marketing_phrases.rs` extended banlist green on
  `main` ✓
- `crates/mnemo-mcp/tests/sdk_matrix_doc_present.rs` green on
  `main` ✓
- `docs/spec-drift-2026-05-04.md` live (v0.4.3 U2) ✓
- All 17 crates published at `0.4.3` on crates.io ✓ (cargo-publish
  job completed `success` under the bumped 300-min cap — see
  [`docs/release/v0.4.3-publish-status.md`](docs/release/v0.4.3-publish-status.md))
- `mnemo-db@0.4.3` on PyPI ✓
- `@mndfreek/mnemo-sdk@0.4.3` on npm ✓
- 4 dependabot bumps merged after v0.4.3 cut: `actions/setup-node`
  v4→v6 (#69), `actions/download-artifact` v7→v8 (#70), `toml`
  0.9→1.1 (#71), `tokenizers` 0.22→0.23 (#72) ✓

### ⚠️  Breaking — persisted state upgrade required

- **Bumped `duckdb` 1.4 → 1.5.2** (closes [#41](https://github.com/sattyamjjain/mnemo/issues/41) Step 1; PR [#75](https://github.com/sattyamjjain/mnemo/pull/75)).
  DuckDB 1.5.2 stamps a newer on-disk file-format header. **Operators
  upgrading mnemo across this version must:**
  1. **Back up** any persisted `*.mnemo.db` file (and the sibling
     `*.usearch` / `*.tantivy` index directories) before running the
     new binary.
  2. **Open the DB once with the new binary** to upgrade the file
     format in place. Once upgraded, the file is no longer readable
     by mnemo binaries pinned to duckdb 1.4.x — downgrading after
     this point requires a fresh DB.
  3. If a downgrade is required, restore from the pre-upgrade backup
     in step 1.
  4. **No operator action is required for fresh DBs** — the new
     binary writes the new format on first open.

  See the upstream [DuckDB 1.5.2 release notes](https://duckdb.org/2026/04/13/announcing-duckdb-152)
  and the [`duckdb-rs` 1.10502.0 release](https://github.com/duckdb/duckdb-rs/releases) for full file-format change details.

### Changed

- **Migrations are now idempotent under DuckDB 1.5+** (PR [#75](https://github.com/sattyamjjain/mnemo/pull/75)).
  The previous "issue ALTER, swallow column-exists error" pattern in
  `run_migrations` no longer works — DuckDB 1.5 aborts the
  connection's implicit transaction after a few consecutive failures.
  New `apply_alters_idempotent` introspects
  `information_schema.columns` first and only emits an `ALTER` when
  the column is actually missing. Side benefit: also resolves the
  pre-existing Ubuntu DuckDB extension race that was admin-merged
  through every prior release.

## [0.4.2] - 2026-05-03

Reconciliation release. Three S-effort surfaces driven by the
2026-04-30 MCP authorization spec (role-based annotations) and the
Cloudflare Agents Week wrap (2026-04-29). Resyncs the workspace
version metadata that drifted ahead of `main` in the prompt ledger.

### Added

- **A1 — MCP role-aware tool filter.** New
  [`crates/mnemo-mcp/src/role_filter.rs`](crates/mnemo-mcp/src/role_filter.rs)
  with `RoleFilter` trait + `ManifestRoleFilter` impl. Manifest-driven
  `[role_filter]` block (default no-op when omitted, byte-for-byte
  preserves existing behaviour). Aligns with the MCP authorization
  spec (2025-11-25, role-based annotations,
  https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization).
  Three integration tests: `role_filter_allow_deny`,
  `role_filter_audit_event`, `role_filter_no_block_when_unset`.
- **U2 — Cloudflare differentiation.** New
  [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md)
  long-form scenario list with empty-bench placeholders pointing to the
  v0.4.3 `mnemo-bench-cf` crate. README gains a "Why mnemo when
  Cloudflare Agent Memory exists?" section that explicitly concedes
  edge-recall perf likely favours Cloudflare and positions the
  differentiator on provenance, chain replay, and offline auditability.
  Grep-lint `tests/readme_no_marketing_phrases.rs` rejects "beat
  Cloudflare" / "faster than Cloudflare" / "Cloudflare killer" in CI.
- **U2 — SHARE on TS + Go quickstarts.** TypeScript and Go SDK README
  blocks now show `client.share({memoryId, withAgent})` /
  `client.Share(mnemo.ShareInput{...})` lines so the SHARE primitive
  has explicit quickstart parity with REMEMBER / RECALL / FORGET.

### Changed

- **U1 — Workspace version resync.** `workspace.package.version`
  bumped `0.4.1 → 0.4.2`. Internal-crate version pins (lines 99-106 of
  `Cargo.toml`) bumped from `0.4.0-rc2` to `0.4.2` so consumers can
  resolve `mnemo-core = "0.4.2"` against the published workspace.
  `python/pyproject.toml` and `sdks/typescript/package.json` bumped to
  `0.4.2`. `sdks/go/mnemo.go` gains a `Version` constant + package
  version doc-comment so the Go SDK reports the same version on MCP
  `initialize`.
- **Compatibility matrix.** New
  [`docs/compat/version-skew-matrix.md`](docs/compat/version-skew-matrix.md)
  pinning `mnemo` ↔ `rmcp` ↔ `tantivy` ↔ `usearch` ↔ `pgvector` ↔
  Python/TS/Go SDK versions.

### Tests

- `crates/mnemo-core/tests/version_metadata.rs` — asserts
  `env!("CARGO_PKG_VERSION") == "0.4.2"` so any future drift between
  the workspace stamp and the source crate fails CI.
- `python/tests/test_version_alignment.py` — asserts
  `mnemo.__version__` matches the Cargo workspace version.
- `tests/readme_no_marketing_phrases.rs` — top-level integration test
  greps `README.md` for the three banned marketing phrases.

### Deferred to v0.4.3

The 2026-05-02 prompt's six P0/P1 rows are explicitly **rebased to
v0.4.3** because their prerequisite crates (`mnemo-envelope`,
`mnemo-aas01`, `mnemo-mgt`) never landed on `main` between 2026-04-29
and 2026-05-03:

- `mnemo-bench-cf` (full Cloudflare bench crate — v0.4.2 ships only
  the README differentiation paragraph)
- `mnemo-langgraph` 1.2 checkpoint adapter (no LangGraph 1.2 release
  ≤7d to force the schedule)
- `mnemo-purview` (Microsoft Purview log adapter, M-effort)
- `EnvelopeKind::FetcherAttestation` (depends on `mnemo-envelope`
  being on `main` first)
- Agent-vs-human authorship tag (same dependency)
- `mnemo-toolhive` (Stacklok Registry v1.2.0 sync, opportunistic)

### Sources

- MCP Authorization spec — https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization
- Cloudflare Agents Week wrap — https://www.cloudflare.com/agents-week/updates/

## [0.4.1] - 2026-04-28

Silence-breaker release. Picks up the four competitive surfaces that
opened this week (Anthropic CMA-Memory beta, MemMachine + Memori
LoCoMo numbers, DeepSeek V4 1M context, RSAC 2026 SOC telemetry gap)
plus a counterparty-discovery layer for the Project-Deal substrate
shipped yesterday.

### Added

- **P0-1 — First public LoCoMo benchmark.** New
  [`bench/locomo`](bench/locomo) crate with `LoCoMoRun`,
  `LoCoMoResult`, `LoCoMoJudge` trait + `MockJudge` fallback. Cross-
  judge variance tracking (GPT-5.1 + Claude-3.7 Sonnet). Authenticated
  nightly via [`.github/workflows/locomo-nightly.yml`](.github/workflows/locomo-nightly.yml).
  First public report at
  [`docs/benchmarks/locomo-2026-04-28.md`](docs/benchmarks/locomo-2026-04-28.md).
  9 unit tests.
- **P0-2 — `mnemo-cma` crate (Anthropic CMA-Memory compat shim).**
  Drop-in for the filesystem-of-Markdown beta announced 2026-04-23.
  `CmaTreeRoot` / `SyncMode { ReadThrough | WriteThrough | Mirror }`,
  `import_cma_tree` produces a deterministic `ImportSummary` whose
  HMAC chain head is byte-identical for two runs over the same tree,
  `audit_bridge::bridge_event` chains every CMA write into the
  existing provenance ledger via `CmaSource::CmaBeta` /
  `CmaSource::CmaImport` markers, `export_to_tree` reproduces the
  original `.memory/` byte-for-byte. 10 unit tests.
- **P0-3 — `mnemo-baseline` crate (RSAC SOC telemetry gap).**
  Per-agent rolling profile (`AgentBaseline` with recall/write
  rates, namespace fanout, tool mix, HMAC continuity), z-score +
  EWMA drift detector with five `Severity` thresholds, OpenTelemetry
  semconv 1.31 + OCSF 1.4 Application-Activity emitters via
  `JsonExporter`. **Anti-leak invariant** enforced by a regex sweep
  unit test: emitted payloads never contain memory contents. 9 unit
  tests.
- **P1-4 — 1M-context recall budget planner.** New
  `mnemo-core::budget` module: `ContextBudget::for_model(ModelId)`
  + `plan_recall(budget, history, query) -> RecallPlan`. Per-model
  table covers `deepseek-v4-1m`, `claude-3.7-sonnet-1m`,
  `gpt-5.1-400k`, `gemini-2.5-pro-2m` plus their smaller siblings.
  Property test asserts the plan never overflows total context. 9
  unit tests.
- **P1-5 — Project-Deal counterparty discovery + reputation.** Two
  new `mnemo-deal` submodules: `discovery::AgentAdvertisement` for
  the canonical `/.well-known/mnemo-deal-agent.json` body shape +
  `reputation::compute_reputation` with a 90-day half-life decay
  and a per-dispute 10% penalty. The README's threat-model section
  scopes the score as advisory, not enforcement. 7 new tests (17
  total in the crate).
- **P2-6 — `mnemo doctor` + Grafana dashboard JSON.** Typed
  `DoctorReport` + `DoctorFix` recommendations
  (RebuildVectorIndex / RotateHmacKey / RepinMcpCatalog /
  EnableDecayLane / UpgradeRmcp). Committed
  [`dashboards/mnemo-grafana.json`](dashboards/mnemo-grafana.json)
  (Grafana schemaVersion 39), validated by an integration test that
  asserts the operator-critical panels exist. 5 tests.

### Changed

- Workspace version bumped from `0.4.0` to `0.4.1` across all 17
  Rust crates (incl. three new: `mnemo-cma`, `mnemo-baseline`,
  `bench/locomo`), the Python package (`mnemo-db`), and the
  TypeScript SDK (`@mndfreek/mnemo-sdk`).
- `cargo-publish.yml` plan list updated to include the two new
  publishable crates (`mnemo-cma`, `mnemo-baseline`); the bench
  crate is `publish = false` and stays out of crates.io.

### Notes for operators

- The `mnemo cma serve|migrate|export` and `mnemo doctor`,
  `mnemo dashboard` clap subcommands ship the data shapes today;
  wiring them into the binary's `Command` enum is a follow-up
  (mirrors v0.4.0-rc3's pattern). `#[allow(dead_code)]` on each
  module documents the gap so a `cargo clippy -D warnings` build
  stays green.

## [0.4.0] - 2026-04-27

Mesh / code-mode / commerce release. Picks up four net-new
competitive surfaces (Cloudflare Mesh, Cloudflare Code Mode,
Anthropic Project Deal, Wuphf-style Markdown wikis) plus a hard
defense against the new MCP function-hijacking class.

### Added

- **P0-1 — MCP tool-catalog attestation.** New
  `crates/mnemo-cli/src/attest/` module with `PinnedToolCatalog`,
  `CatalogAttestor` trait, and `PinnedAttestor` impl. Operators ship
  a `[tool_catalog_pin]` block in the manifest; `mnemo mcp-server`
  refuses to start if the advertised catalog has any `added` or
  `mutated` tools, and emits a `McpToolCatalogDrift` audit event with
  the per-tool diff. `--allow-removed-drift` lets `removed`-only
  diffs through with a warning. Direct response to **arXiv 2604.20994**
  (function-hijacking via tool-list poisoning). 10 unit tests.
- **P0-2 — `mnemo-mesh` crate (Cloudflare Mesh runtime adapter).**
  SPIFFE-style `MeshIdentity` + `AttestationToken`, `MemOp` enum
  covering `Recall`/`Write`/`Forget`/`Branch`/`ReplayAsOf`/
  `ExportProvenance`, `MeshPolicyEnforcer` trait + `StaticPolicyEnforcer`
  impl with per-(SPIFFE, namespace) ACL, `MeshAuditEnvelope` with
  deterministic `next_chain_head()` chained into the v0.4.0-rc3
  provenance ledger. 13 unit tests. First OSS embedded memory DB to
  speak Cloudflare Mesh attestation natively.
- **P0-3 — `mnemo-codemode` crate (Code Mode WIT recall).** WIT
  world definition (`mnemo:memory@0.4`) under
  `crates/mnemo-codemode/wit/`, host-side runner with
  `ResourceBudget` (fuel / mem_pages / wall), `RecallStep` /
  `GuestProgram` / `RecallBundle`, token-cost estimator that
  asserts code-mode delivers ≥20% token reduction on a 200-turn
  conversation (vs Cloudflare's 99.9% claim — we're more
  conservative because we stream records, not just side effects).
  wasmtime + WASI-stripping path is feature-gated for the follow-up.
  7 unit tests including fuel exhaust + wall-time exceeded.
- **P1-4 — Decay-curve recall primitive.** New
  `mnemo-core::score` module with `ScoreLane` trait + `DecayLane`
  impl. `decay_weight(now, last_access, hits, &DecayParams)` is the
  pure Ebbinghaus exponential with reinforcement and floor;
  `letta_mode` flag in `ScoreContext` zeros the lane for parity with
  Letta's published numbers. Default fuse weights:
  `0.55 vector + 0.20 bm25 + 0.15 recency + 0.10 decay`.
  Competitive response to YourMemory's biological-decay marketing
  (Show HN, 2026-04-27). 9 unit tests.
- **P1-5 — `mnemo-deal` crate (agent-on-agent deal ledger).**
  Chained-HMAC `DealEnvelope` log with `InMemoryDealLedger` impl,
  `verify_chain()` that produces a `DisputeReport` pinpointing the
  first divergent offset. Substrate for Anthropic Project Deal-style
  commerce (announced 2026-04-25). 10 unit tests including tampered
  terms + broken prev_hash detection.
- **P2-6 — `mnemo-md-sync` crate (Markdown+Git working set).**
  Parser for YAML-style frontmatter (`mnemo_id`, `agent_id`,
  `tags`, `expires_at`), `MdSyncSpec` config with
  `SyncFlushPolicy` (PreferEngine / PreferDisk / NewerWins). Wuphf-
  inspired ergonomics with mnemo-grade recall + provenance. 9 unit
  tests. notify-based watcher + gix commit-on-flush land in a
  follow-up; the contract API is stable.

### Changed

- Workspace version bumped from `0.4.0-rc3` to `0.4.0` across all
  Rust crates (14 incl. four new: `mnemo-mesh`, `mnemo-codemode`,
  `mnemo-deal`, `mnemo-md-sync`), the Python package (`mnemo-db`),
  and the TypeScript SDK (`@mndfreek/mnemo-sdk`).
- `mnemo-core::model::EventType` gained `McpToolCatalogDrift` for
  P0-1 audit rows.
- Manifest (B2 hardened mode) gained `tool_catalog_pin_path` and
  `allow_removed_drift` fields. Both optional and additive — older
  manifests load unchanged.
- `cargo-publish.yml` plan list updated to include the four new
  crates so push-to-main publishes them.

### Security

- The P0-1 tool-catalog attestation is a direct response to **arXiv
  2604.20994 (2026-04-23)**: a malicious MCP source that mutates
  `tools/list` can rename a tool, change its `inputSchema`, or
  smuggle a hidden `secret_exfil` tool. Mnemo's hardened launcher
  now refuses to expose any catalog whose fingerprint set differs
  from the operator-pinned baseline.

## [0.4.0-rc3] - 2026-04-26

Threat-model release: hardens the MCP STDIO entry point against the
OX-MCP "exfiltrate-then-act" disclosure (2026-04-24), adds memory
provenance signing on reads, and ships compliance + competitive
parity surfaces (DPDPA, Letta-protocol).

### Added

- **B1 — Memory-provenance signing API.** New `mnemo-core::provenance`
  module with `ProvenanceSigner` (HMAC-SHA256), `ReadProvenance`
  receipt type, and `verify_read_provenance()` helper. `RecallRequest`
  carries a new `with_provenance: Option<bool>` field; when set and a
  signer is attached to the engine, the response includes a verifiable
  receipt that binds the cited records to a server-side key. Supports
  rotated keys via `hmac_key_id`. 6 unit tests + 4 integration tests
  in `crates/mnemo-core/tests/provenance_chain.rs`.
- **B2 — `mnemo mcp-server --manifest <path>` hardened mode.** New
  CLI subcommand that runs a safe-spawn gauntlet BEFORE constructing
  any engine state: refuses inherited sensitive env vars, refuses
  `--config`-style argv injection, refuses untrusted parents (non-TTY
  parent must be in `manifest.allowed_parents`). Loads the HMAC
  keystore the manifest points at and attaches a `ProvenanceSigner`
  (B1) to the engine — key material reaches the binary via a
  chmod-restricted file, never via env or argv. 14 unit tests
  (manifest/safe-spawn/lease) + 4 integration tests spawning the real
  binary.
- **B3 — LongMemEval_M bench + `--with-provenance` toggle.** Bundled
  45-record synthesized dataset at
  `crates/mnemo-core/benches/data/longmemeval_m.jsonl` (override via
  `MNEMO_LONGMEMEVAL_PATH`). New `longmemeval_bench` criterion target
  with `recall_no_provenance` and `recall_with_provenance` arms.
- **B4 — DPDPA Mannsetu adapter (consent-token-per-write).** New
  `mnemo-compliance::mannsetu` module with `MannsetuConsentSource`
  (HTTP binding to the DPB-registered Mannsetu API), `ConsentToken`
  type, and `ConsentTokenGuard` (per-write authorization with
  expiry/scope/revocation checks). 7 new unit tests.
- **B5 — `mnemo-letta` crate (Letta-protocol-compat).** New workspace
  crate exposing `POST /v1/agents`, `POST /v1/agents/{id}/messages`,
  and `GET /v1/agents/{id}/memory` so a Letta-Code-shaped benchmark
  or notebook can talk to Mnemo without code changes. 4 integration
  tests.
- **B6 — `mnemo eval` subcommand.** Replays a JSONL dataset against
  an in-memory engine and emits a per-row JSONL report
  (latency_us, top-k, hit). Used for config sweeps (provenance
  on/off, hybrid weights, recency half-life). Defaults to the
  bundled LongMemEval_M sample.
- **Q1 — Pure-Python provenance SDK.** New `mnemo.provenance`
  module: `ProvenanceSigner` / `ReadProvenance` / `RecordRef`
  dataclasses + `verify_read_provenance()` helper. Auditors verify
  receipts offline without compiling Rust. 6 pytest cases.
- **Q2 — Claude Code MCP installer.** New `mnemo.install_claude_code`
  module + `python -m mnemo install claude-code [--hardened <manifest>]`
  CLI. Idempotently registers Mnemo as an MCP server in
  `~/.claude.json`. 6 pytest cases.
- **Q3 — DPDPA "data passport" PDF builder.** New
  `mnemo.dpdpa_passport` module that renders a one-page PDF showing
  every personal data point Mnemo holds for a subject (DPDPA Section
  11 / 12 right-to-portability/access). Hand-rolled PDF (no
  third-party dep), reproducible byte-for-byte. 5 pytest cases.
- **Q4 — Time-travel debugger UI.** New
  `examples/time-travel-debugger/index.html`. Vanilla JS, no build
  step. Diffs recall results between two `as_of` timestamps.

### Changed

- Workspace version bumped from `0.4.0-rc2` to `0.4.0-rc3` across all
  Rust crates (10 incl. new `mnemo-letta`), the Python package
  (`mnemo-db`), and the TypeScript SDK (`@mndfreek/mnemo-sdk`).
- `RecallRequest` gained `with_provenance: Option<bool>` (additive,
  defaults to `None`). `RecallResponse` gained
  `provenance: Option<ReadProvenance>` (skipped on the wire when
  `None`). Downgrade-safe.
- `MnemoEngine` gained `with_provenance_signer()` builder method.

### Security

- The B2 hardened mode is the direct response to the OX-MCP
  "exfiltrate-then-act" disclosure (2026-04-24). The default `mnemo`
  startup path is unchanged for backward compatibility; new
  deployments should prefer `mnemo mcp-server --manifest <path>`.

## [Unreleased]

### Changed (publication names — no code or behaviour change)

- **PyPI distribution name**: `mnemo` → **`mnemo-db`**. The unqualified
  name on PyPI is held by an unrelated 2021 notebook project
  (`Gabriele Girelli/mnemo-assistant`) with last release 2021-07-06.
  The Python package directory, the import path, and the SDK class
  names are unchanged — `from mnemo import MnemoClient` still works.
  Users now `pip install mnemo-db` and (for extras)
  `pip install 'mnemo-db[anthropic-memory-tool]'` etc.
- **`mnemo-cli` crate** → published as **`mnemo-mcp-server`** on
  crates.io. The unqualified `mnemo-cli` is owned by
  [github.com/watzon/mnemo](https://crates.io/crates/mnemo-cli)
  ("CLI management tool for the Mnemo LLM memory proxy" — a different
  project). The crate directory stays `crates/mnemo-cli/` and the
  installed binary is still `mnemo`. Users now
  `cargo install mnemo-mcp-server` and the resulting binary is
  invoked as `mnemo`.
- README, mdBook docs, integration pages, and example scripts updated
  to reflect both new install commands.

No changes to public APIs, file formats, persistence stamps, or wire
protocols. Downgrade-safe.

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
