//! v0.4.0-rc3 (Task B1) — end-to-end test that the provenance chain
//! survives the full `engine.recall(with_provenance=true)` path
//! against a real DuckDB-backed engine.

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::provenance::{ProvenanceSigner, verify_read_provenance};
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::duckdb::DuckDbStorage;

fn make_engine_with_signer() -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(16).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(16));
    let signer = Arc::new(ProvenanceSigner::new("mnemo-prov-test", &[42u8; 32]));
    Arc::new(
        MnemoEngine::new(storage, index, embedding, "prov-agent".to_string(), None)
            .with_provenance_signer(signer),
    )
}

async fn seed_three(engine: &MnemoEngine) {
    for content in &[
        "Patient prefers dark mode for the dashboard.",
        "Quarterly forecast landed at $42M.",
        "User reported headache on 2026-04-20.",
    ] {
        engine
            .remember(RememberRequest {
                content: (*content).to_string(),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: Some(0.5),
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            })
            .await
            .unwrap();
    }
}

fn recall_request(query: &str, with_provenance: Option<bool>) -> RecallRequest {
    let mut r = RecallRequest::new(query.to_string());
    r.with_provenance = with_provenance;
    r
}

#[tokio::test]
async fn recall_without_with_provenance_returns_no_receipt() {
    let engine = make_engine_with_signer();
    seed_three(&engine).await;
    let resp = engine
        .recall(recall_request("anything", None))
        .await
        .unwrap();
    assert!(
        resp.provenance.is_none(),
        "default recall must not produce a provenance receipt"
    );
}

#[tokio::test]
async fn recall_with_provenance_signs_a_verifiable_receipt() {
    let engine = make_engine_with_signer();
    seed_three(&engine).await;

    let resp = engine
        .recall(recall_request("anything", Some(true)))
        .await
        .unwrap();
    let prov = resp
        .provenance
        .expect("with_provenance=true should produce a receipt");

    // Receipt should reference at least one record.
    assert!(
        !prov.derived_from.is_empty(),
        "non-empty recall must produce a non-empty derived_from list"
    );

    // Pull the cited records back from storage to verify.
    let mut records = Vec::new();
    for r in &prov.derived_from {
        let rec = engine.storage.get_memory(r.id).await.unwrap().unwrap();
        records.push(rec);
    }

    let signer = ProvenanceSigner::new("mnemo-prov-test", &[42u8; 32]);
    verify_read_provenance(&prov, &records, &signer).expect("receipt must verify");
}

#[tokio::test]
async fn recall_with_provenance_when_signer_absent_returns_no_receipt() {
    // Engine intentionally without a signer.
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(16).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(16));
    let engine = Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        "prov-agent".to_string(),
        None,
    ));
    seed_three(&engine).await;

    let resp = engine
        .recall(recall_request("anything", Some(true)))
        .await
        .unwrap();
    assert!(
        resp.provenance.is_none(),
        "without an attached signer, with_provenance=true is silently a no-op"
    );
}

#[tokio::test]
async fn tampered_storage_record_fails_post_recall_verification() {
    let engine = make_engine_with_signer();
    seed_three(&engine).await;

    let resp = engine
        .recall(recall_request("anything", Some(true)))
        .await
        .unwrap();
    let prov = resp.provenance.unwrap();

    // Pull records, then mutate the first one's content_hash to
    // simulate post-recall tampering by an attacker who got write
    // access to the DB.
    let mut records = Vec::new();
    for r in &prov.derived_from {
        let mut rec = engine.storage.get_memory(r.id).await.unwrap().unwrap();
        if records.is_empty() {
            rec.content_hash = vec![0xCC; 32];
        }
        records.push(rec);
    }

    let signer = ProvenanceSigner::new("mnemo-prov-test", &[42u8; 32]);
    let err = verify_read_provenance(&prov, &records, &signer).unwrap_err();
    assert!(matches!(
        err,
        mnemo_core::provenance::ProvenanceError::RecordContentHashMismatch { .. }
    ));
}
