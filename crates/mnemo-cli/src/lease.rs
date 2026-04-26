//! v0.4.0-rc3 (Task B2) — capability-leased reads.
//!
//! Defends against the OX-MCP "exfiltrate-then-act" injection chain:
//! every `mnemo.recall` returns a per-read [`LeaseToken`] with a
//! short TTL (default 60s) and a scope set. Privileged tools
//! (`mnemo.forget_subject`, `mnemo.export_audit_log`) only run when
//! the caller presents a non-expired lease that names the right
//! scope and the right `agent_id`.
//!
//! The MCP-tools-layer wiring (mutating `mnemo.recall` to return a
//! lease and `mnemo.forget_subject` to require one) is a separate
//! follow-up that lives in the `mnemo-mcp` crate. The store itself is
//! built and exercised by `mnemo-mcp-server` so that follow-up has a
//! stable home for the runtime state without another binary change.
//! `#[allow(dead_code)]` markers document that gap precisely.

#![allow(dead_code)]

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum LeaseScope {
    ForgetSubject,
    ExportAuditLog,
}

impl LeaseScope {
    pub fn name(&self) -> &'static str {
        match self {
            LeaseScope::ForgetSubject => "forget_subject",
            LeaseScope::ExportAuditLog => "export_audit_log",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LeaseToken {
    pub id: Uuid,
    pub agent_id: String,
    pub scopes: BTreeSet<LeaseScope>,
}

#[derive(Debug, Clone)]
struct StoredLease {
    agent_id: String,
    scopes: BTreeSet<LeaseScope>,
    issued_at: Instant,
}

pub struct LeaseStore {
    inner: Mutex<HashMap<Uuid, StoredLease>>,
    ttl: Duration,
}

impl LeaseStore {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub fn issue(&self, agent_id: &str, scopes: BTreeSet<LeaseScope>) -> LeaseToken {
        let id = Uuid::now_v7();
        let stored = StoredLease {
            agent_id: agent_id.to_string(),
            scopes: scopes.clone(),
            issued_at: Instant::now(),
        };
        self.inner.lock().unwrap().insert(id, stored);
        LeaseToken {
            id,
            agent_id: agent_id.to_string(),
            scopes,
        }
    }

    pub fn check(
        &self,
        token_id: Uuid,
        expected_agent: &str,
        wanted: LeaseScope,
    ) -> Result<(), LeaseError> {
        let mut map = self.inner.lock().unwrap();
        let lease = map.get(&token_id).ok_or(LeaseError::NotFound)?;
        if lease.agent_id != expected_agent {
            return Err(LeaseError::WrongAgent);
        }
        if lease.issued_at.elapsed() > self.ttl {
            map.remove(&token_id);
            return Err(LeaseError::Expired);
        }
        if !lease.scopes.contains(&wanted) {
            return Err(LeaseError::ScopeMissing { wanted });
        }
        Ok(())
    }

    pub fn purge_expired(&self) {
        let mut map = self.inner.lock().unwrap();
        let cutoff = self.ttl;
        map.retain(|_, lease| lease.issued_at.elapsed() <= cutoff);
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LeaseError {
    #[error("lease token not found — was it tampered or replayed across servers?")]
    NotFound,
    #[error("lease bound to a different agent_id")]
    WrongAgent,
    #[error("lease expired — issue a fresh recall first")]
    Expired,
    #[error("lease does not name {wanted:?} in its scopes")]
    ScopeMissing { wanted: LeaseScope },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scopes(items: &[LeaseScope]) -> BTreeSet<LeaseScope> {
        items.iter().cloned().collect()
    }

    #[test]
    fn issued_lease_validates_for_named_scope() {
        let store = LeaseStore::new(60);
        let lease = store.issue("agent-1", scopes(&[LeaseScope::ForgetSubject]));
        store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject)
            .unwrap();
    }

    #[test]
    fn lease_for_different_scope_is_rejected() {
        let store = LeaseStore::new(60);
        let lease = store.issue("agent-1", scopes(&[LeaseScope::ForgetSubject]));
        let err = store
            .check(lease.id, "agent-1", LeaseScope::ExportAuditLog)
            .unwrap_err();
        assert!(matches!(err, LeaseError::ScopeMissing { .. }));
    }

    #[test]
    fn lease_for_wrong_agent_is_rejected() {
        let store = LeaseStore::new(60);
        let lease = store.issue("agent-1", scopes(&[LeaseScope::ForgetSubject]));
        let err = store
            .check(lease.id, "agent-2", LeaseScope::ForgetSubject)
            .unwrap_err();
        assert_eq!(err, LeaseError::WrongAgent);
    }

    #[test]
    fn unknown_token_id_is_rejected() {
        let store = LeaseStore::new(60);
        let err = store
            .check(Uuid::now_v7(), "agent-1", LeaseScope::ForgetSubject)
            .unwrap_err();
        assert_eq!(err, LeaseError::NotFound);
    }

    #[test]
    fn expired_lease_is_rejected_and_purged() {
        let store = LeaseStore::new(0);
        let lease = store.issue("agent-1", scopes(&[LeaseScope::ForgetSubject]));
        std::thread::sleep(Duration::from_millis(5));
        let err = store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject)
            .unwrap_err();
        assert_eq!(err, LeaseError::Expired);
        let err2 = store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject)
            .unwrap_err();
        assert_eq!(err2, LeaseError::NotFound);
    }
}
