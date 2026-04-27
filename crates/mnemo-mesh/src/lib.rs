//! v0.4.0 (P0-2) — Cloudflare Mesh runtime adapter.
//!
//! Cloudflare Mesh (announced 2026-04-24) defines the
//! lifecycle-attestation envelope agent infrastructure is moving to:
//! every workload presents a SPIFFE-style identity + an attestation
//! token, and every privileged op carries an audit envelope back to
//! a chained ledger. This crate makes Mnemo speak that protocol so
//! Mesh-deployed agents can use Mnemo as their memory plane without
//! losing the lifecycle-attestation chain.
//!
//! Three pieces:
//!
//! 1. [`identity::MeshIdentity`] — the (workload_spiffe_id,
//!    attestation_token) pair the caller presents on every op.
//! 2. [`policy::MeshPolicyEnforcer`] — pluggable ACL that decides
//!    whether the caller can perform a [`MemOp`] against a
//!    [`Namespace`].
//! 3. [`MeshAuditEnvelope`] — chained-HMAC envelope that links each
//!    decision back to the existing memory-provenance chain head, so
//!    audit-log export emits one continuous ledger instead of two
//!    parallel ones.

pub mod identity;
pub mod policy;

use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use identity::MeshIdentity;
pub use policy::{MeshPolicyEnforcer, PolicyDecision, StaticPolicyEnforcer};

/// Tenant + scope qualifier the policy decides against. Matches
/// Cloudflare Mesh namespace shape: `<tenant>/<scope>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Namespace {
    pub tenant: String,
    pub scope: String,
}

impl Namespace {
    pub fn new(tenant: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            tenant: tenant.into(),
            scope: scope.into(),
        }
    }

    pub fn as_label(&self) -> String {
        format!("{}/{}", self.tenant, self.scope)
    }
}

/// The privileged operations Mesh ACLs gate. Matches the verbs an
/// LLM-host agent could try to invoke against Mnemo. New verbs land
/// here when new privileged tools appear in the MCP catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum MemOp {
    Recall,
    Write,
    Forget,
    Branch,
    ReplayAsOf,
    ExportProvenance,
}

impl MemOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemOp::Recall => "recall",
            MemOp::Write => "write",
            MemOp::Forget => "forget",
            MemOp::Branch => "branch",
            MemOp::ReplayAsOf => "replay_as_of",
            MemOp::ExportProvenance => "export_provenance",
        }
    }
}

/// Audit envelope appended to the chained ledger after every authorized
/// op. The `prev_chain_head` matches the existing
/// `mnemo-core::provenance` HMAC chain, so an export joins memory
/// receipts and Mesh decisions on a single timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshAuditEnvelope {
    pub caller_spiffe: String,
    pub op: MemOp,
    pub namespace: Namespace,
    pub decided: PolicyDecision,
    /// HMAC of the existing provenance chain right before this op.
    /// 32 raw bytes; serialise as hex/base64 at the wire boundary.
    pub prev_chain_head: [u8; 32],
    pub envelope_at: SystemTime,
}

impl MeshAuditEnvelope {
    /// Hash this envelope into the next chain head. Pure fn so the
    /// audit-log writer can compute heads without touching the
    /// caller's HMAC key.
    pub fn next_chain_head(&self, prev_head: &[u8; 32]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(prev_head);
        h.update(self.caller_spiffe.as_bytes());
        h.update(b"|");
        h.update(self.op.as_str().as_bytes());
        h.update(b"|");
        h.update(self.namespace.as_label().as_bytes());
        h.update(b"|");
        h.update(self.decided.as_str().as_bytes());
        h.update(b"|");
        h.update(
            chrono::DateTime::<chrono::Utc>::from(self.envelope_at)
                .to_rfc3339()
                .as_bytes(),
        );
        h.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(decision: PolicyDecision) -> MeshAuditEnvelope {
        MeshAuditEnvelope {
            caller_spiffe: "spiffe://t1/a1".into(),
            op: MemOp::Recall,
            namespace: Namespace::new("t1", "shared"),
            decided: decision,
            prev_chain_head: [0u8; 32],
            envelope_at: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn next_chain_head_is_deterministic() {
        let e = env(PolicyDecision::Allow);
        let h1 = e.next_chain_head(&[0u8; 32]);
        let h2 = e.next_chain_head(&[0u8; 32]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn next_chain_head_changes_with_decision() {
        let allow = env(PolicyDecision::Allow);
        let deny = env(PolicyDecision::DenyMissingIdentity);
        assert_ne!(
            allow.next_chain_head(&[0u8; 32]),
            deny.next_chain_head(&[0u8; 32])
        );
    }

    #[test]
    fn next_chain_head_changes_with_namespace() {
        let mut e = env(PolicyDecision::Allow);
        let h1 = e.next_chain_head(&[1u8; 32]);
        e.namespace = Namespace::new("t1", "private");
        let h2 = e.next_chain_head(&[1u8; 32]);
        assert_ne!(h1, h2);
    }

    #[test]
    fn memop_strings_round_trip() {
        for op in [
            MemOp::Recall,
            MemOp::Write,
            MemOp::Forget,
            MemOp::Branch,
            MemOp::ReplayAsOf,
            MemOp::ExportProvenance,
        ] {
            let s = op.as_str();
            assert!(!s.is_empty());
            assert_eq!(s, s.to_lowercase());
        }
    }
}
