# DPDPA — Digital Personal Data Protection Act (India)

Enforceable from **13 November 2026**, the DPDPA requires Indian data
fiduciaries to consult a DPB-registered Consent Manager before
processing personal data and to honour consent withdrawal as an erasure
right.

Mnemo's `mnemo-compliance` crate provides two primitives to help with
this; both are behind the `compliance` feature flag so v0.1.1 callers
stay compiling.

## `ConsentSource` trait

`crates/mnemo-compliance/src/consent.rs`. A pluggable interface that
looks up the current consent state for a subject.

```rust
use mnemo_compliance::{ConsentSource, ConsentState, HttpConsentManager};

let cm = HttpConsentManager::new("https://consent.example.com/v1")
    .with_bearer(std::env::var("CONSENT_TOKEN")?);
let state: ConsentState = cm.fetch_consent("user-42").await?;
if state.has_scope("remember") && state.is_active() {
    // proceed with writing personal data
}
```

### Available implementations

* **`HttpConsentManager`** — generic HTTP binding. Expects
  `GET {base_url}/consent/{subject_id}` to return a body matching
  [`ConsentState`]. Optional bearer-token auth.
* **`StaticConsentSource`** — in-memory map, for tests and single-tenant
  self-hosting.

### `ConsentState` shape

```rust
pub struct ConsentState {
    pub subject_id: String,
    pub scopes: Vec<String>,          // granted purposes
    pub expires_at: Option<String>,   // optional wall-clock expiry
    pub token_hash: String,           // SHA-256 of the signed token
}
```

Missing scopes are treated as denied. Expired states are rejected by
`is_active()` and by `HttpConsentManager::fetch_consent`.

## Integration point: write-path consent check

Operators should call `fetch_consent` before every `engine.remember`
that touches personal data, and map a missing scope to
`ComplianceError::ConsentDenied { subject_id, scope }`. A reference
middleware is sketched in the `compliance` feature's docs but not yet
wired into the core `remember` pipeline — doing so is part of the
v0.3.2 roadmap (requires a `PolicyHook` surface on `MnemoEngine`).

## Consent withdrawal

Wire the consent manager's withdrawal webhook to
[`engine.forget_subject(subject_id, ForgetStrategy::Redact)`] (which
ships since v0.2.0). `Redact` preserves `content_hash` + `prev_hash` so
the audit trail stays verifiable even after the content is erased;
alternatively use `HardDelete` if you have no retention obligation.

## Audit trail

Every `forget_subject` emits a `MemoryRedact` audit event with a
hash-chain link to the prior event. Combine with the
[EU AI Act export surface](eu-ai-act.md) for a single signed trail.
