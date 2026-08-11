use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Input for `mnemo.provenance` — read the write-provenance of memories.
///
/// Provide exactly one selector: `memory_id` (the provenance of one memory),
/// `principal` (everything a writer authored), or `session_id` (everything
/// written under a session / trace id).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceInput {
    /// Look up the provenance of a single memory by its ID.
    pub memory_id: Option<String>,
    /// List everything this principal (writing agent/user) wrote, newest first.
    pub principal: Option<String>,
    /// List everything written under this session / trace id, newest first.
    pub session_id: Option<String>,
    /// Max rows for the principal/session listings (default 1000, capped 10000).
    pub limit: Option<usize>,
}

/// Input for `mnemo.forget_by_provenance` — FORGET BY PROVENANCE.
///
/// Revoke every memory a principal (or session) authored, in one call. This is
/// remediation targeted at the responsible writer — not an indiscriminate wipe.
/// Provide exactly one of `principal` or `session_id`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ForgetByProvenanceInput {
    /// Revoke everything this principal wrote.
    pub principal: Option<String>,
    /// Revoke everything written under this session / trace id.
    pub session_id: Option<String>,
    /// "soft_delete" (default, recoverable), "hard_delete" (permanent), or
    /// "redact" (blank content, keep the audit hash chain).
    pub strategy: Option<String>,
}
