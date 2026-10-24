# audit-log tamper-evidence — adversarial attacks vs. `verify_event_chain`

> Post-hoc attacks on a real, 64-event `agent_events` hash chain, scored by mnemo's shipped `verify_event_chain` (the verifier `verify_event_integrity` runs). **Detection rate** with a **Wilson 95%** interval per attack; **honest** about the two classes the pure chain verifier does not catch. Deterministic, offline, byte-stable.

- Trials/attack: 200; chain length: 64; embedder: Noop (offline); backend: in-memory DuckDB.
- Each attack mutates an exported copy of the chain and re-runs the verifier; the store is not consulted.

## Detection rate

| attack | threat | detection | Wilson 95% | caught by chain? |
|---|---|---:|---:|:--:|
| delete (mid-chain) | remove one event from the middle of the log | 200/200 (100.0%) | [98.1%, 100.0%] | ✅ yes |
| reorder (swap two events) | swap two adjacent events to change ordering | 200/200 (100.0%) | [98.1%, 100.0%] | ✅ yes |
| forge (integrity field content_hash) | rewrite the hashed content_hash of one event | 200/200 (100.0%) | [98.1%, 100.0%] | ✅ yes |
| forge (payload only) | rewrite an event's payload JSON, leaving content_hash intact | 0/200 (0.0%) | [0.0%, 1.9%] | ❌ NO |
| truncate (tail) | drop the last k events from the log | 0/200 (0.0%) | [0.0%, 1.9%] | ❌ NO |

## Benign control

Legitimately-appended chain of 72 events: **0/72 falsely flagged (0.0%)** [Wilson 95% 0.0%–5.1%]. A trustworthy verifier must accept every legitimate append — the detection rates above are only meaningful at ~0% false positives.

## Honest reading (do not oversell)

- **delete (mid-chain)** — the successor's prev_hash no longer links to its new predecessor; verifier names the first orphaned event
- **reorder (swap two events)** — both swapped positions' prev_hash linkage break
- **forge (integrity field content_hash)** — tampering the hashed field breaks the successor's prev_hash
- **forge (payload only)** — GAP: verify_event_chain does not recompute content_hash from the payload, so a payload-only rewrite is not caught. Mitigations mnemo ships: the memory record's content is hash-bound (verify_chain recomputes it); Postgres' prevent_event_modification trigger blocks in-place UPDATE
- **truncate (tail)** — GAP: the surviving prefix is a valid chain; a pure verifier has no expected length/head anchor. Postgres' append-only trigger blocks tail deletion through ordinary SQL; on DuckDB nothing does yet (checkpoints are unsigned and record no head hash or count)

The pure chain verifier is tamper-**evidence** for deletion, reordering, and any edit of the hashed integrity fields that is not re-hashed — not a guarantee against every edit. Payload-only forgery and tail truncation are disclosed gaps, with what narrows them named above; they are not claimed as caught. None of this holds against a writer with access to the store, who can recompute the chain. No "best"/"first" claim.

Reproduce: `cargo run --release -p mnemo-audit-tamper-bench` (report dated 2026-07-16).
