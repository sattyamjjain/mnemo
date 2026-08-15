//! Server-level wiring for per-request identity
//! ([ADR 0002](../../../docs/adr/0002-request-identity-model.md)).
//!
//! `crate::identity`'s unit tests already cover the *resolution* rules
//! exhaustively. This file covers the thing those cannot: that
//! [`MnemoServer`] is actually wired to them — that a capability presented in
//! a request's `_meta` reaches the verifier, that its principal displaces the
//! boot identity, and that a bad capability is rejected instead of downgraded.
//!
//! That distinction is not academic. On 2026-08-15 this repo shipped a CI
//! guard that asserted a property in one place while the code it guarded ran
//! somewhere else, and the job passed over sixteen skipped tests. A rule that
//! is unit-tested but unwired is the recurring defect here (`role_filter`
//! #124, tool-catalog attestation v0.5.20, `LeaseStore` → ADR 0001).

use std::sync::Arc;

use chrono::Duration;
use mnemo_core::embedding::DeterministicEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::capability::{Capability, CapabilityIssuer};
use mnemo_core::query::MnemoEngine;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_mcp::identity::CAPABILITY_META_KEY;
use mnemo_mcp::server::MnemoServer;
use rmcp::model::RequestMetaObject;

const BOOT_AGENT: &str = "boot-agent";

fn engine() -> Arc<MnemoEngine> {
    Arc::new(MnemoEngine::new(
        Arc::new(DuckDbStorage::open_in_memory().unwrap()),
        Arc::new(UsearchIndex::new(128).unwrap()),
        Arc::new(DeterministicEmbedding::new(128)),
        BOOT_AGENT.to_string(),
        None,
    ))
}

fn issuer() -> Arc<CapabilityIssuer> {
    Arc::new(CapabilityIssuer::new("k1", b"integration-test-key"))
}

/// A `_meta` object carrying `cap` under the capability key.
fn meta_with(cap: &Capability) -> RequestMetaObject {
    let mut meta = RequestMetaObject::default();
    meta.0.0.insert(
        CAPABILITY_META_KEY.to_string(),
        serde_json::to_value(cap).expect("capability serialises"),
    );
    meta
}

#[test]
fn no_capability_keeps_the_boot_identity() {
    // The default deployment. This MUST keep working unchanged or every
    // existing stdio operator breaks on upgrade.
    let server = MnemoServer::new(engine());
    let caller = server
        .resolve_caller(&RequestMetaObject::default())
        .expect("a request without a capability is ordinary, not an error");
    assert_eq!(caller.caller_id, BOOT_AGENT);
    assert!(caller.roles.is_empty());
}

#[test]
fn a_verified_capability_displaces_the_boot_identity() {
    let iss = issuer();
    let server = MnemoServer::new(engine()).with_capability_issuer(iss.clone());
    let cap = iss.issue("alice", "role:reader", Some(Duration::minutes(5)));

    let caller = server
        .resolve_caller(&meta_with(&cap))
        .expect("a valid capability is accepted");

    assert_eq!(
        caller.caller_id, "alice",
        "the capability's principal must become the caller id, not `{BOOT_AGENT}`"
    );
    assert_eq!(caller.roles, vec!["reader".to_string()]);
}

#[test]
fn two_capabilities_resolve_to_two_distinct_callers_on_one_server() {
    // #126's revisit gate is "distinct callers hold distinct identities". This
    // is that gate, met on one server instance without a new transport.
    let iss = issuer();
    let server = MnemoServer::new(engine()).with_capability_issuer(iss.clone());

    let alice = server
        .resolve_caller(&meta_with(&iss.issue("alice", "role:reader", None)))
        .expect("alice's capability is valid");
    let bob = server
        .resolve_caller(&meta_with(&iss.issue("bob", "role:writer", None)))
        .expect("bob's capability is valid");

    assert_eq!(alice.caller_id, "alice");
    assert_eq!(bob.caller_id, "bob");
    assert_ne!(
        alice.caller_id, bob.caller_id,
        "two callers on one server must not collapse to one identity"
    );
    assert_eq!(alice.roles, vec!["reader".to_string()]);
    assert_eq!(bob.roles, vec!["writer".to_string()]);
}

#[test]
fn a_forged_capability_is_rejected_not_downgraded() {
    let iss = issuer();
    let server = MnemoServer::new(engine()).with_capability_issuer(iss.clone());
    let mut cap = iss.issue("alice", "role:admin", None);
    cap.principal = "mallory".into(); // signature no longer covers the principal

    let err = server
        .resolve_caller(&meta_with(&cap))
        .expect_err("a tampered capability must be rejected");

    // The property that matters: rejection, NOT a quiet fall back to the
    // operator's identity. A downgrade would grant the forgery more authority
    // than it even claimed.
    assert!(
        err.message.contains("capability rejected"),
        "unexpected error message: {}",
        err.message
    );
}

#[test]
fn an_expired_capability_is_rejected() {
    let iss = issuer();
    let server = MnemoServer::new(engine()).with_capability_issuer(iss.clone());
    let cap = iss.issue("alice", "role:admin", Some(Duration::seconds(-1)));

    let err = server
        .resolve_caller(&meta_with(&cap))
        .expect_err("an expired capability must be rejected");
    assert!(
        err.message.contains("expired"),
        "unexpected error message: {}",
        err.message
    );
}

#[test]
fn presenting_a_capability_to_a_server_with_no_issuer_is_an_error() {
    // Fail closed. A server that cannot verify has no basis to act on a token,
    // and treating the caller as the operator would be strictly worse than
    // having no capability support at all.
    let server = MnemoServer::new(engine()); // deliberately no issuer
    let cap = issuer().issue("alice", "role:admin", None);

    let err = server
        .resolve_caller(&meta_with(&cap))
        .expect_err("an unverifiable capability must not be silently accepted");
    assert!(
        err.message.contains("no issuer key configured"),
        "unexpected error message: {}",
        err.message
    );
}

#[test]
fn resource_scoping_and_tool_gating_read_the_same_identity() {
    // `list_resources` scopes reads by caller id while the role filter gates
    // tools by caller id. ADR 0002 §"What this changes in the code" calls out
    // that if these two drift apart, one agent's memories get served to every
    // authenticated caller behind a correct-looking tool gate — a read leak.
    // Both now come from `resolve_caller`, so pin that they agree.
    let iss = issuer();
    let server = MnemoServer::new(engine()).with_capability_issuer(iss.clone());
    let cap = iss.issue("carol", "role:reader", None);

    let resolved = server.resolve_caller(&meta_with(&cap)).unwrap();
    assert_eq!(resolved.caller_id, "carol");

    // The boot-derived accessor is the FALLBACK and must still report the boot
    // identity — it is what a capability-less request gets. The point is that
    // the request path no longer uses it when a capability is present.
    assert_eq!(
        server.caller_agent_id(),
        BOOT_AGENT,
        "the boot accessor is the no-capability fallback and should be unchanged"
    );
}
