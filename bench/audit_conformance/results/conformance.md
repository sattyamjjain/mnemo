# mnemo audit-conformance report

> **Deterministic, offline proof** that mnemo's memory-write log is tamper-evident and externally verifiable without trusting the store. Built entirely on shipped `mnemo-core` primitives (`hash::verify_chain`, `hash::verify_event_chain`, `MnemoEngine::verify_integrity`, `verify_event_integrity`). No network, no LLM. This file is **byte-stable**: re-run and `diff` — it will not change.

Reproduce: `cargo run --release -p mnemo-audit-conformance-bench`

**Parameters:** 64 records written through the real `remember()` path; 256 single-byte tamper trials.

## Conformance

| property | verdict | detail |
|---|---|---|
| `write_chain_verifies` | ✅ PASS | 64/64 exported records verify (SHA-256 content+prev_hash chain); engine.verify_integrity agrees=true |
| `event_log_verifies` | ✅ PASS | 64 append-only events verify (one MemoryWrite per remember); engine.verify_event_integrity agrees=true |
| `tamper_is_detected` | ✅ PASS | 256/256 single-byte mutations caught (rate 100.0%, Wilson95 [98.5%, 100.0%]) |
| `append_only_retention` | ✅ PASS | forget appended exactly 1 event (64→65), event chain still verifies=true, original write row retained (deleted_at set)=true, active chain still valid=true |
| `crypto_vector_pristine_verifies` | ✅ PASS | fixed 3-write chain verifies (3/3 records) |
| `crypto_vector_tamper_detected` | ✅ PASS | one-byte content flip rejected; first_broken_at = fixed uuid 00000000-0000-0000-0000-000000000002 (record #2) |

**Overall: CONFORMANT.**

Tamper-detection rate over 256 trials: **100.0%** (Wilson 95% [98.5%, 100.0%]). A finite sample cannot *prove* 100%; the Wilson lower bound is the honest floor.

## Recomputable crypto vector

Fixed inputs → fixed SHA-256, so you can recompute the hex offline with any SHA-256 tool and confirm the chaining algorithm:

```json
{
  "agent_id": "audit-conformance-agent",
  "chain_hash_sha256_hex": [
    "f19f27d7b5ea0bce7dacb338e8ba38bdbe63ff6baec30de94636d8de0c0d23a5",
    "a0aeb0b6b24d8cca2ec8f4fcc7aa1cd0846e4ef7a49e007370ab5d12c3782008",
    "bc42da29ba1e874c64409ffc93f780986c10836011861a3c0fabff3f34b6fb31"
  ],
  "content_hash_sha256_hex": [
    "7701c6f4ae7e294b191f5b129264ebca38eb82b361bb156a9b75430a2b6de298",
    "09eef074d4084c02c4a56e9eb8b3657068c0e571b9eac4ac7079c1b321995898",
    "fa8a071e2ed6ec6d100e3242e04b0853c71e5444bb8bfe7ffbe0a2f3afc82fbe"
  ],
  "inputs": [
    {
      "content": "patient record created: intake note",
      "created_at": "2026-01-01T00:00:00Z"
    },
    {
      "content": "dosage adjusted to 5mg by clinician",
      "created_at": "2026-01-01T00:00:01Z"
    },
    {
      "content": "discharge summary finalised",
      "created_at": "2026-01-01T00:00:02Z"
    }
  ],
  "recompute": "content_hash[i] = SHA256(content[i] || agent_id || created_at[i]); chain_hash[0] = SHA256(content_hash[0]); chain_hash[i>0] = SHA256(content_hash[i] || content_hash[i-1])",
  "tamper": {
    "detected": true,
    "first_broken_at_uuid": "00000000-0000-0000-0000-000000000002",
    "mutated_record_index": 1
  }
}
```

## What this does and does NOT claim

- **Does:** the write log is an append-only SHA-256 hash chain; an external verifier detects any post-hoc mutation and names the first broken record; `forget` appends a signed delete event and retains the original write (row + event), so the audit trail survives deletion.
- **Does NOT:** enforce a calendar retention window (e.g. the EU AI Act Art.26(6) six-month clock) — that is a deployment policy on top of this log — and does NOT itself constitute legal compliance. It proves the *mechanism* a record-keeping obligation depends on. See [`docs/compliance/eu-ai-act-art12.md`](../../../docs/compliance/eu-ai-act-art12.md) and [`docs/compliance/dpdp-2027.md`](../../../docs/compliance/dpdp-2027.md).
