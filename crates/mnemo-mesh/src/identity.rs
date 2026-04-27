//! SPIFFE-style identity types (v0.4.0 P0-2).
//!
//! Mesh issues each workload a SPIFFE-compatible ID
//! (`spiffe://<trust-domain>/<workload-path>`) plus an attestation
//! token signed by the Mesh control plane. Mnemo doesn't validate
//! the token cryptographically yet — that lives in a future
//! `MeshTokenValidator` trait wired to whatever signing scheme the
//! operator's Mesh trust bundle uses. For now we expose the data
//! shape so downstream policy code is forwards-compatible.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One Mesh-attested workload. Carries the SPIFFE ID + the raw
/// attestation token bytes. Validation is the policy enforcer's job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshIdentity {
    pub workload_spiffe_id: String,
    pub attestation: AttestationToken,
}

impl MeshIdentity {
    pub fn new(spiffe_id: impl Into<String>, token: AttestationToken) -> Self {
        Self {
            workload_spiffe_id: spiffe_id.into(),
            attestation: token,
        }
    }

    /// Extract `(trust_domain, workload_path)` from the SPIFFE ID.
    /// Returns `None` for malformed ids.
    pub fn split_spiffe(&self) -> Option<(&str, &str)> {
        let rest = self.workload_spiffe_id.strip_prefix("spiffe://")?;
        let slash = rest.find('/')?;
        Some((&rest[..slash], &rest[slash + 1..]))
    }

    pub fn trust_domain(&self) -> Option<&str> {
        self.split_spiffe().map(|(td, _)| td)
    }
}

/// Opaque token bytes the Mesh control plane signs. We hold the
/// envelope but defer signature validation to a follow-up validator
/// trait. The token is included in the audit envelope so an offline
/// auditor can re-verify against the historical trust bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationToken {
    pub raw: Vec<u8>,
    /// Operator-defined kid pointing at which Mesh trust-bundle key
    /// signed `raw`.
    pub kid: String,
}

impl AttestationToken {
    pub fn new(raw: impl Into<Vec<u8>>, kid: impl Into<String>) -> Self {
        Self {
            raw: raw.into(),
            kid: kid.into(),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum IdentityError {
    #[error("malformed SPIFFE ID: {0:?}")]
    MalformedSpiffe(String),
    #[error("attestation token is empty — refuse to authorize against an empty token")]
    EmptyToken,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_spiffe_extracts_components() {
        let id = MeshIdentity::new(
            "spiffe://prod.mnemo.io/agent/runner-42",
            AttestationToken::new(vec![1, 2, 3], "k1"),
        );
        assert_eq!(
            id.split_spiffe(),
            Some(("prod.mnemo.io", "agent/runner-42"))
        );
        assert_eq!(id.trust_domain(), Some("prod.mnemo.io"));
    }

    #[test]
    fn malformed_spiffe_returns_none() {
        let id = MeshIdentity::new("not-a-spiffe-id", AttestationToken::new(vec![1], "k"));
        assert!(id.split_spiffe().is_none());
        assert!(id.trust_domain().is_none());
    }

    #[test]
    fn malformed_no_workload_path_returns_none() {
        let id = MeshIdentity::new(
            "spiffe://only-trust-domain",
            AttestationToken::new(vec![1], "k"),
        );
        assert!(id.split_spiffe().is_none());
    }
}
