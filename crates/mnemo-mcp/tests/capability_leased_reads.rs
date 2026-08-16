//! Capability-leased reads at the server level —
//! [#126](https://github.com/sattyamjjain/mnemo/issues/126) acceptance.
//!
//! `lease.rs`'s unit tests cover the store's rules. This covers the wiring and
//! the property the feature exists for: a `mnemo.forget_subject` cannot run
//! unless the *same principal* performed a recent `mnemo.recall`. That is the
//! break in the OX-MCP exfiltrate-then-act chain.
//!
//! #126's acceptance list is "expired / wrong-scope / wrong-agent all
//! rejected", and each has a test below. The wrong-agent case is the one that
//! only became meaningful with per-request identity (ADR 0002): before it,
//! every caller resolved to the same boot agent id, so every lease validated
//! for every caller and this test could not have failed.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use mnemo_mcp::lease::{LeaseError, LeaseScope, LeaseStore};

fn forget_scope() -> BTreeSet<LeaseScope> {
    std::iter::once(LeaseScope::ForgetSubject).collect()
}

/// The subject set a normal narrow read produces (#160). Every lease below is
/// issued over `alice` and spent on `alice`, so these tests keep measuring the
/// property they were written for (freshness / causality / caller-binding)
/// rather than accidentally measuring the new subject gate.
fn subject_alice() -> BTreeSet<String> {
    std::iter::once("alice".to_string()).collect()
}

#[test]
fn a_read_by_one_caller_does_not_authorise_an_erasure_by_another() {
    // The attack this feature exists to stop, in one assertion. Alice recalls;
    // Mallory tries to spend Alice's lease.
    let store = LeaseStore::new(60);
    let alices_lease = store.issue("alice", forget_scope(), subject_alice());

    assert_eq!(
        store
            .check(
                alices_lease.id,
                "mallory",
                LeaseScope::ForgetSubject,
                "alice"
            )
            .expect_err("a lease is not bearer authority — it is bound to who earned it"),
        LeaseError::WrongAgent
    );
    store
        .check(alices_lease.id, "alice", LeaseScope::ForgetSubject, "alice")
        .expect("alice can still spend her own lease");
}

#[test]
fn an_expired_lease_is_rejected() {
    // #126 acceptance: expired.
    let store = LeaseStore::new(0);
    let lease = store.issue("alice", forget_scope(), subject_alice());
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(
        store
            .check(lease.id, "alice", LeaseScope::ForgetSubject, "alice")
            .expect_err("a stale read must not keep authorising erasures"),
        LeaseError::Expired
    );
}

#[test]
fn a_lease_without_the_forget_scope_is_rejected() {
    // #126 acceptance: wrong scope.
    let store = LeaseStore::new(60);
    let scopeless = store.issue("alice", BTreeSet::new(), subject_alice());
    assert_eq!(
        store
            .check(scopeless.id, "alice", LeaseScope::ForgetSubject, "alice")
            .expect_err("a lease must name the scope it is spent on"),
        LeaseError::ScopeMissing {
            wanted: LeaseScope::ForgetSubject
        }
    );
}

#[test]
fn an_invented_lease_token_is_rejected() {
    // A caller that never read at all cannot fabricate its way past the gate.
    let store = LeaseStore::new(60);
    assert_eq!(
        store
            .check(
                uuid::Uuid::now_v7(),
                "alice",
                LeaseScope::ForgetSubject,
                "alice"
            )
            .expect_err("an unissued token must not authorise anything"),
        LeaseError::NotFound
    );
}

#[test]
fn a_lease_is_not_reusable_after_it_expires_even_by_its_owner() {
    // Expiry is not a courtesy to strangers: the principal that earned the
    // lease also loses it. Otherwise the lease decays into an ambient
    // permission for whoever read once, which is the thing it replaces.
    let store = LeaseStore::new(0);
    let lease = store.issue("alice", forget_scope(), subject_alice());
    std::thread::sleep(Duration::from_millis(10));
    assert!(
        store
            .check(lease.id, "alice", LeaseScope::ForgetSubject, "alice")
            .is_err()
    );
}

#[test]
fn a_lease_earned_by_a_narrow_read_cannot_erase_a_wider_subject() {
    // #160 acceptance, and the last of ADR 0001's four properties to land.
    //
    // Until this shipped, the three checks above all passed for an erasure of a
    // subject the caller had never read: freshness held, causality held,
    // caller-binding held, and the delete was still unauthorised. An injected
    // "now forget everything about bob" rode a legitimate read of alice.
    let store = LeaseStore::new(60);
    let narrow = store.issue("alice", forget_scope(), subject_alice());

    assert_eq!(
        store
            .check(narrow.id, "alice", LeaseScope::ForgetSubject, "bob")
            .expect_err("reading about alice is not authority to erase bob"),
        LeaseError::SubjectNotCovered {
            subject: "bob".to_string(),
            covered: vec!["alice".to_string()],
        }
    );
    store
        .check(narrow.id, "alice", LeaseScope::ForgetSubject, "alice")
        .expect("the subject the read covered is still erasable — this narrows, it does not block");
}

#[test]
fn a_read_that_surfaced_no_subject_authorises_no_erasure() {
    // #160, fail-closed direction. An empty coverage set must mean "nothing",
    // never "anything". Getting this backwards would turn every subject-free
    // recall into a blanket erasure grant — strictly worse than the gap #160
    // described.
    let store = LeaseStore::new(60);
    let empty = store.issue("alice", forget_scope(), BTreeSet::new());
    assert!(
        store
            .check(empty.id, "alice", LeaseScope::ForgetSubject, "alice")
            .is_err(),
        "a lease covering no subject must authorise no erasure"
    );
}

#[test]
fn concurrent_callers_get_independent_leases() {
    // A shared store must not let one caller's activity validate another's
    // act, under concurrency as well as sequentially.
    let store = Arc::new(LeaseStore::new(60));
    let handles: Vec<_> = ["alice", "bob", "carol"]
        .into_iter()
        .map(|who| {
            let store = store.clone();
            std::thread::spawn(move || (who, store.issue(who, forget_scope(), subject_alice()).id))
        })
        .collect();
    let issued: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    for (owner, id) in &issued {
        store
            .check(*id, owner, LeaseScope::ForgetSubject, "alice")
            .expect("each caller can spend its own lease");
        for (other, _) in &issued {
            if other != owner {
                assert!(
                    store
                        .check(*id, other, LeaseScope::ForgetSubject, "alice")
                        .is_err(),
                    "{other} must not be able to spend {owner}'s lease"
                );
            }
        }
    }
}
