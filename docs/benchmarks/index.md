# Benchmarks — single entry point

Every headline number mnemo publishes, in one table, with the exact command that
regenerates it and the raw file it was read from. **Nothing here is re-run or
restated** — each row is transcribed from the result file it links, so the raw
file is always the source of truth. If a number in the README and a number here
ever diverge, the raw file wins and the divergence is a bug to file, not to
paper over.

## How to read this table

- **The axis matters.** Almost every mnemo number is a **retrieval / audit**
  metric — did the memory layer *surface the right evidence* or *prove what it
  did*. It is **not** end-to-end, LLM-judged QA accuracy (the Mem0 / Zep / Letta
  leaderboard axis), which needs a generative LLM + a judge and is **not run**
  in any harness here. The last column says so per-row; read it before quoting a
  number.
- **Small slices, wide intervals.** The bundled fixtures are deliberately small
  (credibility / wiring checks, not leaderboard runs). Wilson-95 intervals are
  reported in the raw files; treat sub-0.05 gaps as ties.
- **Two lanes of reproducibility.** Rows marked *CI* run in continuous
  integration on every push. Rows marked *local* need a model on disk
  (`--features onnx`, a downloaded ONNX model) or a local Ollama and are not
  gated in CI yet — see [#125](https://github.com/sattyamjjain/mnemo/issues/125).

## The table

| Benchmark | Headline number | Reproduce | Raw results | What it does **not** show |
|---|---|---|---|---|
| **Semantic recall** — real embedder (nomic-embed-text, 768-dim, local Ollama), LongMemEval_M held-out slice (n=23, 5 seeds) | `vector_only` **recall@1 0.739**, recall@5 0.826, recall@10 1.00 (MRR 0.806) | `ollama pull nomic-embed-text && cargo run --release -p mnemo-locomo-bench --bin semantic_recall_bench` | [`bench/locomo/results/semantic_recall_2026-08-03.md`](../../bench/locomo/results/semantic_recall_2026-08-03.md) · [`baseline.json`](baseline.json) | End-to-end LLM-judged QA accuracy (no LLM runs); it is retrieval quality + token efficiency only. Small paraphrase-heavy slice, **not** LongMemEval_S scale. Default `auto` fusion *under*performs pure vector here (0.426) — reported as-is. |
| **ONNX MiniLM recall** — different embedder (all-MiniLM-L6-v2, 384-dim), n=45 | **recall@1 0.689** [Wilson 95% 0.543, 0.805], recall@10 0.911, MRR 0.770 | `MNEMO_ONNX_MODEL_PATH=… cargo run --release --features onnx -p mnemo-locomo-bench --bin locomo_v1_bench` | [`docs/benchmarks/locomo-v1.md`](locomo-v1.md) · [`bench/results/locomo_v1.json`](../../bench/results/locomo_v1.json) | **Not** the 0.739 headline — different embedder *and* slice. Not CI-reproducible: needs the MiniLM ONNX model on disk. pgvector path not exercised. |
| **LoCoMo claimed-vs-observed** — single-hop retrieval, hash embedder (byte-stable) | **recall@1 24.4%** [Wilson 95% 14.2%, 38.7%], recall@5 46.7% | `cargo run --release -p mnemo-locomo-bench --bin reproduction_bench` | [`bench/locomo/results/reproduction_2026-07-06.md`](../../bench/locomo/results/reproduction_2026-07-06.md) | A *retrieval* metric on a small slice with a lexical offline embedder — deliberately modest, **not comparable** to vendors' LLM-judged full-dataset QA claims. Not a ranking. |
| **BEAM-style retrieval** — default `auto` hybrid, offline hashed embedder, synthetic fixture | `open_domain` **68.6%** [64.4%, 72.5%]; `multi_hop` **0.6%** [0.2%, 1.7%] | `cargo run --release -p mnemo-locomo-bench --bin beam_bench` | [`bench/locomo/results/beam_2026-07-04.md`](../../bench/locomo/results/beam_2026-07-04.md) | Synthetic fixture, no LLM judge — **not comparable** to Hindsight BEAM's 64.1% (10M-token, LLM-graded). Low `multi_hop` is honest: default RRF is not the multi-hop tool (use `graph`/`reconstruct`). |
| **Implicit-association** — indirect queries + orientation cache, real embedder, 30-row corpus | `indirect` recall@5 **~0.87**, `indirect+orientation` combined **1.00** | `ollama pull nomic-embed-text && cargo run --release -p mnemo-locomo-bench --bin implicit_association` | [`bench/locomo/results/implicit_association_2026-07-30.md`](../../bench/locomo/results/implicit_association_2026-07-30.md) · [`write-up`](implicit-association.md) | Scores *retrieval surfacing*, not an LLM's answers — **not comparable** to InMind's 84.0% / 14.4%. 30 rows ⇒ wide CIs. Not a leaderboard claim. |
| **Reconstruct A/B** — graph-linked multi-hop recovery | gold-coverage@5 **0.083 → 0.208** | `cargo run --release -p mnemo-locomo-bench --bin reconstruct_ab` | [`bench/locomo/results/reconstruct_ab_2026-06-21.md`](../../bench/locomo/results/reconstruct_ab_2026-06-21.md) | An adversarially multi-hop, by-construction fixture — **not** a claim that flat retrieval is wrong. Run it on your own corpus before concluding. |
| **ASI06 auditable resistance** — SHA-256 hash-chain + read-provenance HMAC vs 3 poisoning families | **100.0%** rejection (1500/1500) [Wilson 95% 99.7%, 100.0%], **0%** benign FP (0/300) | `cargo run --release -p mnemo-asi06-poisoning-bench --bin asi06_poisoning` | [`docs/benchmarks/asi06-poisoning.md`](asi06-poisoning.md) · [`bench/results/asi06_poisoning.json`](../../bench/results/asi06_poisoning.json) | **Tamper-evidence + attribution, NOT write-time prevention** — a validly-written poison is not blocked, only made un-hideable. Does not judge whether a memory is *true*. |
| **MINJA poisoning** — real ONNX MiniLM embedder, quarantine lanes | lexical/self-ref lane **ASR 100% → 0%**, 0/300 benign FP; embedding z-score lane **does not generalize** (poison ~1.5σ < 3σ gate) | `cargo run --release --features onnx -p mnemo-poisoning-bench --bin poisoning_real_bench` | [`docs/BENCH_POISONING.md`](../BENCH_POISONING.md) · [`bench/results/poisoning_real.json`](../../bench/results/poisoning_real.json) | Not CI-reproducible (needs `--features onnx` + model). The z-score lane's failure on a dense embedder is published, not hidden — it corrects the hash-embedder bench's rosier reading. |
| **Forged-reasoning injection** — fabricated chain-of-thought presented as reasoned truth | **ASR 100% → 0%** (defense off→on), 0/180 benign false-quarantine | `cargo run --release -p mnemo-forged-reasoning-bench --bin forged_reasoning` | [`bench/results/forged_reasoning.json`](../../bench/results/forged_reasoning.json) | Defense is **opt-in** (`RecallRequest.reasoning_trust` must be set); it excludes injected/unverified reasoning by provenance — it is not truth-checking of the reasoning's content. |
| **Poisoning defense-delta** — hash embedder, byte-stable (the CI-gated companion) | byte-stable defense-delta artifact | `cargo run --release -p mnemo-poisoning-bench --bin poisoning_bench` | [`bench/poisoning/results/poisoning_2026-07-07.md`](../../bench/poisoning/results/poisoning_2026-07-07.md) | Hash embedder is a lexical floor, **not** a real-embedder ASR (see the MINJA row for the real-embedder result). |
| **Phase-3 exploitation, non-adaptive** — already-shortened records, pinned real embedder, #37 | poison **0.956** (129/135) [0.906, 0.980]; **benign floor 0.956 — identical**, so poisoning delta **+0.0000**; detector quarantines **0/135**, defense delta **+0.0000**; poison mean z **1.08** vs 3.0 gate | `MNEMO_ONNX_MODEL_PATH=./model.onnx cargo run --release --features onnx -p mnemo-minja-phase3-bench --bin minja_phase3_bench` | [`2026-08-25-minja-phase3-nonadaptive.md`](2026-08-25-minja-phase3-nonadaptive.md) · [`bench/results/minja_phase3.json`](../../bench/results/minja_phase3.json) | **NOT a MINJA number** — Phases 1-2 (generative, adaptive) are not implemented, and ADR 0003 says removing the shortening step is what makes a result stop being MINJA. Three measured **nulls**: the z-score lane fires on nothing, and the attack rate is fully explained by topical retrieval (the matched benign twin scores the same). Says nothing about an adaptive attacker. n=45 distinct queries is below the repo's own n>=100 bar; quote the conservative [0.852, 0.988]. |
| **Salami compositional poisoning** — N individually-benign, collectively-harmful writes (arXiv:2608.01637), #37 | **save 100%** [Wilson 95% 99.5, 100]; **assembly 100%** [98.1, 100]; benign control **0%** [0, 1.9]; aggregate crosses on the **4th** benign write (n=200/arm) | `cargo run --release -p mnemo-salami-poisoning-bench -- --trials 200` | [`salami-compositional-poisoning.md`](salami-compositional-poisoning.md) · [`results/salami_2026-08-12.md`](../../bench/salami_poisoning/results/salami_2026-08-12.md) · [`bench/results/salami_poisoning.json`](../../bench/results/salami_poisoning.json) | **Measures a gap, does not defend one** — no per-write control rejects a benign slice; retrieval reassembles the whole. Lexical/offline embedder (semantic is future); harm is a structural oracle, not an LLM. Compositional subset of #37 only. |
| **Audit-log tamper-evidence** — EU AI Act Art.12, `verify_event_chain` | **100%** on delete / reorder / hash-forge (200 trials each); **0%** on 2 disclosed gaps | `cargo run --release -p mnemo-audit-tamper-bench` | [`bench/audit_tamper/results/audit_tamper.md`](../../bench/audit_tamper/results/audit_tamper.md) · [`write-up`](audit-log-tamper-evidence.md) | Payload-only forge (content_hash intact) and tail-truncation are **NOT caught** — disclosed, not hidden. Detection, not prevention. Not itself a conformity assessment. |
| **Audit-conformance** — single-byte-mutation detection | **100%** detection (256 trials) [Wilson 95% ≥ 98.5%] | `cargo run --release -p mnemo-audit-conformance-bench` | [`bench/audit_conformance/results/conformance.md`](../../bench/audit_conformance/results/conformance.md) | Detection, not prevention. A single-byte-mutation model of tampering — the tamper-evidence row above covers structural attacks. |
| **Retention conformance** — India DPDP Rules 2025 365-day floor, byte-stable | all gates **PASS** (append-only log survives every deletion/compaction/cold-tier path) | `cargo run --release -p mnemo-retention-conformance-bench` | [`bench/retention_conformance/results/retention_conformance.md`](../../bench/retention_conformance/results/retention_conformance.md) | A deterministic conformance proof on shipped primitives — **not** a legal conformity assessment and not legal advice. |
| **Embedding-backend selection** — SLA-aware recommender, 50-doc/10-query fixture | per-backend nDCG@10, recall@10, p50/p95 embed latency; recommender picks highest-nDCG backend under the SLO | `mnemo bench embeddings --slo-ms <N>` *(or* `cargo bench -p mnemo-embeddings-bench`*)* | `target/criterion/embed_single/` · [`bench/embeddings/README.md`](../../bench/embeddings/README.md) | Not a faithful arXiv:2606.09900 reproduction; `hashing-baseline` is a bench-local lexical sanity floor, not a production backend. Numbers depend on which backends you have configured. |
| **DuckLake vs embedded DuckDB** — storage-engine evaluation for [#41](https://github.com/sattyamjjain/mnemo/issues/41) Step 2 (n=1M, Apple M4, local FS) | **0/5 queries** meet the issue's own ≥20% p95 gate; DuckLake is **2.2×–10.7× slower** on every read shape. Wins only on bulk write (**+23.8%**) | `pip install duckdb && python3 scripts/bench/ducklake_eval.py` | [`2026-08-15-ducklake-evaluation.md`](2026-08-15-ducklake-evaluation.md) | Raw SQL against both engines, **not** through mnemo's `StorageBackend` (the ratio is the figure, not the absolute). Single-node, local-filesystem, single-writer only — says nothing about DuckLake on S3/GCS with concurrent writers, which is what it is built for. No time-travel/snapshot-isolation test. |

## Known caveat — semantic-recall MRR across refreshes

The README quotes `vector_only` **MRR 0.805** from the 2026-06-22 run
([`bench/RESULTS.md`](../../bench/RESULTS.md), which links
[`semantic_recall_2026-06-22.md`](../../bench/locomo/results/semantic_recall_2026-06-22.md)).
The newest run
([`semantic_recall_2026-08-03.md`](../../bench/locomo/results/semantic_recall_2026-08-03.md),
summarized in [`baseline.json`](baseline.json)) reports **MRR 0.806** for the
same mode. `recall@1` (0.739) and `recall@5` (0.826) are identical across both
runs; only MRR moved by 0.001, well inside the bench's own "treat sub-0.05 gaps
as ties" threshold. This table cites the newest run. The delta is recorded here
rather than reconciled by editing one number to match the other.

## Also measured (in-tree, no standalone result file)

These live as `#[test]` evals inside the crates, not as `bench/` result files,
so they have no raw artifact to link — run the test to see the table:

| Eval | Headline | Reproduce |
|---|---|---|
| Budgeted evidence retention (LongMemEval-style, 8192-token budget) | budgeted capsules vs naive truncation — see the test's printed table | `cargo test -p mnemo-core --test budgeted_evidence_retention -- --nocapture` |
| Domain-scoped dilution (50 → 1,000 docs) | scoping keeps precision flat as the off-domain corpus grows | `cargo test -p mnemo-core --test domain_scoped_dilution -- --nocapture` |

## Framing, not parity

Every external benchmark named here (Engram, BEAM/Hindsight, InMind, LoCoMo,
Mem0/Zep/Letta) is a **reference point for the framing**, never a parity claim.
Vendor-published scores are cited, not re-run in mnemo's harness; only mnemo's
own rows are reproducible from this repo. No "first" / "best" claim is made.
