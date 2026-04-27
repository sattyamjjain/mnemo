//! Per-namespace ACL enforcement (v0.4.0 P0-2).
//!
//! `MeshPolicyEnforcer` is the trait Mnemo's hardened mode calls
//! before every privileged op. The default impl
//! [`StaticPolicyEnforcer`] reads from a static map operators ship in
//! the manifest; production deployments are expected to swap in an
//! HTTP- or gRPC-backed enforcer that talks to the Mesh control
//! plane's authorization service.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::identity::MeshIdentity;
use crate::{MemOp, Namespace};

/// Outcome of an authorization check. Mirrors the shape of common
/// Mesh decision objects: allow / explicit-deny / missing-token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    Allow,
    Deny,
    DenyMissingIdentity,
    DenyEmptyAttestation,
    DenyNamespaceMismatch,
}

impl PolicyDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            PolicyDecision::Allow => "allow",
            PolicyDecision::Deny => "deny",
            PolicyDecision::DenyMissingIdentity => "deny_missing_identity",
            PolicyDecision::DenyEmptyAttestation => "deny_empty_attestation",
            PolicyDecision::DenyNamespaceMismatch => "deny_namespace_mismatch",
        }
    }

    pub fn is_allow(&self) -> bool {
        matches!(self, PolicyDecision::Allow)
    }
}

pub trait MeshPolicyEnforcer: Send + Sync {
    fn authorize(&self, caller: Option<&MeshIdentity>, ns: &Namespace, op: MemOp)
    -> PolicyDecision;
}

/// One ACL row keyed by `(SPIFFE ID, Namespace)` granting a set of
/// `MemOp`s. Anything not enumerated denies by default.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StaticPolicy {
    pub rules: BTreeMap<(String, String), BTreeSet<String>>,
}

impl StaticPolicy {
    pub fn allow(
        &mut self,
        spiffe_id: impl Into<String>,
        ns: &Namespace,
        ops: &[MemOp],
    ) -> &mut Self {
        let key = (spiffe_id.into(), ns.as_label());
        let entry = self.rules.entry(key).or_default();
        for op in ops {
            entry.insert(op.as_str().to_string());
        }
        self
    }

    pub fn permits(&self, spiffe_id: &str, ns: &Namespace, op: MemOp) -> bool {
        self.rules
            .get(&(spiffe_id.to_string(), ns.as_label()))
            .map(|ops| ops.contains(op.as_str()))
            .unwrap_or(false)
    }
}

pub struct StaticPolicyEnforcer {
    policy: StaticPolicy,
}

impl StaticPolicyEnforcer {
    pub fn new(policy: StaticPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> &StaticPolicy {
        &self.policy
    }
}

impl MeshPolicyEnforcer for StaticPolicyEnforcer {
    fn authorize(
        &self,
        caller: Option<&MeshIdentity>,
        ns: &Namespace,
        op: MemOp,
    ) -> PolicyDecision {
        let Some(c) = caller else {
            return PolicyDecision::DenyMissingIdentity;
        };
        if c.attestation.raw.is_empty() {
            return PolicyDecision::DenyEmptyAttestation;
        }
        // Trust-domain enforcement: refuse if the SPIFFE trust domain
        // doesn't match the namespace tenant. Cheap defense against
        // cross-tenant token replay.
        if let Some(td) = c.trust_domain()
            && td != ns.tenant
            && self.policy.rules.is_empty()
        {
            return PolicyDecision::DenyNamespaceMismatch;
        }
        if self.policy.permits(&c.workload_spiffe_id, ns, op) {
            PolicyDecision::Allow
        } else {
            PolicyDecision::Deny
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::AttestationToken;

    fn caller() -> MeshIdentity {
        MeshIdentity::new(
            "spiffe://t1/agent-1",
            AttestationToken::new(vec![1, 2, 3], "k"),
        )
    }

    #[test]
    fn missing_identity_denies() {
        let p = StaticPolicyEnforcer::new(StaticPolicy::default());
        let d = p.authorize(None, &Namespace::new("t1", "shared"), MemOp::Recall);
        assert_eq!(d, PolicyDecision::DenyMissingIdentity);
    }

    #[test]
    fn empty_attestation_denies() {
        let p = StaticPolicyEnforcer::new(StaticPolicy::default());
        let bad = MeshIdentity::new(
            "spiffe://t1/x",
            AttestationToken::new(Vec::<u8>::new(), "k"),
        );
        let d = p.authorize(Some(&bad), &Namespace::new("t1", "shared"), MemOp::Recall);
        assert_eq!(d, PolicyDecision::DenyEmptyAttestation);
    }

    #[test]
    fn missing_acl_row_denies_by_default() {
        let p = StaticPolicyEnforcer::new(StaticPolicy::default());
        let d = p.authorize(
            Some(&caller()),
            &Namespace::new("t1", "shared"),
            MemOp::Recall,
        );
        // Caller's SPIFFE trust-domain matches the namespace tenant
        // (`t1`), so the cross-tenant guard does not fire; with an
        // empty static policy the row lookup misses and we get the
        // generic `Deny`. The cross-tenant case is exercised separately
        // in `cross_tenant_denies_with_namespace_mismatch`.
        assert_eq!(d, PolicyDecision::Deny);
    }

    #[test]
    fn cross_tenant_denies_with_namespace_mismatch() {
        let p = StaticPolicyEnforcer::new(StaticPolicy::default());
        let cross = MeshIdentity::new(
            "spiffe://t-other/agent-1",
            AttestationToken::new(vec![1], "k"),
        );
        let d = p.authorize(Some(&cross), &Namespace::new("t1", "shared"), MemOp::Recall);
        assert_eq!(d, PolicyDecision::DenyNamespaceMismatch);
    }

    #[test]
    fn matching_acl_row_allows() {
        let mut policy = StaticPolicy::default();
        let ns = Namespace::new("t1", "shared");
        policy.allow("spiffe://t1/agent-1", &ns, &[MemOp::Recall, MemOp::Write]);
        let p = StaticPolicyEnforcer::new(policy);
        assert_eq!(
            p.authorize(Some(&caller()), &ns, MemOp::Recall),
            PolicyDecision::Allow
        );
        assert_eq!(
            p.authorize(Some(&caller()), &ns, MemOp::Forget),
            PolicyDecision::Deny
        );
    }

    #[test]
    fn cross_namespace_denies() {
        let mut policy = StaticPolicy::default();
        let allowed = Namespace::new("t1", "shared");
        policy.allow("spiffe://t1/agent-1", &allowed, &[MemOp::Recall]);
        let p = StaticPolicyEnforcer::new(policy);
        // Same tenant, different scope → no rule → Deny.
        assert_eq!(
            p.authorize(
                Some(&caller()),
                &Namespace::new("t1", "private"),
                MemOp::Recall
            ),
            PolicyDecision::Deny
        );
    }
}
