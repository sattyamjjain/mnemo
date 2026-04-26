//! Compliance primitives for Mnemo.
//!
//! Two regulatory shapes are covered:
//!
//! * **DPDPA (India, enforceable 2026-11-13).** [`ConsentSource`] models an
//!   external consent-manager; [`HttpConsentManager`] is a generic HTTP
//!   binding users can point at any DPB-registered CM endpoint. A missing
//!   scope on `remember` surfaces as [`ComplianceError::ConsentDenied`].
//!
//! * **EU AI Act (enforceable 2026-08-02).** [`export_audit_log`] streams
//!   `AgentEvent` rows in one of two shapes:
//!     - [`AuditFormat::NdjsonSigned`] — one JSON event per line with a
//!       detached Ed25519 signature covering the current line and the
//!       previous line's hash (chain). Verification walks the chain and
//!       rejects tampered or reordered records.
//!     - [`AuditFormat::EuAiOfficeCsv`] — the columnar template the AI
//!       Office consumes for GPAI document requests.
//!
//! The crate is feature-gated at the workspace level behind `compliance`
//! (see `mnemo-cli`'s Cargo.toml); it can be used standalone by anyone
//! embedding `mnemo-core`.

pub mod audit;
pub mod consent;
pub mod error;
pub mod mannsetu;

pub use audit::{AuditBundle, AuditFormat, AuditSigner, export_audit_log, verify_ndjson_signed};
pub use consent::{ConsentSource, ConsentState, HttpConsentManager, Scope as ConsentScope};
pub use error::ComplianceError;
pub use mannsetu::{
    ConsentToken, ConsentTokenGuard, MANNSETU_PROD_BASE_URL, MANNSETU_SANDBOX_BASE_URL,
    MannsetuConfig, MannsetuConsentSource,
};
