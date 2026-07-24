# ASI06 — auditable memory-poisoning resistance

> **The number:** across three ASI06 poisoning attack families, mnemo's
> **auditable layer rejects 100% of cover-up / forgery attempts**
> (1500/1500, **Wilson 95% [99.7%, 100.0%]**), at **0% benign false-positive**
> (0/300, [0.0%, 1.3%]). A naive store with no cryptographic layer catches
> **0%** — it has no primitive that *could*.
>
> **Read this honestly.** "Resistance" here is **tamper-evidence + attribution**,
> not write-time prevention. The auditable layer does **not** stop a poison from
> being written; it makes poisoning **impossible to hide** — any attempt to erase
> the true fact, forge a clean provenance, or splice the drift trail out of the
> history is cryptographically rejected by an *offline* verifier. Write-time
> *quarantine* is a **separate** layer, measured elsewhere (see
> [Two different layers](#two-different-layers)).

- Runner: [`bench/asi06_poisoning/`](../../bench/asi06_poisoning)
- Raw result (deterministic key order, no wall-clock): [`bench/results/asi06_poisoning.json`](../../bench/results/asi06_poisoning.json)
- One command: `cargo run --release -p mnemo-asi06-poisoning-bench --bin asi06_poisoning`

## Positioning — the wedge

mnemo is **on-prem, MCP-native, cryptographically-auditable memory for regulated
AI** (EU AI Act Art.12 record-keeping · India DPDPA · HIPAA §164.312(b) audit
controls). The incumbents in the agent-memory space compete on **recall quality**
leaderboards. mnemo's wedge is a different axis that regulated deployments
actually have to satisfy: **can you prove, offline, that your agent's memory was
not silently poisoned or rewritten?**

[OWASP Agentic Top-10 **ASI06 — Memory & Context Poisoning**](https://genai.owasp.org/2025/12/09/owasp-top-10-for-agentic-applications-the-benchmark-for-agentic-security-in-the-age-of-autonomous-ai/)
names the persistent-memory attack — "an attacker writes today and the agent acts
wrongly months later" — and its **recommended control is provenance metadata on
every memory write plus periodic evaluation against ground truth**
([OWASP, 2026](https://genai.owasp.org/2026/05/13/memory-is-a-feature-it-is-also-an-attack-surface/)).
This benchmark measures exactly that control, shipped and wired in mnemo.

## What is exercised (mnemo's shipped auditable primitives)

The bench never re-implements crypto — it drives two shipped functions:

| primitive | file | what it proves |
|---|---|---|
| `hash::verify_chain` | [`crates/mnemo-core/src/hash.rs`](../../crates/mnemo-core/src/hash.rs) | every write is a SHA-256 `content_hash` chained by `prev_hash`; recompute detects any content edit, hash edit, splice, reorder, or back-date |
| `provenance::verify_read_provenance` | [`crates/mnemo-core/src/provenance.rs`](../../crates/mnemo-core/src/provenance.rs) | every audited recall carries an HMAC receipt binding the answer to the exact records (+ their hashes) it derived from; recompute detects a forged receipt or a post-recall record swap |

## Threat model & metric

A realistic ASI06 adversary does not just poison — they **cover their tracks**,
because a poisoning that shows up in a signed audit trail is a poisoning the
regulator (or the clinician, or the incident responder) will catch. For each
attack family the bench builds a tamper-evident store, injects a poison, applies
the adversary's **cover-up**, and records whether the auditable verifier
**rejects** it.

> **resistance = rejected / attempts**, per family, over deterministic trials,
> with a **Wilson 95%** interval. **benign FPR** = legitimate operations wrongly
> rejected / total.

The primitives are cryptographic and deterministic, so this is **byte-stable and
fully offline** — no embedder, no database, no network, no seed.

## Attack families (roadmap #37: contradictory facts · authority-spoof · belief-drift)

Each maps to a channel in the 2026 poisoning literature
([arXiv:2606.24322](https://arxiv.org/abs/2606.24322), *Securing LLM-Agent
Long-Term Memory Against Poisoning: Non-Malleable, Origin-Bound Authority* —
summarization / trusted-tool / manufactured-corroboration channels):

1. **Contradictory-fact silent overwrite** — append a poison contradicting a gold
   fact (a *valid* write; the auditable layer does not block it), then **rewrite
   the gold record in place** to erase the true value. → `verify_chain` recomputes
   the gold record's `content_hash` from its (now-changed) content → mismatch →
   **rejected**.
2. **Authority-spoofed origin + provenance forgery** — the poison claims a trusted
   origin; the attacker then makes the audit trail show the answer derived only
   from *trusted* records, either by **forging a read-receipt** (they lack the
   server HMAC key) or by **swapping a cited record** after the real receipt was
   signed. → `verify_read_provenance` recomputes the HMAC / compares the cited
   `content_hash` → **rejected**.
3. **Belief-drift trail splice / back-date** — inject a gradual drift sequence,
   then hide it by **splicing** an intermediate record out of the exported history
   or **back-dating** the poison to look long-standing. → `verify_chain` sees the
   broken `prev_hash` link (or the timestamp-driven `content_hash` mismatch) →
   **rejected**.

## Results

Deterministic run — 500 cover-up attempts / family, 300 benign controls:

| attack family | primitive | resistance | 95% CI |
|---|---|---:|---|
| Contradictory-fact silent overwrite | `verify_chain` (content_hash) | **100.0%** | [99.2%, 100.0%] |
| Authority-spoof + provenance forgery | `verify_read_provenance` (HMAC + binding) | **100.0%** | [99.2%, 100.0%] |
| Belief-drift splice / back-date | `verify_chain` (prev_hash + content_hash) | **100.0%** | [99.2%, 100.0%] |
| **Overall** | 1500 attempts | **100.0%** | **[99.7%, 100.0%]** |

**Benign false-positive control:** **0 / 300** legitimate operations wrongly
rejected = **0.0%** [95% 0.0%, 1.3%]. The controls are honest fact
*supersession* (append, not rewrite), signing-**key rotation** (verified via the
retained historical key), and legitimate **consolidation** (originals retained,
chain extended) — each superficially resembles an attack but is not one.

**Naive baseline:** a store with no `content_hash` chain and no signed receipt
rejects **0 / 1500** — by construction, it holds no primitive that *could* detect
any of these cover-ups. That gap is the entire thesis.

> A resistance rate is meaningless without the false-positive control.
> [arXiv:2606.30566](https://arxiv.org/abs/2606.30566) (*Forensic Trajectory
> Signatures for Agent Memory Poisoning Detection*) finds that naive behavioral
> *detection* of poisoning carries **24.7–52.6% false positives** — "standalone
> blocking is not viable". mnemo's resistance here is cryptographic
> tamper-evidence, not behavioral detection, which is why it holds at 0% FP.

## Two different layers

mnemo has **two** independent poisoning defenses; do not conflate them:

| layer | mechanism | blocks the write? | benchmark |
|---|---|---|---|
| **Write-time quarantine** | anomaly detector (lexical + embedding z-score) skips flagged writes at recall | yes (when it fires) | [`docs/BENCH_POISONING.md`](../BENCH_POISONING.md), [`bench/poisoning`](../../bench/poisoning); ASI06 quarantine-resistance micro-bench [`bench/locomo/src/bin/asi06_resistance.rs`](../../bench/locomo/src/bin/asi06_resistance.rs) |
| **Auditable layer** (*this bench*) | SHA-256 hash-chain + read-provenance HMAC | **no** — makes poisoning tamper-evident & attributable | this document |

The honest, complete story: the quarantine layer *tries to stop* poison at write
time (and, as its own bench shows, has real blind spots — marker-free semantic
poison survives); the auditable layer *guarantees* that whatever gets through
**cannot be hidden or denied**. Defense-in-depth for regulated AI.

## Limitations — what this is **NOT**

- **Not write-time prevention.** This layer detects/attributes; it does not stop
  the initial poisoned write. See the table above.
- **Cryptographic, not semantic.** It proves the *record log* wasn't tampered; it
  does not judge whether a (validly-written) memory is *true*. Truth-checking is
  OWASP's other ASI06 control — "evaluation against ground truth" — which is an
  operator responsibility on top of the tamper-evident substrate.
- **HMAC receipts are keyholder-verifiable.** For externally-auditable,
  non-repudiable receipts, pair with the `mnemo-compliance` Ed25519-signed
  audit-log export.
- **Not a recall-quality claim.** This deliberately does **not** chase the
  LoCoMo / LongMemEval leaderboards (e.g. [Mem0's 93.4% LongMemEval](https://mem0.ai/research)).
  Those leaderboards are noisy ground truth: an independent audit found
  [**6.4% of LoCoMo's answer key is wrong** and its LLM judge accepts up to 63% of
  intentionally-wrong answers](https://penfieldlabs.substack.com/p/we-audited-locomo-64-of-the-answer).
  mnemo's wedge is auditability under adversarial pressure, not a leaderboard rank
  — see [`docs/benchmarks/locomo-v1.md`](locomo-v1.md) for our honest,
  wide-CI retrieval numbers.

## Sources

- [OWASP Top 10 for Agentic Applications (2026)](https://genai.owasp.org/2025/12/09/owasp-top-10-for-agentic-applications-the-benchmark-for-agentic-security-in-the-age-of-autonomous-ai/) · [Memory Is a Feature. It Is Also an Attack Surface (OWASP, 2026)](https://genai.owasp.org/2026/05/13/memory-is-a-feature-it-is-also-an-attack-surface/)
- [arXiv:2606.24322 — Securing LLM-Agent Long-Term Memory Against Poisoning: Non-Malleable, Origin-Bound Authority](https://arxiv.org/abs/2606.24322)
- [arXiv:2606.30566 — Forensic Trajectory Signatures for Agent Memory Poisoning Detection](https://arxiv.org/abs/2606.30566)
- [Mem0 research (93.4% LongMemEval)](https://mem0.ai/research) · [We audited LoCoMo: 6.4% of the answer key is wrong](https://penfieldlabs.substack.com/p/we-audited-locomo-64-of-the-answer)
