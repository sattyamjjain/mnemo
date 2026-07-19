# mnemo retention-conformance report

> **Deterministic, offline proof** that mnemo's append-only `agent_events` log survives every deletion / compaction / cold-tier path within the **dpdp-rules** retention floor of **365 days**. Built entirely on shipped primitives (`forget`, `run_ttl_sweep`, `run_decay_pass`, `run_consolidation`, cold archive) scored by `mnemo_compliance::RetentionProfile`. No network, no LLM. This file is **byte-stable**: re-run and `diff` — it will not change.

Reproduce: `cargo run --release -p mnemo-retention-conformance-bench -- --profile dpdp`

**Obligation:** India DPDP Rules 2025 — retain personal data, traffic data and processing logs (Seventh Schedule)

**Commencement:** 2027-05-13 · **Source:** https://www.meity.gov.in/documents/act-and-policies/digital-personal-data-protection-rules-2025-gDOxUjMtQWa

**Backend:** `duckdb` · **Seed per path:** 24 memory-write + 4 traffic events.

## Conformance — one row per deletion path

| path / check | verdict | detail |
|---|---|---|
| `backend_append_only_gate` | ✅ PASS | backend 'duckdb' events_are_append_only=true; floor=365 days |
| `forget_soft_delete` | ✅ PASS | 28 events before, 29 after (Δ+1); 0 dropped (0 within 365-day floor), 0 rewritten |
| `forget_soft_delete::traffic_metadata` | ✅ PASS | 4/4 traffic-metadata events retained with fields intact |
| `forget_hard_delete` | ✅ PASS | 28 events before, 29 after (Δ+1); 0 dropped (0 within 365-day floor), 0 rewritten |
| `forget_hard_delete::traffic_metadata` | ✅ PASS | 4/4 traffic-metadata events retained with fields intact |
| `forget_redact` | ✅ PASS | 28 events before, 29 after (Δ+1); 0 dropped (0 within 365-day floor), 0 rewritten |
| `forget_redact::traffic_metadata` | ✅ PASS | 4/4 traffic-metadata events retained with fields intact |
| `forget_archive_cold_tier` | ✅ PASS | 28 events before, 29 after (Δ+1); 0 dropped (0 within 365-day floor), 0 rewritten |
| `forget_archive_cold_tier::traffic_metadata` | ✅ PASS | 4/4 traffic-metadata events retained with fields intact |
| `ttl_sweep_hard_expiry` | ✅ PASS | 28 events before, 29 after (Δ+1); 0 dropped (0 within 365-day floor), 0 rewritten |
| `ttl_sweep_hard_expiry::traffic_metadata` | ✅ PASS | 4/4 traffic-metadata events retained with fields intact |
| `decay_pass` | ✅ PASS | 28 events before, 28 after (Δ+0); 0 dropped (0 within 365-day floor), 0 rewritten |
| `decay_pass::traffic_metadata` | ✅ PASS | 4/4 traffic-metadata events retained with fields intact |
| `consolidation` | ✅ PASS | 28 events before, 28 after (Δ+0); 0 dropped (0 within 365-day floor), 0 rewritten |
| `consolidation::traffic_metadata` | ✅ PASS | 4/4 traffic-metadata events retained with fields intact |

**Overall: CONFORMANT.**

## What this does and does NOT claim

- **Does:** every deletion path edits *memory content* and *appends* an audit event; none removes or rewrites an `agent_events` row, so the processing log — including traffic/processing metadata — is retained across the floor. The backend gate fails loud (`ComplianceError::RetentionFloorUnsupported`) on a backend that cannot guarantee this.
- **Does NOT:** enforce a calendar clock, delete data *after* the floor, or constitute legal compliance. It is a **conformance check for** the named obligation's retention mechanism — not a certification. See [`docs/compliance/dpdp-2027.md`](../../../docs/compliance/dpdp-2027.md) and [`docs/compliance/eu-ai-act-art12.md`](../../../docs/compliance/eu-ai-act-art12.md).

Report dated 2026-07-19.
