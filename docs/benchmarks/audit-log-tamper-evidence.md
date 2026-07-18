# Audit-log tamper-evidence — adversarial attacks vs. `verify_event_chain`

> **Not legal advice; not a leaderboard claim.** Every number below is produced
> by a benchmark that ships in this repository (Apache-2.0) and is reproducible
> offline. The bench builds a **real** `agent_events` hash chain through the
> shipped `remember()` path and scores four post-hoc attacks with mnemo's shipped
> `verify_event_chain` — the same verifier `MnemoEngine::verify_event_integrity`
> runs. It is deliberately **honest about the two attack classes the pure chain
> verifier does not catch**.

## Why this matters (EU AI Act Art.12)

Art.12 of Regulation (EU) 2024/1689 requires high-risk AI systems to keep
**automatic, tamper-evident event logs** over their lifetime; Art.19(1) (provider)
and Art.26(6) (deployer) require retaining those logs for **at least six months**.
Breach of these provider/deployer obligations sits in the Art.99(4) penalty tier:
**up to €15,000,000 or 3% of total worldwide annual turnover, whichever is
higher.** A record-keeping obligation is only as good as the log's tamper-evidence
— the property this bench measures directly. Per-clause mapping:
[`docs/compliance/eu-ai-act-art12.md`](../compliance/eu-ai-act-art12.md).

## What the chain is

Every write through `remember()` appends a `MemoryWrite` event to an append-only
`agent_events` log. Each event carries a SHA-256 `content_hash` and a
`prev_hash = SHA256(content_hash ‖ predecessor.content_hash)`, forming a hash
chain (`crates/mnemo-core/src/query/event_builder.rs`,
`crates/mnemo-core/src/hash.rs`). `verify_event_chain` walks the exported log and
rejects it — naming the first broken event — if any linkage does not recompute.

## The attacks and the result

200 independent trials per attack against a 64-event chain; each trial mutates an
**exported copy** of the log (the store is never consulted) and re-runs the
verifier. Detection outcomes are structural, so the counts are deterministic and
the report is **byte-stable** across runs and machines.

| attack | threat | detection | Wilson 95% | caught by chain? |
|---|---|---:|---:|:--:|
| **delete (mid-chain)** | remove one event from the middle of the log | **200/200 (100.0%)** | [98.1%, 100.0%] | ✅ yes |
| **reorder (swap two events)** | swap two adjacent events to change ordering | **200/200 (100.0%)** | [98.1%, 100.0%] | ✅ yes |
| **forge (integrity field `content_hash`)** | rewrite the hashed `content_hash` of one event | **200/200 (100.0%)** | [98.1%, 100.0%] | ✅ yes |
| **forge (payload only)** | rewrite an event's `payload` JSON, leaving `content_hash` intact | **0/200 (0.0%)** | [0.0%, 1.9%] | ❌ **NO** |
| **truncate (tail)** | drop the last _k_ events from the log | **0/200 (0.0%)** | [0.0%, 1.9%] | ❌ **NO** |

**Benign control.** A legitimately-appended 72-event chain is falsely flagged
**0/72 (0.0%)** [Wilson 95% 0.0%–5.1%]. A trustworthy verifier must accept every
legitimate append — the detection rates above are only meaningful at ~0% false
positives.

## Honest reading — the two gaps and their shipped mitigations

The pure chain verifier is tamper-**evidence** for deletion, reordering, and any
edit of the hashed integrity fields — **not** a guarantee against every edit. Two
classes are disclosed gaps, **not** claimed as caught:

- **forge (payload only) — NOT caught.** `verify_event_chain` binds each event's
  `content_hash` (a SHA-256 of the operation's source string), and does **not**
  recompute it from the arbitrary `payload` JSON, so a payload-only rewrite that
  leaves `content_hash`/`prev_hash` intact passes. Mitigations mnemo ships: the
  underlying **memory record's content is hash-bound** (`verify_chain` recomputes
  it and catches content edits at 100% — see the audit-conformance bench), and
  PostgreSQL's `prevent_event_modification` trigger blocks in-place `UPDATE` on
  `agent_events`. Binding the full event into `content_hash` would close it in the
  pure verifier too.
- **truncate (tail) — NOT caught.** The surviving prefix is itself a valid chain,
  and a pure chain verifier has no expected length or head anchor. Mitigations
  mnemo ships: a signed **checkpoint** records the expected latest hash + count,
  and the PostgreSQL append-only trigger blocks tail deletion.

No "best" / "first" claim. The bench proves the *mechanism* a record-keeping
obligation depends on — it is not itself a conformity assessment and does not
enforce the ≥6-month retention calendar (a deployment policy on top of the log).

## Reproduce

```bash
cargo run --release -p mnemo-audit-tamper-bench
```

Writes a byte-stable report to
[`bench/audit_tamper/results/audit_tamper.md`](../../bench/audit_tamper/results/audit_tamper.md)
(+ `audit_tamper.json`). Re-run and `diff` — the bytes will not change. The
contract (100% on the three structural attacks, 0% on the two disclosed gaps,
0 benign false positives) is pinned by `bench/audit_tamper/tests/tamper.rs`.

Companion proof that the memory-write log itself is tamper-evident and carries a
recomputable SHA-256 crypto vector:
[`docs/benchmarks`](.) → `cargo run --release -p mnemo-audit-conformance-bench`.
