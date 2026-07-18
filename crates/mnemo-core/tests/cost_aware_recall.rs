//! Integration test for the cost-aware, answer-impact-scored recall
//! path ([`mnemo_core::query::evidence`]) wired through a real
//! `MnemoEngine`.
//!
//! Verifies, against an in-memory engine seeded with several memories:
//! - `max_evidence` caps the returned set;
//! - `stop_when_sufficient` returns a prefix and reports early-stop;
//! - the default (no `evidence_budget`) read path is unchanged;
//! - the response carries `evidence_selection` diagnostics;
//! - a larger budget never drops a higher-ranked item a smaller budget
//!   kept (prefix invariant) end-to-end through the engine.

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::evidence::{DeltaScorer, EvidenceBudget, ScorerKind};
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

const AGENT: &str = "cost-aware-itest";

fn build_engine() -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("duckdb open"));
    let index = Arc::new(UsearchIndex::new(3).expect("usearch new"));
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy open"));
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft)
}

async fn seed(engine: &MnemoEngine, n: usize, tag: &str) {
    for i in 0..n {
        let mut req = RememberRequest::new(format!("evidence chunk {i} about budget topic {tag}"));
        req.agent_id = Some(AGENT.to_string());
        req.tags = Some(vec![tag.to_string()]);
        // Descending importance so the ranked order is deterministic.
        req.importance = Some(1.0 - (i as f32) * 0.05);
        engine.remember(req).await.expect("remember");
    }
}

fn recall_req(tag: &str, budget: Option<EvidenceBudget>) -> RecallRequest {
    let mut r = RecallRequest::new("budget topic".to_string());
    r.agent_id = Some(AGENT.to_string());
    r.tags = Some(vec![tag.to_string()]);
    r.limit = Some(20);
    // Lexical (BM25) recall so the no-op embedder stays valid: this suite
    // deliberately exercises the evidence scorer's *retrieval-score fallback*
    // (q_emb is degenerate under NoopEmbedding). All seeds carry the query
    // tokens, so BM25 returns them; the vector-dependent "auto" path now
    // hard-errors under a no-op embedder (v0.5.13) and is covered separately.
    r.strategy = Some("lexical".to_string());
    r.evidence_budget = budget;
    r
}

#[tokio::test]
async fn no_budget_is_unchanged_and_omits_diagnostics() {
    let engine = build_engine();
    seed(&engine, 8, "plain").await;

    let resp = engine
        .recall(recall_req("plain", None))
        .await
        .expect("recall");
    assert!(
        resp.memories.len() >= 5,
        "default path returns the front-loaded set: got {}",
        resp.memories.len()
    );
    assert!(
        resp.evidence_selection.is_none(),
        "no budget → no evidence_selection diagnostics"
    );
}

#[tokio::test]
async fn max_evidence_caps_the_returned_set() {
    let engine = build_engine();
    seed(&engine, 8, "capped").await;

    let resp = engine
        .recall(recall_req("capped", Some(EvidenceBudget::capped(3))))
        .await
        .expect("recall");
    assert_eq!(resp.memories.len(), 3, "max_evidence=3 caps the result");
    let sel = resp
        .evidence_selection
        .expect("budget set → diagnostics present");
    assert_eq!(sel.returned, 3);
    assert!(sel.capped, "cap flag must be set");
    assert_eq!(sel.scorer, "cosine");
}

#[tokio::test]
async fn early_stop_returns_a_prefix() {
    let engine = build_engine();
    seed(&engine, 8, "early").await;

    // NoopEmbedding → cosine falls back to the retrieval score. The
    // top hit's score may be modest, so set a low cumulative bar that
    // a couple of chunks clear, and confirm we get fewer than the full
    // set plus an early-stop flag.
    let budget = EvidenceBudget::early_stop(0.05);
    let resp = engine
        .recall(recall_req("early", Some(budget)))
        .await
        .expect("recall");
    let sel = resp
        .evidence_selection
        .clone()
        .expect("diagnostics present");
    assert!(
        resp.memories.len() < 8,
        "early-stop must return fewer than the seeded 8: got {}",
        resp.memories.len()
    );
    assert_eq!(resp.memories.len(), sel.returned);
    assert!(sel.stopped_early, "early-stop flag must be set: {sel:?}");
    assert!(sel.cumulative_score >= 0.05);
}

#[tokio::test]
async fn delta_scorer_is_used_when_attached() {
    // Engine with an injected answer-impact scorer that always scores
    // 1.0 — a single chunk clears any sub-1.0 bar, proving the
    // injected closure path is live end-to-end.
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(3).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
    let engine = MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None)
        .with_full_text(ft)
        .with_evidence_scorer(Arc::new(DeltaScorer::new(|_ctx| 1.0)));

    seed(&engine, 6, "delta").await;

    let budget = EvidenceBudget {
        stop_when_sufficient: true,
        sufficiency_threshold: 0.9,
        scorer: ScorerKind::Delta,
        ..Default::default()
    };
    let resp = engine
        .recall(recall_req("delta", Some(budget)))
        .await
        .expect("recall");
    let sel = resp.evidence_selection.expect("diagnostics present");
    assert_eq!(sel.scorer, "delta", "the attached delta scorer must run");
    assert_eq!(
        resp.memories.len(),
        1,
        "a 1.0-scoring chunk clears the 0.9 bar after one"
    );
}

#[tokio::test]
async fn delta_kind_without_attached_scorer_falls_back_to_cosine() {
    // Request asks for Delta, but the engine has no scorer attached →
    // must fall back to cosine rather than error.
    let engine = build_engine();
    seed(&engine, 5, "fallback").await;

    let budget = EvidenceBudget {
        scorer: ScorerKind::Delta,
        max_evidence: Some(2),
        ..Default::default()
    };
    let resp = engine
        .recall(recall_req("fallback", Some(budget)))
        .await
        .expect("recall");
    let sel = resp.evidence_selection.expect("diagnostics present");
    assert_eq!(
        sel.scorer, "cosine",
        "delta-without-scorer must fall back to cosine"
    );
    assert_eq!(resp.memories.len(), 2);
}

#[tokio::test]
async fn larger_budget_is_prefix_superset_end_to_end() {
    let engine = build_engine();
    seed(&engine, 10, "prefix").await;

    let mut prev_ids: Vec<String> = Vec::new();
    for cap in 1..=10 {
        let resp = engine
            .recall(recall_req("prefix", Some(EvidenceBudget::capped(cap))))
            .await
            .expect("recall");
        let ids: Vec<String> = resp.memories.iter().map(|m| m.id.to_string()).collect();
        // The previous (smaller) budget's result must be a prefix of
        // this one — same ordering, no higher-ranked item dropped.
        for (i, id) in prev_ids.iter().enumerate() {
            assert_eq!(
                &ids[i], id,
                "cap={cap}: ordering changed at index {i} vs the smaller budget"
            );
        }
        assert!(
            ids.len() >= prev_ids.len(),
            "cap={cap}: larger budget returned fewer items"
        );
        prev_ids = ids;
    }
}
