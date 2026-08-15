//! Capability-leased reads — [#126](https://github.com/sattyamjjain/mnemo/issues/126)
//! / [ADR 0001](../../../docs/adr/0001-capability-leased-reads.md).
//!
//! # The threat
//!
//! The OX-MCP "exfiltrate-then-act" chain: an injected instruction makes an
//! agent *read* something, then *act* on it destructively. Each step looks
//! ordinary in isolation. A lease binds the second step to the first —
//! `mnemo.forget_subject` will not run unless the caller can show it performed
//! a recall, recently, as itself, with the right scope.
//!
//! # Why this is now buildable
//!
//! #126 deferred this until "a multi-caller, authenticated transport ... where
//! distinct callers hold distinct identities", because on a single-caller stdio
//! transport a lease keyed on `agent_id` is "pure ceremony": with one possible
//! caller, binding an act to that caller proves nothing.
//!
//! Per-request identity (ADR 0002) is what changed. A lease is now bound to the
//! **capability-verified principal of the request that minted it**, so replaying
//! it as a different principal fails — including on stdio, where a gateway may
//! multiplex several agents over one pipe.
//!
//! # Two deliberate departures from the removed implementation
//!
//! 1. **No `ExportAuditLog` scope.** The original named two privileged tools,
//!    but `export_audit_log` is not an MCP tool — it is the
//!    `mnemo_compliance::export_audit_log` library function. A scope gating a
//!    tool that does not exist is exactly the claimed-but-not-wired defect this
//!    repo has repaired four times. It goes in when the tool does.
//! 2. **Opt-in.** The store is attached or it is not. Unattached, `recall` and
//!    `forget_subject` behave exactly as they always have. Attached,
//!    `forget_subject` *requires* a valid lease. Enforcing unconditionally
//!    would break every shipped client of a docs-drift-tested tool on upgrade,
//!    which is the operator's call and not this module's.

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

/// What a lease authorises. One variant, deliberately — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum LeaseScope {
    ForgetSubject,
}

impl LeaseScope {
    pub fn name(&self) -> &'static str {
        match self {
            LeaseScope::ForgetSubject => "forget_subject",
        }
    }
}

/// A minted lease, as handed back to the caller on the read that created it.
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

/// In-memory lease store with a TTL.
///
/// Process-local by design: a lease is meant to bind two calls in one agent's
/// working session, and a lease that survived a restart would be a durable
/// grant — the long-lived-credential shape ADR 0002 rejected.
pub struct LeaseStore {
    inner: Mutex<HashMap<Uuid, StoredLease>>,
    ttl: Duration,
}

impl LeaseStore {
    /// `ttl_seconds` bounds how long a read stays actionable. Short is the
    /// point: the lease exists to prove the act follows *this* read, and a long
    /// TTL degrades it toward an ambient permission.
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub fn ttl_seconds(&self) -> u64 {
        self.ttl.as_secs()
    }

    /// Mint a lease for `agent_id` — the caller's per-request resolved
    /// principal, never a boot-time constant.
    pub fn issue(&self, agent_id: &str, scopes: BTreeSet<LeaseScope>) -> LeaseToken {
        let id = Uuid::now_v7();
        let stored = StoredLease {
            agent_id: agent_id.to_string(),
            scopes: scopes.clone(),
            issued_at: Instant::now(),
        };
        self.inner
            .lock()
            .expect("lease store mutex poisoned")
            .insert(id, stored);
        LeaseToken {
            id,
            agent_id: agent_id.to_string(),
            scopes,
        }
    }

    /// Validate a presented lease for `expected_agent` and `wanted` scope.
    ///
    /// Checks agent binding, then expiry, then scope. An expired lease is
    /// evicted on the way out so a replay cannot keep it warm in the map.
    pub fn check(
        &self,
        token_id: Uuid,
        expected_agent: &str,
        wanted: LeaseScope,
    ) -> Result<(), LeaseError> {
        let mut map = self.inner.lock().expect("lease store mutex poisoned");
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

    /// Drop expired leases. Bounds memory on a long-lived server; `check`
    /// already refuses expired leases, so this is hygiene, not enforcement.
    pub fn purge_expired(&self) {
        let mut map = self.inner.lock().expect("lease store mutex poisoned");
        let cutoff = self.ttl;
        map.retain(|_, lease| lease.issued_at.elapsed() <= cutoff);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LeaseError {
    #[error(
        "lease token not found — it was never issued by this server, has already expired, or \
         belongs to a different process. Call mnemo.recall first and present the lease it returns."
    )]
    NotFound,
    #[error(
        "lease is bound to a different caller. A lease proves THIS caller performed the read; \
         presenting another caller's lease is the replay it exists to stop."
    )]
    WrongAgent,
    #[error("lease expired — call mnemo.recall again for a fresh one")]
    Expired,
    #[error("lease does not name `{}` in its scopes", wanted.name())]
    ScopeMissing { wanted: LeaseScope },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scopes(items: &[LeaseScope]) -> BTreeSet<LeaseScope> {
        items.iter().copied().collect()
    }

    #[test]
    fn issued_lease_validates_for_its_scope_and_agent() {
        let store = LeaseStore::new(60);
        let lease = store.issue("agent-1", scopes(&[LeaseScope::ForgetSubject]));
        store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject)
            .expect("a fresh, correctly-scoped, correctly-bound lease is valid");
    }

    #[test]
    fn lease_for_wrong_agent_is_rejected() {
        // The replay case the whole mechanism exists for: agent-2 presenting a
        // lease agent-1 earned.
        let store = LeaseStore::new(60);
        let lease = store.issue("agent-1", scopes(&[LeaseScope::ForgetSubject]));
        let err = store
            .check(lease.id, "agent-2", LeaseScope::ForgetSubject)
            .expect_err("another caller's lease must not authorise this caller");
        assert_eq!(err, LeaseError::WrongAgent);
    }

    #[test]
    fn expired_lease_is_rejected_and_evicted() {
        let store = LeaseStore::new(0); // everything is instantly stale
        let lease = store.issue("agent-1", scopes(&[LeaseScope::ForgetSubject]));
        std::thread::sleep(Duration::from_millis(10));
        let err = store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject)
            .expect_err("an expired lease must not authorise anything");
        assert_eq!(err, LeaseError::Expired);
        assert_eq!(
            store.len(),
            0,
            "an expired lease should not linger in the map"
        );
    }

    #[test]
    fn lease_without_the_wanted_scope_is_rejected() {
        let store = LeaseStore::new(60);
        let lease = store.issue("agent-1", scopes(&[]));
        let err = store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject)
            .expect_err("a lease that does not name the scope must not authorise it");
        assert_eq!(
            err,
            LeaseError::ScopeMissing {
                wanted: LeaseScope::ForgetSubject
            }
        );
    }

    #[test]
    fn unknown_token_is_rejected() {
        let store = LeaseStore::new(60);
        let err = store
            .check(Uuid::now_v7(), "agent-1", LeaseScope::ForgetSubject)
            .expect_err("an invented token id must not authorise anything");
        assert_eq!(err, LeaseError::NotFound);
    }

    #[test]
    fn purge_removes_only_expired_leases() {
        let store = LeaseStore::new(60);
        let keep = store.issue("agent-1", scopes(&[LeaseScope::ForgetSubject]));
        store.purge_expired();
        assert_eq!(store.len(), 1);
        store
            .check(keep.id, "agent-1", LeaseScope::ForgetSubject)
            .expect("a live lease survives a purge");
    }

    #[test]
    fn a_lease_is_single_agent_even_with_identical_scopes() {
        // Two callers each doing a read get distinct, non-interchangeable
        // leases. This is what per-request identity (ADR 0002) buys: before it,
        // both would have been minted for the same boot agent id and would have
        // validated for each other.
        let store = LeaseStore::new(60);
        let alice = store.issue("alice", scopes(&[LeaseScope::ForgetSubject]));
        let bob = store.issue("bob", scopes(&[LeaseScope::ForgetSubject]));
        assert_ne!(alice.id, bob.id);
        assert!(
            store
                .check(alice.id, "bob", LeaseScope::ForgetSubject)
                .is_err()
        );
        assert!(
            store
                .check(bob.id, "alice", LeaseScope::ForgetSubject)
                .is_err()
        );
    }
}
