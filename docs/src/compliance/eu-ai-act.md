# EU AI Act — audit log export

The EU AI Act (enforceable from **2 August 2026** for GPAI providers)
requires retention of event logs with integrity controls and a path to
export records for AI Office document requests. Mnemo's
`mnemo-compliance` crate ships a matching surface.

## `export_audit_log`

```rust
use mnemo_compliance::{
    AuditFormat, AuditSigner, export_audit_log, verify_ndjson_signed,
};

let events = engine.storage.list_events("agent-id", 10_000, 0).await?;
let signer = AuditSigner::from_secret_bytes(&ed25519_secret);

// NDJSON with detached Ed25519 signature chain
let bundle = export_audit_log(
    &events,
    AuditFormat::NdjsonSigned,
    Some(&signer),
)?;
std::fs::write("audit.ndjson", bundle.bytes)?;

// Reverse: verify
let verified = verify_ndjson_signed(
    &std::fs::read("audit.ndjson")?,
    bundle.verifying_key_hex.as_ref().unwrap(),
)?;
```

### Supported formats

* **`AuditFormat::NdjsonSigned`** — one JSON line per event plus a
  detached Ed25519 signature that covers
  `SHA256(index ∥ prev_hash ∥ event_json)`. Canonicalises through
  `serde_json::Value` so the signer and verifier agree on bytes
  regardless of struct field ordering. Tampering breaks the chain at the
  first mutated byte and `verify_ndjson_signed` returns
  `ComplianceError::ChainBroken { index, reason }`.
* **`AuditFormat::EuAiOfficeCsv`** — the columnar template the AI
  Office consumes for GPAI document requests. RFC4180-escaped; header
  row first. Columns: `event_id, timestamp, agent_id, event_type,
  model, thread_id, tokens_input, tokens_output, content_hash`.

## Key management

`AuditSigner` never generates or stores keys on its own. Operators are
expected to keep the 32-byte Ed25519 secret behind an HSM or KMS and
pass it through `AuditSigner::from_secret_bytes`. `generate_ephemeral`
exists purely for tests.

## Integration with `forget_subject`

When a DPDPA consent withdrawal triggers `forget_subject`, the emitted
`MemoryRedact` events land in the audit trail with a proper
`prev_hash` link. A later `export_audit_log` call signs the chain
end-to-end, giving regulators a single artefact that covers both the
original write and its regulated erasure.

## What's NOT in v0.3.1

* No encryption-key rotation story (deferred to v0.4.0).
* No streaming export from the REST / gRPC surfaces — today the export
  function is synchronous against an in-memory event list. For very
  long audit windows, callers should batch via
  `storage.list_events(agent_id, limit, offset)` and feed slices into
  `export_audit_log`.
* No retention-policy enforcement. The trail retains whatever the
  storage backend retains.
