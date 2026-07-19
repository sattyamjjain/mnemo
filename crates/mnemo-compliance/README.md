# mnemo-compliance

[![crates.io](https://img.shields.io/crates/v/mnemo-compliance.svg)](https://crates.io/crates/mnemo-compliance)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Compliance primitives for **[Mnemo](https://github.com/sattyamjjain/mnemo)** —
the on-prem, MCP-native, cryptographically-auditable memory database for AI
agents. This crate layers **DPDPA consent management** and **EU AI Act Art.12
audit-log export** on top of [`mnemo-core`](https://crates.io/crates/mnemo-core)'s
tamper-evident hash chain.

> **Not legal advice.** These are the *technical* substrate — an append-only,
> tamper-evident, externally-verifiable memory-write log with signed export and
> consent records — not a compliance certificate. See the honest, hedged
> regulatory mappings:
> [EU AI Act Art.12](https://github.com/sattyamjjain/mnemo/blob/main/docs/compliance/eu-ai-act-art12.md)
> · [India DPDP](https://github.com/sattyamjjain/mnemo/blob/main/docs/compliance/dpdp-2027.md).

## Install

```bash
cargo add mnemo-core mnemo-compliance
```

## What it provides

- **`export_audit_log` / `AuditBundle` / `AuditFormat`** — export the memory-write
  event log in an auditor-friendly form (NDJSON), optionally **Ed25519-signed**.
- **`verify_ndjson_signed`** — verify a signed export offline, without the store.
- **`HttpConsentManager` / `ConsentState` / `ConsentScope`** — DPDPA-style
  consent records (the *substrate*; notice/grievance live at the app layer).
- **`RetentionProfile` / `RetentionReport`** — processing-log retention-conformance
  profiles (DPDP Rules 2025 → 365 days; EU AI Act Art.19/26(6) → 180 days; HIPAA
  §164.312(b) → six years). Verifies, over before/after event snapshots, that no
  deletion / compaction / cold-tier path dropped or rewrote a log row inside the
  floor — and **fails loud** (`ComplianceError::RetentionFloorUnsupported`, naming
  the backend) when a backend cannot guarantee an append-only log. A **conformance
  check for** the named obligation — not a certification.

The tamper-evidence these build on is proven — deterministically, offline — by
the [`audit-conformance`](https://github.com/sattyamjjain/mnemo/tree/main/bench/audit_conformance)
bench: 100% single-byte-mutation detection over 256 trials (Wilson 95%
[98.5%, 100%]).

## Positioning

How Mnemo compares to Mem0 / Letta / native provider memory on the
compliance-audit axis:
**[docs/POSITIONING.md](https://github.com/sattyamjjain/mnemo/blob/main/docs/POSITIONING.md)**.

## License

Apache-2.0.
