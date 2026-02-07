# Compliance Overview

Mnemo is an MCP-native memory database for AI agents. This section documents how
Mnemo's architecture and feature set align with industry compliance frameworks,
specifically **SOC 2 Type II** and **HIPAA**. The goal is to provide operators,
auditors, and prospective customers with a clear mapping between regulatory
requirements and the technical controls that Mnemo implements.

## Compliance Posture Summary

Mnemo was designed with security and auditability as first-class concerns. The
following capabilities form the foundation of its compliance story:

| Capability | Module | Description |
|---|---|---|
| Encryption at rest | `encryption.rs` | AES-256-GCM content encryption with HMAC-based integrity tags |
| Hash chain verification | `hash.rs` | SHA-256 content hashes linked into a tamper-evident chain |
| Role-based access control | `acl.rs` | Six-level permission hierarchy (Read through Admin) |
| Delegation model | `delegation.rs` | Transitive, scoped, time-bounded permission delegation |
| Memory poisoning detection | `poisoning.rs` | Anomaly scoring against agent behavioral baselines |
| Immutable audit log | `event.rs` | Append-only AgentEvent log with OpenTelemetry fields |
| TTL enforcement | `MemoryRecord.expires_at` | Automatic expiration filtering during recall |
| Quarantine | `MemoryRecord.quarantined` | Flagged memories excluded from recall results |
| Checkpoint/Branch/Merge | `checkpoint.rs` | Git-like state management with full version history |
| Cognitive forgetting | `lifecycle.rs` | Ebbinghaus-inspired decay with configurable functions |

## Documents in This Section

- **[SOC 2 Controls](./soc2-controls.md)** -- Maps each SOC 2 Trust Service
  Criteria category (CC1 through CC9) to specific Mnemo features, implementation
  modules, and current status.

- **[HIPAA Safeguards](./hipaa-safeguards.md)** -- Maps HIPAA Administrative,
  Physical, and Technical Safeguards to Mnemo capabilities, identifies gaps, and
  provides recommendations for covered-entity deployments.

## How to Use This Documentation

**For auditors:** Each control mapping includes the control identifier,
a description of the requirement, the specific Mnemo module or feature that
addresses it, the current implementation status, and any known gaps with
recommended mitigations.

**For operators:** Use this documentation to understand which compliance controls
Mnemo provides out of the box and which require additional operational procedures,
infrastructure configuration, or third-party tooling.

**For developers:** The module references in each control mapping point directly
to source files in `crates/mnemo-core/src/`. Consult those files for
implementation details when extending or auditing controls.

## Implementation Status Legend

Throughout the compliance documents, the following status labels are used:

| Status | Meaning |
|---|---|
| **Implemented** | The control is fully implemented in code and tested |
| **Partially Implemented** | Core functionality exists but additional work is needed for full coverage |
| **Planned** | The control is on the roadmap but not yet implemented |
| **Operational** | The control depends on deployment-time configuration or external processes, not application code |

## Versioning

This compliance documentation reflects the state of Mnemo as of Sprint 3
completion (67 tests passing, 10 MCP tools, 26-method StorageBackend trait).
It should be updated whenever security-relevant features are added or modified.

| Version | Date | Changes |
|---|---|---|
| 1.0 | 2026-02-07 | Initial compliance documentation covering SOC 2 and HIPAA |
