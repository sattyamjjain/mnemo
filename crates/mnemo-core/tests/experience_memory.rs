//! Experience-memory tier (DocTrace, arXiv:2606.10921) integration tests.
//!
//! Covers the contract from the change spec:
//!   1. a successful plan is stored (and failures are not);
//!   2. a stored plan is replayed on a structurally-similar query;
//!   3. a plan is NOT replayed on a dissimilar query;
//!   4. consent/RBAC is respected (private plans are invisible to other
//!      agents; public plans are visible);
//!   5. the mode is inert (and default recall unchanged) when disabled;
//!   6. plan records never leak into ordinary recall.

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::memory::Scope;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::experience::{RecallPlanRequest, RememberPlanRequest};
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;

const AGENT: &str = "experience-owner";

fn build_engine(experience_on: bool) -> MnemoEngine {
    let storage =
        Arc::new(mnemo_core::storage::duckdb::DuckDbStorage::open_in_memory().expect("duckdb"));
    let index = Arc::new(UsearchIndex::new(8).expect("usearch"));
    let embedding = Arc::new(NoopEmbedding::new(8));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy"));
    let eng =
        MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft);
    if experience_on {
        eng.with_experience_memory()
    } else {
        eng
    }
}

fn plan_req(query: &str, score: f32, scope: Option<Scope>) -> RememberPlanRequest {
    RememberPlanRequest {
        query: query.to_string(),
        steps: vec!["bm25(reset)".to_string(), "rerank".to_string()],
        chunk_ids: vec!["chunk-a".to_string(), "chunk-b".to_string()],
        outcome_score: score,
        agent_id: None,
        scope,
        org_id: None,
    }
}

fn recall_req(query: &str, agent_id: Option<&str>) -> RecallPlanRequest {
    RecallPlanRequest {
        query: query.to_string(),
        agent_id: agent_id.map(str::to_string),
        org_id: None,
        similarity_threshold: None,
    }
}

#[tokio::test]
async fn plan_stored_on_success_and_replayed_on_similar_query() {
    let engine = build_engine(true);

    let stored = engine
        .remember_plan(plan_req("how do I reset my password", 0.92, None))
        .await
        .unwrap();
    assert!(stored.stored, "a confirmed-good plan must be cached");
    let plan_id = stored.id.expect("stored plan has an id");

    // A structurally-similar query (reordered, re-cased, repunctuated)
    // replays the same plan.
    let replay = engine
        .recall_plan(recall_req("Password reset — how to?", None))
        .await
        .unwrap();
    let plan = replay.plan.expect("similar query must replay the plan");
    assert_eq!(plan.id, plan_id);
    assert_eq!(plan.chunk_ids, vec!["chunk-a", "chunk-b"]);
    assert_eq!(plan.steps, vec!["bm25(reset)", "rerank"]);
    assert!(plan.similarity >= 0.7);
}

#[tokio::test]
async fn plan_not_replayed_on_dissimilar_query() {
    let engine = build_engine(true);
    engine
        .remember_plan(plan_req("how do I reset my password", 0.92, None))
        .await
        .unwrap();

    let miss = engine
        .recall_plan(recall_req("what is the capital of france", None))
        .await
        .unwrap();
    assert!(
        miss.plan.is_none(),
        "a dissimilar query must not replay an unrelated plan"
    );
}

#[tokio::test]
async fn failed_outcomes_are_not_cached() {
    let engine = build_engine(true);
    let resp = engine
        .remember_plan(plan_req("flaky query that did not resolve", 0.30, None))
        .await
        .unwrap();
    assert!(!resp.stored, "below-threshold outcomes must not be cached");
    assert!(resp.id.is_none());

    let miss = engine
        .recall_plan(recall_req("flaky query that did not resolve", None))
        .await
        .unwrap();
    assert!(miss.plan.is_none());
}

#[tokio::test]
async fn mode_off_is_inert() {
    let engine = build_engine(false);
    // remember_plan errors when the mode is disabled.
    let err = engine
        .remember_plan(plan_req("how do I reset my password", 0.92, None))
        .await;
    assert!(err.is_err(), "remember_plan must error when mode is off");
    // recall_plan misses (never errors) when disabled.
    let miss = engine
        .recall_plan(recall_req("how do I reset my password", None))
        .await
        .unwrap();
    assert!(miss.plan.is_none());
    assert_eq!(miss.candidates_considered, 0);
}

#[tokio::test]
async fn rbac_private_plan_invisible_to_other_agents() {
    let engine = build_engine(true);
    // Owner caches a PRIVATE plan (default scope).
    engine
        .remember_plan(plan_req(
            "how do I reset my password",
            0.92,
            Some(Scope::Private),
        ))
        .await
        .unwrap();

    // A different agent must NOT replay the owner's private plan.
    let intruder = engine
        .recall_plan(recall_req("password reset how to", Some("intruder")))
        .await
        .unwrap();
    assert!(intruder.plan.is_none(), "private plan must be RBAC-gated");
    assert_eq!(intruder.candidates_considered, 0);

    // The owner still replays it.
    let owner = engine
        .recall_plan(recall_req("password reset how to", Some(AGENT)))
        .await
        .unwrap();
    assert!(owner.plan.is_some(), "owner must replay their own plan");
}

#[tokio::test]
async fn rbac_public_plan_visible_to_other_agents() {
    let engine = build_engine(true);
    engine
        .remember_plan(plan_req(
            "how do I reset my password",
            0.92,
            Some(Scope::Public),
        ))
        .await
        .unwrap();

    let other = engine
        .recall_plan(recall_req("password reset how to", Some("someone-else")))
        .await
        .unwrap();
    assert!(
        other.plan.is_some(),
        "a public plan must be replayable by other agents"
    );
}

#[tokio::test]
async fn plan_records_excluded_from_ordinary_recall() {
    let engine = build_engine(true);
    engine
        .remember_plan(plan_req("reset password flow", 0.92, None))
        .await
        .unwrap();
    // A normal memory sharing the same words.
    let mut normal = RememberRequest::new("reset password instructions for users".to_string());
    normal.tags = Some(vec!["doc".to_string()]);
    engine.remember(normal).await.unwrap();

    let mut req = RecallRequest::new("reset password".to_string());
    req.strategy = Some("lexical".to_string());
    req.limit = Some(10);
    let resp = engine.recall(req).await.unwrap();

    // The plan record (reserved tag) is excluded; the normal doc is found.
    assert!(
        resp.memories
            .iter()
            .all(|m| !m.tags.iter().any(|t| t == "__experience_plan__")),
        "experience-tier plans must never surface in ordinary recall"
    );
    assert!(
        resp.memories
            .iter()
            .any(|m| m.content.contains("instructions for users")),
        "ordinary recall must still return normal memories"
    );
}
