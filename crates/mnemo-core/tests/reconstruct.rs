//! Integration tests for the active-reconstruction recall strategy
//! (MRAgent, arXiv:2606.06036).
//!
//! Covers: `reconstruct` returns the raw hits PLUS a belief-state node that
//! references graph-linked context; the belief node's source/linked id sets
//! are disjoint; the typed `RetrievalMode::Reconstruct` selects the same
//! path; and the default `auto` read path is unchanged (no belief node).

use std::collections::HashSet;
use std::sync::Arc;

use mnemo_core::embedding::DeterministicEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::retrieval::RetrievalMode;
use mnemo_core::storage::duckdb::DuckDbStorage;

fn create_engine(agent_id: &str) -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(128).unwrap());
    let embedding = Arc::new(DeterministicEmbedding::new(128));
    Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        agent_id.to_string(),
        None,
    ))
}

async fn remember(engine: &MnemoEngine, content: &str) -> uuid::Uuid {
    engine
        .remember(RememberRequest::new(content.to_string()))
        .await
        .unwrap()
        .id
}

async fn remember_related(engine: &MnemoEngine, content: &str, related: uuid::Uuid) -> uuid::Uuid {
    let mut req = RememberRequest::new(content.to_string());
    req.related_to = Some(vec![related.to_string()]);
    engine.remember(req).await.unwrap().id
}

#[tokio::test]
async fn reconstruct_returns_belief_with_linked_context() {
    let engine = create_engine("agent-1");

    // Two related memories. Whichever one is the single retrieved hit, the
    // other must be pulled in as graph-linked context by the belief walk.
    let a = remember(&engine, "Paris is the capital of France").await;
    let b = remember_related(&engine, "France is in Europe", a).await;

    let mut req = RecallRequest::new("France".to_string());
    req.strategy = Some("reconstruct".to_string());
    req.limit = Some(1);
    let resp = engine.recall(req).await.unwrap();

    let belief = resp
        .reconstruction
        .expect("reconstruct strategy must attach a belief-state node");
    assert_eq!(belief.cue, "France");
    assert_eq!(belief.source_ids.len(), 1, "limit=1 yields one source hit");
    assert!(
        !belief.linked_context_ids.is_empty(),
        "the graph walk must surface the related memory as linked context"
    );

    // source and linked id sets are disjoint, and together cover both memories.
    let sources: HashSet<_> = belief.source_ids.iter().copied().collect();
    let linked: HashSet<_> = belief.linked_context_ids.iter().copied().collect();
    assert!(
        sources.is_disjoint(&linked),
        "linked context excludes sources"
    );
    let all: HashSet<_> = sources.union(&linked).copied().collect();
    assert!(all.contains(&a) && all.contains(&b));

    assert!(belief.summary.contains("Reconstructed belief"));
    assert!(belief.summary.contains("Linked context"));
    assert!(belief.confidence.is_finite());

    // The raw hits are still returned alongside the belief node.
    assert_eq!(resp.memories.len(), 1);
}

#[tokio::test]
async fn typed_mode_reconstruct_matches_string_strategy() {
    let engine = create_engine("agent-1");
    let a = remember(&engine, "alpha").await;
    let _b = remember_related(&engine, "beta", a).await;

    let mut req = RecallRequest::new("alpha".to_string());
    req.mode = Some(RetrievalMode::Reconstruct);
    let resp = engine.recall(req).await.unwrap();
    assert!(
        resp.reconstruction.is_some(),
        "RetrievalMode::Reconstruct must select the reconstruct path"
    );
}

#[tokio::test]
async fn default_recall_has_no_belief_node() {
    let engine = create_engine("agent-1");
    let a = remember(&engine, "gamma").await;
    let _b = remember_related(&engine, "delta", a).await;

    // Default (auto / RRF) — additive guarantee: no reconstruction node.
    let resp = engine
        .recall(RecallRequest::new("gamma".to_string()))
        .await
        .unwrap();
    assert!(
        resp.reconstruction.is_none(),
        "default read path must be unchanged (no belief node)"
    );

    // Explicit semantic — also no belief node.
    let mut req = RecallRequest::new("gamma".to_string());
    req.strategy = Some("semantic".to_string());
    let resp = engine.recall(req).await.unwrap();
    assert!(resp.reconstruction.is_none());
}

#[tokio::test]
async fn reconstruct_with_no_matches_yields_empty_belief() {
    let engine = create_engine("empty-agent");
    let mut req = RecallRequest::new("nothing here".to_string());
    req.strategy = Some("reconstruct".to_string());
    let resp = engine.recall(req).await.unwrap();
    let belief = resp
        .reconstruction
        .expect("belief node present even on miss");
    assert!(belief.source_ids.is_empty());
    assert_eq!(belief.confidence, 0.0);
}
