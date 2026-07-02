# OWASP ASI06 — Memory & Context Poisoning: how mnemo maps + a published resistance number

> **Status:** honest, reproducible. This page maps mnemo's existing surfaces to
> OWASP Agentic Security Initiative **ASI06 (Memory & Context Poisoning)** and
> publishes a **memory-poisoning resistance micro-benchmark** for a query-only
> MINJA-style attack. It is **not** a claim of general poisoning immunity — read
> the Limitations section.

## What ASI06 covers

OWASP ASI06 is the agentic-security risk where an attacker corrupts an agent's
**persistent memory or retrieved context** so that later, benign-looking recalls
surface attacker-controlled "facts" or instructions. For a store like mnemo the
relevant attack is **query-only MINJA**
([Memory INJection Attack, arXiv:2503.03704](https://arxiv.org/abs/2503.03704)):
the attacker never touches the database — they feed attacker-controlled content
through content the agent processes (a retrieved web page, a document), causing
the agent to *write* a poisoned memory that later gets recalled as if it were a
trusted fact.

## mnemo surface → ASI06 control mapping

| mnemo surface | Code path | ASI06 role |
|---|---|---|
| **REMEMBER anomaly scan** | `query::poisoning::check_for_anomaly` (runs on every `remember`) | **Write-time detection.** Scores each new record on prompt-injection patterns, MINJA self-referential bridging markers on indirect-ingest records (`SourceType::Retrieval`/`source:*` tags), agent-profile importance/length outliers, and (opt-in) an embedding z-score baseline. Score ≥ 0.5 ⇒ anomalous. |
| **Quarantine** | `query::poisoning::quarantine_memory` | **Containment.** An anomalous record is written but flagged `quarantined = true`; it stays auditable but is fenced off from serving. |
| **RECALL quarantine filter** | `query::recall` shared `passes_filters` (`if record.quarantined { return false }`) | **Read-time suppression.** Quarantined records are excluded from *every* recall lane (vector, BM25, graph), so a poisoned write cannot re-enter agent context via retrieval. |
| **Hash-chained events + memories** | `hash::compute_chain_hash`, `verify` / `verify_event_integrity` | **Tamper-evidence.** Even a record that slips detection cannot be silently altered or back-dated without breaking the chain, and `replay_quarantine` gives operators a deterministic review queue. |
| **Append-only audit log** | PostgreSQL `prevent_event_modification` trigger | **Non-repudiation** of the write/quarantine trail. |

## The published resistance number

Measured by [`bench/locomo/src/bin/asi06_resistance.rs`](../../bench/locomo/src/bin/asi06_resistance.rs)
(runnable: `cargo run --release -p mnemo-locomo-bench --bin asi06_resistance`),
**200 deterministic trials per attack class, top-5 recall, seed `0xa510062026`,
2026-07-02, at commit `300d80d`** (base). The record is written once via the real
`remember` path; the **only** difference between arms is the quarantine gate
(UNDEFENDED = the identical record with `quarantined` forced back to `false`,
i.e. a store with no poisoning detector). *Poisoning success* = the poison record
is returned in the attacker's target-query top-5.

| attack class | undefended poisoning | defended poisoning | quarantined | **resistance** (Wilson 95%) |
|---|---:|---:|---:|---:|
| `canonical_minja` (bridging markers) | 100.0% | 0.0% | 200/200 | **100.0%** [98.1%, 100.0%] |
| `evasive_paraphrase` (markers stripped) | 100.0% | 100.0% | 0/200 | 0.0% [0.0%, 1.9%] |

**Headline:** against the canonical query-only MINJA poison — the attack as the
paper describes it, which relies on self-referential bridging phrasing to make
the false fact "stick" — mnemo's existing defense quarantines **200/200** and
suppresses it from recall: **100.0% resistance, Wilson 95% CI [98.1%, 100.0%]**.
In an otherwise-identical store *without* the detector the same poison is
recalled 100% of the time.

## Methodology (so the number is checkable, not marketing)

- **Threat model:** query-only. Poison arrives via an indirect-ingest path
  (`SourceType::Retrieval`, tagged `source:web`) — the attacker cannot write to
  the DB directly.
- **Attack:** each trial seeds a small benign corpus (including the *true* fact
  for a per-trial target subject), then injects a *false* fact for the same
  subject. Canonical trials carry MINJA bridging markers ("remember this",
  "whenever asked about …", "from now on, always"); evasive trials strip them.
- **Defended vs undefended** isolates exactly one variable — the `quarantined`
  flag — so the number measures the *defense*, not a difference in the record or
  the retrieval.
- **Deterministic:** fixed seed, in-process splitmix64, in-memory DuckDB +
  Tantivy per trial; re-running reproduces the table.
- **Confidence:** Wilson 95% score interval on the defended poisoning rate,
  mapped through `resistance = 1 − rate`.

## Limitations (do not over-read the headline)

- **Query-only MINJA variant, not a full adversarial suite.** One attack family,
  one target-fact shape. It is not gradient-optimized, multi-turn, or
  collusion-based poisoning.
- **The lexical layer has a real blind spot.** The `evasive_paraphrase` row is
  the honest counter-example: a marker-free semantic paraphrase of the false
  fact is **not** caught (0.0% resistance) because the always-on layer keys on
  injection/bridging *phrasing*. The intended defense there is the **opt-in**
  embedding z-score baseline gate
  (`PoisoningPolicy::with_outlier_threshold`, [`crate::anomaly::outlier`]), which
  needs a trained per-agent baseline and is **not exercised** in this run.
- **Single, degenerate embedder.** The bench uses `NoopEmbedding`; BM25/Tantivy
  carries retrieval. Numbers are about *detection + quarantine*, not embedding
  quality, and do not transfer to a claim about semantic-drift detection.
- **Detection is heuristic.** A score ≥ 0.5 threshold over lexical + profile
  signals; a determined attacker who avoids every marker and stays within the
  agent's importance/length envelope can evade the lexical layer, which is
  exactly why quarantine is *containment with an audit trail*, not a guarantee.

## Reproduce

```bash
cargo run --release -p mnemo-locomo-bench --bin asi06_resistance
# → bench/locomo/results/asi06_resistance_<date>.{md,json}
# flags: --trials 200  --k 5  --seed 0xa510062026
```
