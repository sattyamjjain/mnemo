//! End-to-end: write provenance is recorded on REMEMBER, is queryable by
//! memory / principal / session, is tamper-evident, and FORGET BY PROVENANCE
//! revokes exactly the target principal's or session's writes — against a real
//! DuckDB-backed engine.

use std::sync::Arc;

use mnemo_core::embedding::DeterministicEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::capability::CapabilityIssuer;
use mnemo_core::model::write_provenance::WriteOp;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::forget::ForgetStrategy;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::duckdb::DuckDbStorage;

const CAP_KEY: &[u8] = &[7u8; 32];

fn engine() -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(16).unwrap());
    let embedding = Arc::new(DeterministicEmbedding::new(16));
    let issuer = Arc::new(CapabilityIssuer::new("mnemo-cap-test", CAP_KEY));
    Arc::new(
        MnemoEngine::new(storage, index, embedding, "agent-x".to_string(), None)
            .with_capability_issuer(issuer),
    )
}

fn req(content: &str, created_by: Option<&str>, thread_id: Option<&str>) -> RememberRequest {
    let mut r = RememberRequest::new(content.to_string());
    r.created_by = created_by.map(|s| s.to_string());
    r.thread_id = thread_id.map(|s| s.to_string());
    r
}

#[tokio::test]
async fn remember_records_queryable_provenance() {
    let e = engine();
    let a = e
        .remember(req("m1", Some("alice"), Some("sess-1")))
        .await
        .unwrap();
    e.remember(req("m2", Some("alice"), Some("sess-1")))
        .await
        .unwrap();
    e.remember(req("m3", Some("bob"), Some("sess-2")))
        .await
        .unwrap();

    // by memory id
    let p = e
        .write_provenance_for(a.id)
        .await
        .unwrap()
        .expect("provenance recorded for the memory");
    assert_eq!(p.principal, "alice");
    assert_eq!(p.op, WriteOp::Remember);
    assert_eq!(p.memory_id, a.id);

    // by principal — the query that makes incident cleanup possible
    let alice = e.writes_by_principal("alice", 100).await.unwrap();
    assert_eq!(alice.len(), 2);
    assert!(alice.iter().all(|r| r.principal == "alice"));

    // by session
    let sess1 = e.writes_by_session("sess-1", 100).await.unwrap();
    assert_eq!(sess1.len(), 2);
}

#[tokio::test]
async fn provenance_chain_is_tamper_evident() {
    let e = engine();
    for i in 0..5 {
        e.remember(req(&format!("m{i}"), Some("p"), Some("s")))
            .await
            .unwrap();
    }
    let r = e.verify_provenance_chain(1000).await.unwrap();
    assert!(r.valid, "chain should verify: {:?}", r.error_message);
    assert_eq!(r.verified_records, 5);
}

#[tokio::test]
async fn forget_by_principal_revokes_only_that_principals_writes() {
    let e = engine();
    let a1 = e.remember(req("a1", Some("alice"), None)).await.unwrap();
    let a2 = e.remember(req("a2", Some("alice"), None)).await.unwrap();
    let b1 = e.remember(req("b1", Some("bob"), None)).await.unwrap();

    let resp = e
        .forget_by_principal("alice", ForgetStrategy::HardDelete)
        .await
        .unwrap();
    assert_eq!(resp.forgotten.len(), 2);
    assert!(resp.errors.is_empty());

    assert!(e.storage.get_memory(a1.id).await.unwrap().is_none());
    assert!(e.storage.get_memory(a2.id).await.unwrap().is_none());
    assert!(
        e.storage.get_memory(b1.id).await.unwrap().is_some(),
        "bob's memory must survive a forget-by-principal(alice)"
    );
}

#[tokio::test]
async fn forget_by_session_revokes_a_whole_session() {
    let e = engine();
    let poisoned = e
        .remember(req("x", Some("alice"), Some("sess-poison")))
        .await
        .unwrap();
    e.remember(req("y", Some("bob"), Some("sess-poison")))
        .await
        .unwrap();
    let keep = e
        .remember(req("z", Some("alice"), Some("sess-ok")))
        .await
        .unwrap();

    let resp = e
        .forget_by_session("sess-poison", ForgetStrategy::HardDelete)
        .await
        .unwrap();
    assert_eq!(resp.forgotten.len(), 2);
    assert!(e.storage.get_memory(poisoned.id).await.unwrap().is_none());
    assert!(
        e.storage.get_memory(keep.id).await.unwrap().is_some(),
        "the other session's memory must survive"
    );
}

#[tokio::test]
async fn capability_authorised_write_records_capability() {
    let e = engine();
    // Same key/id as the engine's issuer, so the token verifies.
    let issuer = CapabilityIssuer::new("mnemo-cap-test", CAP_KEY);
    let cap = issuer.issue("carol", "remember", None);
    let resp = e
        .remember_with_capability(req("authored", None, Some("sess-c")), &cap)
        .await
        .unwrap();
    let p = e.write_provenance_for(resp.id).await.unwrap().unwrap();
    assert_eq!(p.principal, "carol", "principal comes from the capability");
    assert_eq!(p.capability_id, Some(cap.id));
}

#[tokio::test]
async fn invalid_capability_is_rejected() {
    let e = engine();
    // Different key => signature will not verify against the engine's issuer.
    let forged = CapabilityIssuer::new("mnemo-cap-test", &[99u8; 32]);
    let cap = forged.issue("mallory", "remember", None);
    let result = e.remember_with_capability(req("x", None, None), &cap).await;
    assert!(
        result.is_err(),
        "a capability signed with the wrong key must be rejected"
    );
}
