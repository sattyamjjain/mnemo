//! Minimal verifiable capability (lease) — the seed of #126 (capability-leased
//! access).
//!
//! A [`Capability`] binds a `principal` to a `scope` with an optional expiry and
//! is **HMAC-signed** by an issuer key, so a capability id recorded in write
//! provenance can be *verified* against the key rather than trusted as a
//! free-form string. The write path (REMEMBER / SHARE) verifies a presented
//! capability before recording it in [`crate::model::write_provenance`].
//!
//! This is deliberately small: it is the authorisation *token*, not a policy
//! engine. Enforcing what a `scope` permits (namespace / op gating) is the
//! follow-up tracked in #126. Today the token proves exactly one thing:
//! "principal `P` held a valid, unexpired, issuer-signed capability with scope
//! `S`." That is enough to make a write's authority a real, checkable fact
//! instead of a recorded label.

use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;
use sha2::Sha256;

/// A signed, time-bounded authorisation token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub id: Uuid,
    /// Who holds this capability (the writing principal).
    pub principal: String,
    /// What it authorises. Free-form for now (e.g. `"remember"`,
    /// `"share:agent-x"`, `"namespace:acme"`); scope *enforcement* is #126.
    pub scope: String,
    pub issued_at: DateTime<Utc>,
    /// `None` = no expiry.
    pub expires_at: Option<DateTime<Utc>>,
    /// HMAC-SHA256 over `id || principal || scope || issued_at || expires_at`,
    /// binding the token to the issuer key.
    pub signature: Vec<u8>,
    /// Issuer key identifier, so a rotated key can still verify old tokens.
    pub key_id: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityError {
    #[error("capability signature mismatch — forged token or wrong key")]
    SignatureMismatch,
    #[error("capability expired at {0}")]
    Expired(DateTime<Utc>),
    #[error("capability was issued by key `{0}`, which this issuer does not hold")]
    UnknownKey(String),
}

/// Issues and verifies [`Capability`] tokens against a single HMAC key.
///
/// To verify tokens issued under a rotated key, keep the old issuer around and
/// dispatch by `capability.key_id` (same pattern as
/// [`crate::provenance::ProvenanceKeystore`]).
#[derive(Debug, Clone)]
pub struct CapabilityIssuer {
    key_id: String,
    key: Vec<u8>,
}

impl CapabilityIssuer {
    pub fn new(key_id: impl Into<String>, key: &[u8]) -> Self {
        Self {
            key_id: key_id.into(),
            key: key.to_vec(),
        }
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Issue a signed capability valid for `ttl` (`None` = no expiry).
    pub fn issue(
        &self,
        principal: impl Into<String>,
        scope: impl Into<String>,
        ttl: Option<Duration>,
    ) -> Capability {
        let principal = principal.into();
        let scope = scope.into();
        let issued_at = Utc::now();
        let expires_at = ttl.map(|d| issued_at + d);
        let id = Uuid::now_v7();
        let signature = self.sign(&id, &principal, &scope, &issued_at, &expires_at);
        Capability {
            id,
            principal,
            scope,
            issued_at,
            expires_at,
            signature,
            key_id: self.key_id.clone(),
        }
    }

    fn sign(
        &self,
        id: &Uuid,
        principal: &str,
        scope: &str,
        issued_at: &DateTime<Utc>,
        expires_at: &Option<DateTime<Utc>>,
    ) -> Vec<u8> {
        // HMAC key length is unconstrained for HMAC-SHA256, so this never fails.
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(&self.key)
            .expect("HMAC-SHA256 accepts any key length");
        mac.update(id.as_bytes());
        mac.update(principal.as_bytes());
        mac.update(scope.as_bytes());
        mac.update(issued_at.to_rfc3339().as_bytes());
        if let Some(exp) = expires_at {
            mac.update(exp.to_rfc3339().as_bytes());
        }
        mac.finalize().into_bytes().to_vec()
    }

    /// Verify a capability's signature and expiry against this issuer's key.
    pub fn verify(&self, cap: &Capability) -> Result<(), CapabilityError> {
        if cap.key_id != self.key_id {
            return Err(CapabilityError::UnknownKey(cap.key_id.clone()));
        }
        let expected = self.sign(
            &cap.id,
            &cap.principal,
            &cap.scope,
            &cap.issued_at,
            &cap.expires_at,
        );
        if !bool::from(expected.ct_eq(&cap.signature)) {
            return Err(CapabilityError::SignatureMismatch);
        }
        if let Some(exp) = cap.expires_at
            && Utc::now() > exp
        {
            return Err(CapabilityError::Expired(exp));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issuer() -> CapabilityIssuer {
        CapabilityIssuer::new("mnemo-cap-test", &[9u8; 32])
    }

    #[test]
    fn issue_then_verify_round_trips() {
        let iss = issuer();
        let cap = iss.issue("alice", "remember", Some(Duration::hours(1)));
        assert_eq!(cap.principal, "alice");
        assert_eq!(cap.scope, "remember");
        iss.verify(&cap)
            .expect("freshly issued capability must verify");
    }

    #[test]
    fn no_expiry_verifies() {
        let iss = issuer();
        let cap = iss.issue("svc", "share:agent-x", None);
        assert!(cap.expires_at.is_none());
        iss.verify(&cap).unwrap();
    }

    #[test]
    fn tampered_principal_fails() {
        let iss = issuer();
        let mut cap = iss.issue("alice", "remember", None);
        cap.principal = "mallory".to_string();
        assert_eq!(iss.verify(&cap), Err(CapabilityError::SignatureMismatch));
    }

    #[test]
    fn tampered_scope_fails() {
        let iss = issuer();
        let mut cap = iss.issue("alice", "remember", None);
        cap.scope = "admin".to_string();
        assert_eq!(iss.verify(&cap), Err(CapabilityError::SignatureMismatch));
    }

    #[test]
    fn expired_capability_fails() {
        let iss = issuer();
        // Negative ttl => already expired.
        let cap = iss.issue("alice", "remember", Some(Duration::seconds(-1)));
        assert!(matches!(iss.verify(&cap), Err(CapabilityError::Expired(_))));
    }

    #[test]
    fn wrong_key_id_is_rejected() {
        let iss = issuer();
        let mut cap = iss.issue("alice", "remember", None);
        cap.key_id = "rotated-out".to_string();
        assert_eq!(
            iss.verify(&cap),
            Err(CapabilityError::UnknownKey("rotated-out".to_string()))
        );
    }

    #[test]
    fn different_key_same_id_fails_signature() {
        let a = CapabilityIssuer::new("k", &[1u8; 32]);
        let b = CapabilityIssuer::new("k", &[2u8; 32]);
        let cap = a.issue("alice", "remember", None);
        // Same key_id, different key material => signature mismatch, not UnknownKey.
        assert_eq!(b.verify(&cap), Err(CapabilityError::SignatureMismatch));
    }
}
