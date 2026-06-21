//! Integration tests for the topic-document consolidation primitive
//! (Infini-Memory, arXiv:2606.10677).
//!
//! Covers: consolidating N members into a retrievable topic document,
//! provenance preservation, fact revision that keeps the old fact in
//! history, hash-chain integrity after consolidation, permission gating,
//! and round-tripping the new `EventType` variants.

use std::str::FromStr;
use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::event::EventType;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::consolidate::ConsolidateRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::duckdb::DuckDbStorage;

fn create_engine(agent_id: &str) -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(128).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(128));
    Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        agent_id.to_string(),
        None,
    ))
}

async fn remember(engine: &MnemoEngine, content: &str) -> uuid::Uuid {
    let mut req = RememberRequest::new(content.to_string());
    req.tags = Some(vec!["evidence".to_string()]);
    engine.remember(req).await.unwrap().id
}

#[tokio::test]
async fn consolidate_groups_members_into_retrievable_unit() {
    let engine = create_engine("agent-1");

    let m1 = remember(&engine, "Acme signed the MSA on 2026-01-05").await;
    let m2 = remember(&engine, "Acme's MSA renewal is annual").await;
    let m3 = remember(&engine, "Acme primary contact is Dana").await;

    let resp = engine
        .consolidate(ConsolidateRequest::new(
            vec![m1, m2, m3],
            "Acme account".to_string(),
        ))
        .await
        .unwrap();

    assert_eq!(resp.source_count, 3);
    assert_eq!(resp.version, 1);
    assert!(resp.superseded_id.is_none());
    assert_eq!(resp.member_ids.len(), 3);

    // Retrievable as a unit: the topic document exists as a Semantic record.
    let doc = engine
        .storage
        .get_memory(resp.topic_document_id)
        .await
        .unwrap()
        .expect("topic document persisted");
    assert_eq!(
        doc.memory_type,
        mnemo_core::model::memory::MemoryType::Semantic
    );
    assert!(doc.tags.contains(&"Acme account".to_string()));

    // The evidence set is reachable via consolidated_from relations.
    let relations = engine
        .storage
        .get_relations_from(resp.topic_document_id)
        .await
        .unwrap();
    assert_eq!(relations.len(), 3);
    assert!(
        relations
            .iter()
            .all(|r| r.relation_type == "consolidated_from")
    );
    let targets: std::collections::HashSet<_> = relations.iter().map(|r| r.target_id).collect();
    assert!(targets.contains(&m1) && targets.contains(&m2) && targets.contains(&m3));
}

#[tokio::test]
async fn consolidate_preserves_provenance_metadata() {
    let engine = create_engine("agent-1");
    let m1 = remember(&engine, "fact one").await;
    let m2 = remember(&engine, "fact two").await;

    let resp = engine
        .consolidate(ConsolidateRequest::new(vec![m1, m2], "topic".to_string()))
        .await
        .unwrap();

    let doc = engine
        .storage
        .get_memory(resp.topic_document_id)
        .await
        .unwrap()
        .unwrap();
    let meta = doc.metadata.as_object().expect("metadata object");

    assert_eq!(meta["topic"], serde_json::json!("topic"));
    let from = meta["consolidated_from"].as_array().unwrap();
    assert_eq!(from.len(), 2);
    // Per-member provenance (source + timestamp + confidence) is preserved.
    let members = meta["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    for m in members {
        let o = m.as_object().unwrap();
        assert!(o.contains_key("id"));
        assert!(o.contains_key("source_type"));
        assert!(o.contains_key("created_at"));
        assert!(o.contains_key("importance"));
    }
}

#[tokio::test]
async fn revision_supersedes_but_keeps_history() {
    let engine = create_engine("agent-1");
    let m1 = remember(&engine, "old fact: price is $10").await;
    let v1 = engine
        .consolidate(ConsolidateRequest::new(vec![m1], "pricing".to_string()))
        .await
        .unwrap();

    let m2 = remember(&engine, "new fact: price is $12").await;
    let mut rev = ConsolidateRequest::new(vec![m1, m2], "pricing".to_string());
    rev.supersede = Some(v1.topic_document_id);
    let v2 = engine.consolidate(rev).await.unwrap();

    // New document is the current view, at version 2, chained to v1.
    assert_eq!(v2.version, 2);
    assert_eq!(v2.superseded_id, Some(v1.topic_document_id));
    let new_doc = engine
        .storage
        .get_memory(v2.topic_document_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_doc.version, 2);
    assert_eq!(new_doc.prev_version_id, Some(v1.topic_document_id));
    assert!(!new_doc.is_deleted());

    // Old fact remains in history: row retained (NOT deleted, so the hash
    // chain stays whole), marked Consolidated, with a superseded_by pointer.
    let old_doc = engine
        .storage
        .get_memory(v1.topic_document_id)
        .await
        .unwrap()
        .expect("old topic document retained for audit");
    assert!(!old_doc.is_deleted(), "old fact retained for audit");
    assert_eq!(
        old_doc.consolidation_state,
        mnemo_core::model::memory::ConsolidationState::Consolidated
    );
    assert_eq!(
        old_doc.metadata.as_object().unwrap()["superseded_by"],
        serde_json::json!(v2.topic_document_id.to_string())
    );
}

#[tokio::test]
async fn consolidate_emits_hash_chained_audit_events() {
    let engine = create_engine("agent-1");
    let m1 = remember(&engine, "a").await;
    let m2 = remember(&engine, "b").await;

    let v1 = engine
        .consolidate(ConsolidateRequest::new(vec![m1, m2], "t".to_string()))
        .await
        .unwrap();

    // Consolidation event recorded.
    let events = engine
        .storage
        .list_events("agent-1", 1000, 0)
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.event_type == EventType::MemoryConsolidated),
        "MemoryConsolidated event present"
    );
    assert!(v1.revision_event_id.is_none());

    // A revision additionally emits MemoryRevised.
    let m3 = remember(&engine, "c").await;
    let mut rev = ConsolidateRequest::new(vec![m3], "t".to_string());
    rev.supersede = Some(v1.topic_document_id);
    let v2 = engine.consolidate(rev).await.unwrap();
    assert!(v2.revision_event_id.is_some());
    let events = engine
        .storage
        .list_events("agent-1", 1000, 0)
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.event_type == EventType::MemoryRevised)
    );

    // Hash chains stay intact after consolidation + revision.
    let mem_chain = engine
        .verify_integrity(Some("agent-1".to_string()), None)
        .await
        .unwrap();
    assert!(
        mem_chain.valid,
        "memory chain valid: {:?}",
        mem_chain.error_message
    );
    let evt_chain = engine
        .verify_event_integrity(Some("agent-1".to_string()), None)
        .await
        .unwrap();
    assert!(
        evt_chain.valid,
        "event chain valid: {:?}",
        evt_chain.error_message
    );
}

#[tokio::test]
async fn consolidate_rejects_unreadable_member() {
    let engine = create_engine("agent-1");

    // A member owned by a different agent, private and unshared.
    let mut other = RememberRequest::new("secret of agent-2".to_string());
    other.agent_id = Some("agent-2".to_string());
    let foreign = engine.remember(other).await.unwrap().id;

    let mine = remember(&engine, "my own note").await;

    let err = engine
        .consolidate(ConsolidateRequest::new(
            vec![mine, foreign],
            "mixed".to_string(),
        ))
        .await;
    assert!(
        err.is_err(),
        "consolidation must abort on unreadable member"
    );

    // Nothing partial written: no consolidated_from relations exist for `mine`.
    let rels = engine.storage.get_relations_from(mine).await.unwrap();
    assert!(rels.is_empty());
}

#[tokio::test]
async fn consolidate_rejects_empty_and_missing() {
    let engine = create_engine("agent-1");
    assert!(
        engine
            .consolidate(ConsolidateRequest::new(vec![], "t".to_string()))
            .await
            .is_err()
    );
    assert!(
        engine
            .consolidate(ConsolidateRequest::new(
                vec![uuid::Uuid::now_v7()],
                "t".to_string()
            ))
            .await
            .is_err()
    );
}

#[test]
fn new_event_types_round_trip() {
    for et in [EventType::MemoryConsolidated, EventType::MemoryRevised] {
        let s = et.to_string();
        assert_eq!(EventType::from_str(&s).unwrap(), et);
    }
    assert_eq!(
        EventType::MemoryConsolidated.to_string(),
        "memory_consolidated"
    );
    assert_eq!(EventType::MemoryRevised.to_string(), "memory_revised");
}
