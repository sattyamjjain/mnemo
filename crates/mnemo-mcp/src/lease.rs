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
//! # What a lease constrains
//!
//! All four of ADR 0001's properties are enforced:
//!
//! | property | mechanism |
//! |---|---|
//! | **freshness** | the lease expires (`ttl_seconds`) |
//! | **causality** | only a `recall` mints one |
//! | **caller-binding** | it validates for the minting principal alone |
//! | **subject scope** | it names the subjects the read actually covered ([#160]) |
//!
//! # Subject scope — how the set is derived, and why not from the query
//!
//! [#160] was filed saying this was blocked: a recall is a *query*, possibly
//! semantic, while `forget_subject` erases by a `subject:<id>` tag, and
//! inferring "which subjects did this query cover" from a ranked result set
//! either over-narrows or over-broadens. Both readings are correct about the
//! *query*, and both stop one step early — because the **returned records** are
//! not an inference. They are what the caller was handed, and they carry their
//! `subject:` tags with them.
//!
//! So the set is read off the result, not guessed from the request: a lease
//! covers subject `S` iff a record tagged `subject:S` was in the recall response.
//! Nothing is derived, so neither failure mode applies.
//!
//! The issue proposed instead that the caller *declare* a subject set at recall
//! time — but a scope the caller nominates is a scope the caller chose, which
//! answers its own question, and it costs a breaking change to `mnemo.recall`.
//! The result-derived set is both stricter and free.
//!
//! **A recall that returned no subject-tagged records mints a lease that
//! authorises no erasure.** That is fail-closed and deliberate: reading nothing
//! about any subject is not grounds to erase one. The error says so explicitly,
//! because a silent empty set would be indistinguishable from a bug.
//!
//! # Three deliberate departures from the removed implementation
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
//! 3. **Subject scope narrows to what was read, not to what was asked for.**
//!    See above.
//!
//! [#160]: https://github.com/sattyamjjain/mnemo/issues/160

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
    /// Subject ids the minting read actually covered — the `subject:` tags
    /// carried by the records it returned, with the prefix stripped ([#160]).
    ///
    /// An empty set authorises no erasure. See the module docs.
    ///
    /// [#160]: https://github.com/sattyamjjain/mnemo/issues/160
    pub subjects: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct StoredLease {
    agent_id: String,
    scopes: BTreeSet<LeaseScope>,
    subjects: BTreeSet<String>,
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
    ///
    /// `subjects` is the set the read actually covered (see the module docs);
    /// pass it empty and the lease authorises no erasure.
    pub fn issue(
        &self,
        agent_id: &str,
        scopes: BTreeSet<LeaseScope>,
        subjects: BTreeSet<String>,
    ) -> LeaseToken {
        let id = Uuid::now_v7();
        let stored = StoredLease {
            agent_id: agent_id.to_string(),
            scopes: scopes.clone(),
            subjects: subjects.clone(),
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
            subjects,
        }
    }

    /// Validate a presented lease for `expected_agent`, `wanted` scope, and
    /// `subject`.
    ///
    /// Checks agent binding, then expiry, then scope, then subject coverage. An
    /// expired lease is evicted on the way out so a replay cannot keep it warm
    /// in the map.
    ///
    /// The subject check is what stops a lease earned by a narrow read from
    /// authorising an erasure of some *other* subject within the TTL ([#160]).
    /// It is ordered last so the coarser refusals report first — an expired
    /// lease should say "expired", not "wrong subject".
    ///
    /// [#160]: https://github.com/sattyamjjain/mnemo/issues/160
    pub fn check(
        &self,
        token_id: Uuid,
        expected_agent: &str,
        wanted: LeaseScope,
        subject: &str,
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
        if !lease.subjects.contains(subject) {
            return Err(LeaseError::SubjectNotCovered {
                subject: subject.to_string(),
                covered: lease.subjects.iter().cloned().collect(),
            });
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
    #[error(
        "lease does not cover subject `{subject}` — it was earned by a read that returned \
         records for {}. A lease authorises erasure only of the subjects the read it came from \
         actually covered, so a narrow read cannot be spent on a wider delete. Recall subject \
         `{subject}` first (e.g. with tag `subject:{subject}`) and present the lease that read \
         returns.",
        if covered.is_empty() {
            "no subject at all".to_string()
        } else {
            format!("subject(s) {}", covered.join(", "))
        }
    )]
    SubjectNotCovered {
        subject: String,
        covered: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scopes(items: &[LeaseScope]) -> BTreeSet<LeaseScope> {
        items.iter().copied().collect()
    }

    fn subjects(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// A lease over subject `alice`, the shape a normal narrow read produces.
    fn alice_lease(store: &LeaseStore, agent: &str) -> LeaseToken {
        store.issue(
            agent,
            scopes(&[LeaseScope::ForgetSubject]),
            subjects(&["alice"]),
        )
    }

    #[test]
    fn issued_lease_validates_for_its_scope_agent_and_subject() {
        let store = LeaseStore::new(60);
        let lease = alice_lease(&store, "agent-1");
        store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject, "alice")
            .expect("a fresh, correctly-scoped, correctly-bound, in-subject lease is valid");
    }

    #[test]
    fn lease_for_wrong_agent_is_rejected() {
        // The replay case the whole mechanism exists for: agent-2 presenting a
        // lease agent-1 earned.
        let store = LeaseStore::new(60);
        let lease = alice_lease(&store, "agent-1");
        let err = store
            .check(lease.id, "agent-2", LeaseScope::ForgetSubject, "alice")
            .expect_err("another caller's lease must not authorise this caller");
        assert_eq!(err, LeaseError::WrongAgent);
    }

    #[test]
    fn expired_lease_is_rejected_and_evicted() {
        let store = LeaseStore::new(0); // everything is instantly stale
        let lease = alice_lease(&store, "agent-1");
        std::thread::sleep(Duration::from_millis(10));
        let err = store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject, "alice")
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
        let lease = store.issue("agent-1", scopes(&[]), subjects(&["alice"]));
        let err = store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject, "alice")
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
            .check(
                Uuid::now_v7(),
                "agent-1",
                LeaseScope::ForgetSubject,
                "alice",
            )
            .expect_err("an invented token id must not authorise anything");
        assert_eq!(err, LeaseError::NotFound);
    }

    #[test]
    fn purge_removes_only_expired_leases() {
        let store = LeaseStore::new(60);
        let keep = alice_lease(&store, "agent-1");
        store.purge_expired();
        assert_eq!(store.len(), 1);
        store
            .check(keep.id, "agent-1", LeaseScope::ForgetSubject, "alice")
            .expect("a live lease survives a purge");
    }

    #[test]
    fn a_lease_is_single_agent_even_with_identical_scopes() {
        // Two callers each doing a read get distinct, non-interchangeable
        // leases. This is what per-request identity (ADR 0002) buys: before it,
        // both would have been minted for the same boot agent id and would have
        // validated for each other.
        let store = LeaseStore::new(60);
        let alice = alice_lease(&store, "alice");
        let bob = alice_lease(&store, "bob");
        assert_ne!(alice.id, bob.id);
        assert!(
            store
                .check(alice.id, "bob", LeaseScope::ForgetSubject, "alice")
                .is_err()
        );
        assert!(
            store
                .check(bob.id, "alice", LeaseScope::ForgetSubject, "alice")
                .is_err()
        );
    }

    // --- #160: subject-scope narrowing -----------------------------------

    #[test]
    fn a_narrow_read_cannot_be_spent_on_a_different_subject() {
        // THE #160 CASE. Before narrowing, this succeeded: a lease earned by
        // reading `alice` authorised erasing `bob`, by the same caller, inside
        // the TTL. Freshness, causality and caller-binding all held and the
        // erasure was still wrong.
        let store = LeaseStore::new(60);
        let lease = alice_lease(&store, "agent-1");
        let err = store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject, "bob")
            .expect_err("a lease earned reading `alice` must not authorise erasing `bob`");
        assert_eq!(
            err,
            LeaseError::SubjectNotCovered {
                subject: "bob".to_string(),
                covered: vec!["alice".to_string()],
            }
        );
        // ...and the lease it WAS earned for still works, so the narrowing did
        // not simply break erasure.
        store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject, "alice")
            .expect("the subject the read actually covered is still authorised");
    }

    #[test]
    fn a_read_covering_no_subject_authorises_no_erasure() {
        // Fail-closed. A recall that returned nothing subject-tagged mints a
        // lease with an empty set; it must not degrade into a blanket grant,
        // which is the failure mode an "empty means unrestricted" reading would
        // produce.
        let store = LeaseStore::new(60);
        let lease = store.issue(
            "agent-1",
            scopes(&[LeaseScope::ForgetSubject]),
            subjects(&[]),
        );
        assert!(lease.subjects.is_empty());
        let err = store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject, "alice")
            .expect_err("an empty subject set must authorise nothing, not everything");
        assert_eq!(
            err,
            LeaseError::SubjectNotCovered {
                subject: "alice".to_string(),
                covered: vec![],
            }
        );
        // The refusal has to be diagnosable — an operator seeing this needs to
        // know the read covered nothing, not guess at a scope typo.
        assert!(
            err.to_string().contains("no subject at all"),
            "empty-coverage refusal must say so explicitly, got: {err}"
        );
    }

    #[test]
    fn a_read_covering_several_subjects_authorises_each_of_them() {
        // A wide read legitimately earns a wide lease. The narrowing binds the
        // erasure to what was READ, not to one subject per lease.
        let store = LeaseStore::new(60);
        let lease = store.issue(
            "agent-1",
            scopes(&[LeaseScope::ForgetSubject]),
            subjects(&["alice", "bob"]),
        );
        for s in ["alice", "bob"] {
            store
                .check(lease.id, "agent-1", LeaseScope::ForgetSubject, s)
                .unwrap_or_else(|e| panic!("subject `{s}` was covered by the read: {e}"));
        }
        assert!(
            store
                .check(lease.id, "agent-1", LeaseScope::ForgetSubject, "carol")
                .is_err(),
            "a subject outside the read must still be refused"
        );
    }

    #[test]
    fn subject_check_is_ordered_after_the_coarser_refusals() {
        // An expired lease presented for an uncovered subject must report
        // Expired. Reporting SubjectNotCovered would send the operator to fix
        // the wrong thing, and would leak which subjects a stale lease covered.
        let store = LeaseStore::new(0);
        let lease = alice_lease(&store, "agent-1");
        std::thread::sleep(Duration::from_millis(10));
        let err = store
            .check(lease.id, "agent-1", LeaseScope::ForgetSubject, "bob")
            .expect_err("expired");
        assert_eq!(err, LeaseError::Expired);

        // Same for the wrong-agent case: identity failure outranks scope.
        let store = LeaseStore::new(60);
        let lease = alice_lease(&store, "agent-1");
        let err = store
            .check(lease.id, "agent-2", LeaseScope::ForgetSubject, "bob")
            .expect_err("wrong agent");
        assert_eq!(err, LeaseError::WrongAgent);
    }
}
