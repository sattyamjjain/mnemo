# mnemo vs the agent-memory field: the compliance-audit axis

> **Not legal advice; not a leaderboard claim.** Every number on this page was
> produced by a benchmark that ships in this repository (Apache-2.0), is
> reproducible offline, and carries its own honesty caveats in the linked
> report. Competitor figures are each vendors' **own published** claims, cited
> and **not re-run** in mnemo's harness. Vendor features change — the comparison
> reflects public documentation as of **2026-07**; verify against current docs
> before relying on it.

## The thesis

The agent-memory field is racing on one axis: **recall/answer quality** on
LoCoMo / LongMemEval-style leaderboards. mnemo does **not** claim to lead that
race (see [Where mnemo does not lead](#where-mnemo-does-not-lead)). mnemo leads a
*different* axis that funded recall-quality teams have largely skipped:

**on-prem, MCP-native, cryptographically-auditable memory a regulator or auditor
can verify offline, without trusting the store or any hosted tier.**

That axis is not marketing — it is four already-shipped, reproducible benchmarks:

| what | shipped number | reproduce |
|---|---|---|
| **Recall quality** (mnemo's own, honest) | LongMemEval_M, real embedder: **semantic recall@1 0.739, MRR 0.805, ~89% fewer context tokens** vs full history | `cargo run --release -p mnemo-locomo-bench --bin semantic_recall_bench` |
| **Tamper-evident audit log** (EU AI Act Art.12) | **100% single-byte-mutation detection over 256 trials, Wilson 95% [98.5%, 100.0%]**; append-only retention verified; recomputable SHA-256 crypto vector | `cargo run --release -p mnemo-audit-conformance-bench` |
| **Adversarial audit-log tamper-evidence** (EU AI Act Art.12) | **delete / reorder / forge-integrity-field each 100% detected over 200 trials, Wilson 95% [98.1%, 100.0%]**; 0/72 benign false-positives; honest **0%** on payload-only forge + tail truncation (disclosed gaps, shipped mitigations named) | `cargo run --release -p mnemo-audit-tamper-bench` |
| **Memory-poisoning defense delta** (OWASP ASI06) | MINJA **100% → 0% (+100 pts)**; AgentPoison **100% → 3.5% (+96.5 pts)**; **benign control 0/200 false-quarantine** | `cargo run --release -p mnemo-poisoning-bench` |
| **Reproducible-by-disclosure LoCoMo** | single-hop retrieval **recall@1 24.4% [Wilson 95% 14.2%, 38.7%]**, byte-stable, tabled against vendors' cited claims | `cargo run --release -p mnemo-locomo-bench --bin reproduction_bench` |

Sources: [`bench/RESULTS.md`](../bench/RESULTS.md),
[`bench/audit_conformance/results/conformance.md`](../bench/audit_conformance/results/conformance.md),
[`bench/audit_tamper/results/audit_tamper.md`](../bench/audit_tamper/results/audit_tamper.md)
([narrative](benchmarks/audit-log-tamper-evidence.md)),
[`bench/poisoning/results/poisoning_2026-07-07.md`](../bench/poisoning/results/poisoning_2026-07-07.md),
[`bench/locomo/results/reproduction_2026-07-06.md`](../bench/locomo/results/reproduction_2026-07-06.md).

## The comparison

Across the axes a **regulated** deployment actually turns on. `✅` = shipped and
reproducible in this repo (or, for competitors, documented as a first-class
feature); `➖` = partial / conditional; `❌` = not a documented capability
(honest "not found in public docs," not an accusation of absence).

| axis | **mnemo** | Mem0 | Letta | native provider memory |
|---|---|---|---|---|
| **On-prem / self-host, no hosted tier to trust** | ✅ in-process (embedded DuckDB or your PostgreSQL); no SaaS | ➖ OSS core self-hostable, plus a hosted platform | ➖ OSS self-hostable, plus Letta Cloud | ❌ hosted-only; memory lives in the provider |
| **MCP-native primitives** | ✅ REMEMBER / RECALL / FORGET / SHARE *are* MCP tools | ➖ MCP server offered over the store | ➖ integrates MCP tool servers | ❌ provider-internal; not a portable MCP primitive |
| **Cryptographic hash-chain audit log (offline-verifiable)** | ✅ SHA-256 `agent_events` chain; external verifier detects any post-hoc mutation without consulting the store — **proven, 100%/256 trials** | ❌ no tamper-evident / cryptographic audit primitive documented | ❌ not a documented cryptographic audit primitive | ❌ no self-verifiable local audit log exposed to the user |
| **Memory-poisoning defense (measured delta)** | ✅ write-time quarantine + recall skip; **MINJA +100 pts, AgentPoison +96.5 pts, 0/200 benign FPR** (ASR ON vs OFF) | ❌ no benchmarked poisoning-defense primitive documented | ❌ no benchmarked poisoning-defense primitive documented | ❌ opaque; no published poisoning-defense benchmark |
| **Regulatory mapping (EU AI Act Art.12 · India DPDP · OWASP ASI06)** | ✅ per-clause mapping docs wired to the audit bench | ❌ no equivalent published mapping | ❌ no equivalent published mapping | ➖ provider SOC 2 / DPA, but not a self-verifiable memory-log mapping |
| **Recall / answer quality on public leaderboards** | ➖ modest & honest (retrieval metric; see below) | ✅ funded team; strong **published** LLM-judged QA (92.5 claimed) | ✅ established recall/agent-memory research line | ✅ tightly integrated with the frontier model |
| **License** | ✅ Apache-2.0, nothing gated | mixed (OSS + commercial) | mixed (OSS + commercial) | proprietary |

**Sourcing.** Mem0 / Zep-class facts follow each vendor's public docs and the
[Developers Digest 2026 memory-provider survey](https://www.developersdigest.tech/blog/best-ai-agent-memory-providers-2026);
Mem0's 92.5 is its own published figure ([mem0.ai/research](https://mem0.ai/research)),
which community re-runs land materially lower. Letta (formerly MemGPT) is
open-source and self-hostable with MCP tool support; the `❌` cells mean the
capability is **not documented as a first-class feature**, not that it is
impossible to build. "native provider memory" is the *category* of built-in
memory in hosted frontier-model products, not one specific service.

## Where mnemo does not lead

Stated plainly, because the honesty is the credibility:

- **Recall / answer quality — Mem0's funded team wins the number that markets
  the category.** Mem0 publishes an LLM-judged LoCoMo QA figure (92.5). mnemo's
  comparable published numbers are *retrieval* metrics, deliberately modest, and
  **not comparable** to LLM-judged full-dataset QA: LongMemEval_M semantic
  recall@1 **0.739** (real embedder) and LoCoMo single-hop retrieval recall@1
  **24.4%** (offline lexical embedder, no LLM judge). mnemo makes **no claim** to
  beat Mem0, Letta, or a hosted memory service on recall or answer accuracy.
- **mnemo's own *default* hybrid recall is not its best mode.** On the
  paraphrase-heavy LongMemEval_M slice the default `auto`/RRF fusion scores
  recall@1 **0.435** — *below* pure `vector_only` (0.739) — reported as-is in
  [`bench/RESULTS.md`](../bench/RESULTS.md). For single-fact paraphrase recall,
  prefer `strategy="semantic"`.
- **The poisoning defense has disclosed blind spots.** The evasive-MINJA row is
  **+0 pts** (a marker-free false fact evades the lexical lane), and a purely
  in-distribution semantic poison would not trip the z-score gate at all. The
  delta measures what the shipped quarantine buys on two named attacks — **not**
  general poisoning immunity.
- **The audit bench proves a *mechanism*, not legal compliance.** It does not
  enforce a retention calendar (e.g. Art.26(6)'s six-month clock) and is not a
  conformity assessment. It proves the log is complete and tamper-evident — the
  precondition a record-keeping obligation depends on, not the obligation itself.

If your bar is "highest LoCoMo QA," mnemo is the wrong tool. If your bar is "I
can hand an auditor a memory log they can verify offline, on my own
infrastructure, under Apache-2.0," that is the axis mnemo is built for.

## Why this matters now

Three dated forward signals make the compliance-audit axis a *near-term*
procurement question, not a someday-nice-to-have:

- **EU AI Act — Art.12 record-keeping, high-risk obligations apply 2026-08-02.**
  Regulation (EU) 2024/1689 phases in high-risk-system obligations (Art.12
  automatic event logging among them) on **2 August 2026** (Art.113), with
  automatic-log **retention of at least six months** (Art.19(1) provider,
  Art.26(6) deployer). Breach of these provider/deployer obligations sits in the
  Art.99(4) penalty tier: **up to €15,000,000 or 3% of total worldwide annual
  turnover, whichever is higher.** That is the exposure a tamper-evident log
  offsets — proven by the [adversarial tamper-evidence bench](benchmarks/audit-log-tamper-evidence.md)
  (delete / reorder / forge-integrity-field each 100% detected, honest about the
  two disclosed gaps). *Hedge:* a **May-2026 Digital Omnibus** simplification
  proposal may move several high-risk application dates toward **December 2027** —
  a proposal, not enacted law. Source & per-clause mapping:
  [`docs/compliance/eu-ai-act-art12.md`](compliance/eu-ai-act-art12.md).
- **India DPDP — Data-Fiduciary obligations, working commencement 2027-05-13.**
  The Digital Personal Data Protection Act, 2023 + DPDP Rules, 2025 phase in the
  substantive security-safeguard, retention, and record-keeping duties on a later
  notified date; the working figure is **2027-05-13**. Source & mapping:
  [`docs/compliance/dpdp-2027.md`](compliance/dpdp-2027.md).
- **OWASP ASI06 — Memory & Context Poisoning is a named agentic-security risk
  now.** The OWASP Agentic Security Initiative lists memory/context poisoning
  (ASI06) as a top agentic risk; the canonical query-only attack is **MINJA**
  ([arXiv:2503.03704](https://arxiv.org/abs/2503.03704)). mnemo's shipped
  quarantine defense is measured against it. Source & mapping:
  [`docs/security/ASI06.md`](security/ASI06.md).

Each signal maps to a shipped bench: Art.12 → the audit-conformance report plus
the adversarial [tamper-evidence bench](benchmarks/audit-log-tamper-evidence.md),
ASI06 → the poisoning defense delta, DPDP → the same append-only,
tamper-evident, encrypted log plus `Redact`/`HardDelete` erasure primitives.

## Reproduce everything (Apache-2.0, offline)

```bash
# Tamper-evident audit log (EU AI Act Art.12) — byte-stable report + crypto vector
cargo run --release -p mnemo-audit-conformance-bench

# Adversarial audit-log tamper-evidence (EU AI Act Art.12) — delete/reorder/forge/truncate, Wilson-95
cargo run --release -p mnemo-audit-tamper-bench

# Memory-poisoning defense delta (OWASP ASI06) — ASR ON vs OFF, Wilson-95, benign FPR
cargo run --release -p mnemo-poisoning-bench

# Reproducible-by-disclosure LoCoMo single-hop — byte-stable, tabled vs cited claims
cargo run --release -p mnemo-locomo-bench --bin reproduction_bench

# Recall quality with a real embedder (needs local Ollama: `ollama pull nomic-embed-text`)
cargo run --release -p mnemo-locomo-bench --bin semantic_recall_bench
```

Nothing here is paywalled or gated. The point of the benchmarks is not that the
magnitudes win a leaderboard — it is that an outside party can **re-run them and
get the same bytes**, which is the property a compliance story actually needs.
