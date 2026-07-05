# `audit_conformance` — regulated-memory audit-conformance proof

A **deterministic, offline** micro-benchmark that proves mnemo's memory-write
log is **tamper-evident** and **externally verifiable without trusting the
store** — the property EU AI Act Art.12 record-keeping, DPDPA record-of-
processing, and HIPAA §164.312(b) audit controls depend on.

```bash
cargo run --release -p mnemo-audit-conformance-bench
# writes bench/audit_conformance/results/conformance.{md,json}
```

No network, no LLM, no API key. Every run produces a **byte-stable** report
(re-run and `diff` — identical), and the run prints a SHA-256 of the report body
so reproducibility is checkable in one line.

## What it checks

The bench is a *driver + reporter*. Every hash and every verification is a
public, already-shipped `mnemo-core` primitive — the bench never re-implements
cryptography:

| property | built on | claim |
|---|---|---|
| `write_chain_verifies` | `hash::verify_chain`, `MnemoEngine::verify_integrity` | every memory written through the real `remember()` path carries a SHA-256 content hash chained to its predecessor; an external verifier accepts the exported log |
| `event_log_verifies` | `hash::verify_event_chain`, `verify_event_integrity` | the append-only `agent_events` log is itself a valid hash chain |
| `tamper_is_detected` | `hash::verify_chain` | over 256 trials, a single-byte content flip is caught 100% of the time and the first broken record is named (reported with a Wilson 95% interval) |
| `append_only_retention` | `forget` (`SoftDelete`) + event export | `forget` does not erase — it appends a signed `MemoryDelete` event (count only grows, chain still verifies) and the original write row is retained; the audit trail survives deletion |
| `crypto_vector_*` | `hash::compute_content_hash`, `compute_chain_hash` | a fixed, hard-coded input set whose SHA-256 hex is emitted so anyone can recompute it offline and confirm the chaining algorithm |

## Recompute the crypto vector yourself

```bash
printf '%s%s%s' 'patient record created: intake note' 'audit-conformance-agent' '2026-01-01T00:00:00Z' | shasum -a 256
# => 7701c6f4ae7e294b191f5b129264ebca38eb82b361bb156a9b75430a2b6de298   (== content_hash[0] in the report)
```

`content_hash[i] = SHA256(content || agent_id || created_at)`;
`chain_hash[0] = SHA256(content_hash[0])`;
`chain_hash[i>0] = SHA256(content_hash[i] || content_hash[i-1])`.

## What it does NOT claim

- It does **not** enforce a calendar retention window (e.g. the Art.26(6)
  six-month clock) — that is a deployment policy layered on top of this log.
- It is **not** legal compliance. It proves the *mechanism* a record-keeping
  obligation relies on. The mapping to the obligations lives in
  [`docs/compliance/eu-ai-act-art12.md`](../../docs/compliance/eu-ai-act-art12.md)
  and [`docs/compliance/dpdp-2027.md`](../../docs/compliance/dpdp-2027.md).
