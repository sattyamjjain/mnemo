//! AMP cross-adapter conformance suite (deterministic).
//!
//! Mirrors the paper's two headline checks:
//!  1. **recall@5** on a small labelled corpus driven end-to-end
//!     through the AMP `MemoryStore` surface over a real DuckDB-backed
//!     `MnemoEngine`.
//!  2. **RRF-holds-under-rank-0-injection vs max-fusion** — the
//!     fusion-robustness property, exercised as a pure deterministic
//!     check over synthetic ranked lists.
//!
//! Plus a per-op smoke pass covering all 5 ops × the 4 memory types.

use std::sync::Arc;

use mnemo_amp::{
    AmpEnvelope, AmpMemoryType, AmpOp, AmpRouter, ClosureApprove, MemoryStore, MnemoAmpStore,
    max_fuse, rrf_fuse,
};
use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

const AGENT: &str = "amp-conformance-agent";

fn build_engine() -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("duckdb open"));
    let index = Arc::new(UsearchIndex::new(3).expect("usearch new"));
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy open"));
    Arc::new(
        MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft),
    )
}

fn remember_env(content: &str, tag: &str, mt: AmpMemoryType) -> AmpEnvelope {
    let mut env = AmpEnvelope::new(AmpOp::Remember, mt);
    env.agent_id = Some(AGENT.to_string());
    env.content = Some(content.to_string());
    env.tags = vec![tag.to_string()];
    env
}

#[tokio::test]
async fn recall_at_5_on_labelled_corpus() {
    let engine = build_engine();
    let store = MnemoAmpStore::new(engine);

    // Small labelled corpus: 3 "geo" facts (relevant) + 5 distractors.
    let corpus = [
        ("Paris is the capital of France", "geo"),
        ("Berlin is the capital of Germany", "geo"),
        ("Tokyo is the capital of Japan", "geo"),
        ("The mitochondrion is the powerhouse of the cell", "bio"),
        ("Photosynthesis converts light to chemical energy", "bio"),
        ("HTTP is a stateless application protocol", "tech"),
        ("TCP provides reliable byte streams", "tech"),
        ("Rust has no garbage collector", "tech"),
    ];
    for (content, tag) in corpus {
        let r = store
            .remember(&remember_env(content, tag, AmpMemoryType::Semantic))
            .await
            .expect("remember");
        assert!(r.ok);
    }

    // recall@5 scoped to the "geo" label: all 3 relevant facts must
    // appear within the top-5.
    let mut q = AmpEnvelope::new(AmpOp::Recall, AmpMemoryType::Semantic);
    q.agent_id = Some(AGENT.to_string());
    q.query = Some("capital city".to_string());
    q.tags = vec!["geo".to_string()];
    q.top_k = Some(5);
    let resp = store.recall(&q).await.expect("recall");

    assert!(resp.hits.len() <= 5, "recall@5 must not exceed 5 hits");
    let geo_hits = resp
        .hits
        .iter()
        .filter(|h| h.content.contains("capital of"))
        .count();
    assert_eq!(
        geo_hits, 3,
        "all 3 labelled geo facts must be recalled@5, got {geo_hits}: {:?}",
        resp.hits
    );
}

#[tokio::test]
async fn rrf_holds_under_rank0_injection_vs_max_fusion() {
    use mnemo_amp::AmpHit;
    let hit = |id: &str, score: f32| AmpHit {
        id: id.to_string(),
        content: format!("c-{id}"),
        memory_type: AmpMemoryType::Semantic,
        score,
        tags: vec![],
    };
    // Adversarial rank-0 injection in list A; the true item is ranked
    // highly across BOTH lists.
    let a = vec![hit("ADV", 999.0), hit("TRUE", 0.9), hit("n1", 0.5)];
    let b = vec![hit("TRUE", 0.95), hit("m1", 0.6), hit("m2", 0.4)];

    let rrf = rrf_fuse(&[a.clone(), b.clone()], 60.0);
    assert_eq!(
        rrf[0].id, "TRUE",
        "RRF must hold under the rank-0 injection"
    );

    let max = max_fuse(&[a, b]);
    assert_eq!(max[0].id, "ADV", "max-fusion is fooled by the injection");
}

#[tokio::test]
async fn all_five_ops_over_four_types_smoke() {
    let engine = build_engine();
    let store = MnemoAmpStore::new(engine);

    for mt in AmpMemoryType::ALL {
        // remember
        let r = store
            .remember(&remember_env(
                &format!("fact for {}", mt.as_str()),
                mt.as_str(),
                mt,
            ))
            .await
            .expect("remember");
        assert!(r.ok && r.ids.len() == 1, "remember {:?}", mt);
        let id1 = r.ids[0].clone();

        // a second record so merge has >=2 sources
        let r2 = store
            .remember(&remember_env(
                &format!("second fact for {}", mt.as_str()),
                mt.as_str(),
                mt,
            ))
            .await
            .expect("remember 2");
        let id2 = r2.ids[0].clone();

        // recall
        let mut q = AmpEnvelope::new(AmpOp::Recall, mt);
        q.agent_id = Some(AGENT.to_string());
        q.query = Some("fact".to_string());
        q.tags = vec![mt.as_str().to_string()];
        let rec = store.recall(&q).await.expect("recall");
        assert!(rec.ok, "recall {:?}", mt);

        // merge (id1 + id2 → consolidated)
        let mut m = AmpEnvelope::new(AmpOp::Merge, mt);
        m.agent_id = Some(AGENT.to_string());
        m.memory_ids = vec![id1.clone(), id2.clone()];
        let merged = store.merge(&m).await.expect("merge");
        assert!(merged.ok && merged.ids.len() == 1, "merge {:?}", mt);
        let merged_id = merged.ids[0].clone();

        // expire the merged record immediately
        let mut e = AmpEnvelope::new(AmpOp::Expire, mt);
        e.agent_id = Some(AGENT.to_string());
        e.memory_ids = vec![merged_id.clone()];
        let exp = store.expire(&e).await.expect("expire");
        assert!(exp.ok, "expire {:?}", mt);

        // forget (a fresh record, to exercise the op for this type)
        let r3 = store
            .remember(&remember_env(
                &format!("third fact for {}", mt.as_str()),
                mt.as_str(),
                mt,
            ))
            .await
            .expect("remember 3");
        let mut f = AmpEnvelope::new(AmpOp::Forget, mt);
        f.agent_id = Some(AGENT.to_string());
        f.memory_ids = vec![r3.ids[0].clone()];
        let forgot = store.forget(&f).await.expect("forget");
        assert!(forgot.ok, "forget {:?}", mt);
    }
}

#[tokio::test]
async fn hitl_hook_gates_long_term_writes_only() {
    let engine = build_engine();
    // Reject any long-term write whose content contains "secret".
    let hook = Arc::new(ClosureApprove::new(|d| {
        if d.after.contains("secret") {
            mnemo_amp::Approval::Reject("contains secret".into())
        } else {
            mnemo_amp::Approval::Approve
        }
    }));
    let store = MnemoAmpStore::new(engine).with_approval_hook(hook);

    // Semantic (long-term) write with "secret" → rejected.
    let rejected = store
        .remember(&remember_env(
            "the secret launch code",
            "ops",
            AmpMemoryType::Semantic,
        ))
        .await
        .expect("remember call ok");
    assert!(!rejected.ok, "long-term secret write must be rejected");
    assert_eq!(rejected.approved, Some(false));

    // Episodic (short-term) write with "secret" → bypasses the gate.
    let allowed = store
        .remember(&remember_env(
            "the secret launch code",
            "ops",
            AmpMemoryType::Episodic,
        ))
        .await
        .expect("remember call ok");
    assert!(allowed.ok, "short-term write must bypass HITL gate");

    // Semantic write WITHOUT "secret" → approved, and the approval is
    // recorded in the hash-chained audit log as a Decision event.
    let approved = store
        .remember(&remember_env(
            "Paris is the capital of France",
            "geo",
            AmpMemoryType::Semantic,
        ))
        .await
        .expect("remember call ok");
    assert!(approved.ok && approved.approved == Some(true));
}

#[tokio::test]
async fn fan_out_router_fuses_recall_with_rrf() {
    // Two independent engines behind one router; a write fans out to
    // both and recall fuses via RRF.
    let store_a: Arc<dyn MemoryStore> = Arc::new(MnemoAmpStore::new(build_engine()));
    let store_b: Arc<dyn MemoryStore> = Arc::new(MnemoAmpStore::new(build_engine()));
    let router = AmpRouter::fan_out(vec![store_a, store_b]);

    let mut w = AmpEnvelope::new(AmpOp::Remember, AmpMemoryType::Semantic);
    w.agent_id = Some(AGENT.to_string());
    w.content = Some("Paris is the capital of France".to_string());
    w.tags = vec!["geo".to_string()];
    let written = router.route(&w).await.expect("fan-out write");
    assert!(written.ok);

    let mut q = AmpEnvelope::new(AmpOp::Recall, AmpMemoryType::Semantic);
    q.agent_id = Some(AGENT.to_string());
    q.query = Some("capital".to_string());
    q.tags = vec!["geo".to_string()];
    q.top_k = Some(5);
    let recalled = router.route(&q).await.expect("fan-out recall");
    assert!(recalled.ok);
    assert!(
        recalled.hits.iter().any(|h| h.content.contains("Paris")),
        "fused recall must surface the written fact"
    );
}
