//! AMP conformance smoke binary — runs the 5 AMP ops end-to-end
//! against the embedded DuckDB backend and the RRF-vs-max fusion check,
//! printing a PASS/FAIL line per check. Exit code `0` iff every check
//! passes.
//!
//! ```bash
//! cargo run --release --bin amp_conformance -p mnemo-amp
//! ```

use std::sync::Arc;

use mnemo_amp::{
    AmpEnvelope, AmpHit, AmpMemoryType, AmpOp, MemoryStore, MnemoAmpStore, max_fuse, rrf_fuse,
};
use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

const AGENT: &str = "amp-smoke";

fn build_engine() -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("duckdb open"));
    let index = Arc::new(UsearchIndex::new(3).expect("usearch new"));
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy open"));
    Arc::new(
        MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft),
    )
}

#[tokio::main]
async fn main() {
    let mut failures = 0usize;
    let mut check = |name: &str, ok: bool| {
        println!("{} {name}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            failures += 1;
        }
    };

    let engine = build_engine();
    let store = MnemoAmpStore::new(engine);

    // --- remember (semantic) ---
    let mut env = AmpEnvelope::new(AmpOp::Remember, AmpMemoryType::Semantic);
    env.agent_id = Some(AGENT.to_string());
    env.content = Some("Paris is the capital of France".to_string());
    env.tags = vec!["geo".to_string()];
    let r = store.remember(&env).await.expect("remember");
    check("remember.semantic", r.ok && r.ids.len() == 1);
    let id1 = r.ids[0].clone();

    let mut env2 = AmpEnvelope::new(AmpOp::Remember, AmpMemoryType::Semantic);
    env2.agent_id = Some(AGENT.to_string());
    env2.content = Some("Berlin is the capital of Germany".to_string());
    env2.tags = vec!["geo".to_string()];
    let r2 = store.remember(&env2).await.expect("remember 2");
    let id2 = r2.ids[0].clone();

    // --- recall@5 ---
    let mut q = AmpEnvelope::new(AmpOp::Recall, AmpMemoryType::Semantic);
    q.agent_id = Some(AGENT.to_string());
    q.query = Some("capital".to_string());
    q.tags = vec!["geo".to_string()];
    q.top_k = Some(5);
    let rec = store.recall(&q).await.expect("recall");
    check(
        "recall.top5",
        rec.ok && rec.hits.len() <= 5 && rec.hits.iter().any(|h| h.content.contains("Paris")),
    );

    // --- merge (id1 + id2) ---
    let mut m = AmpEnvelope::new(AmpOp::Merge, AmpMemoryType::Semantic);
    m.agent_id = Some(AGENT.to_string());
    m.memory_ids = vec![id1, id2];
    let merged = store.merge(&m).await.expect("merge");
    check("merge.compose", merged.ok && merged.ids.len() == 1);
    let merged_id = merged.ids[0].clone();

    // --- expire (immediate) ---
    let mut e = AmpEnvelope::new(AmpOp::Expire, AmpMemoryType::Semantic);
    e.agent_id = Some(AGENT.to_string());
    e.memory_ids = vec![merged_id];
    let exp = store.expire(&e).await.expect("expire");
    check("expire.immediate", exp.ok);

    // --- forget ---
    let mut env3 = AmpEnvelope::new(AmpOp::Remember, AmpMemoryType::Episodic);
    env3.agent_id = Some(AGENT.to_string());
    env3.content = Some("ephemeral note".to_string());
    let r3 = store.remember(&env3).await.expect("remember 3");
    let mut f = AmpEnvelope::new(AmpOp::Forget, AmpMemoryType::Episodic);
    f.agent_id = Some(AGENT.to_string());
    f.memory_ids = vec![r3.ids[0].clone()];
    let forgot = store.forget(&f).await.expect("forget");
    check("forget.soft_delete", forgot.ok && forgot.ids.len() == 1);

    // --- RRF holds under rank-0 injection; max-fusion is fooled ---
    let hit = |id: &str, score: f32| AmpHit {
        id: id.to_string(),
        content: format!("c-{id}"),
        memory_type: AmpMemoryType::Semantic,
        score,
        tags: vec![],
    };
    let la = vec![hit("ADV", 999.0), hit("TRUE", 0.9), hit("n1", 0.5)];
    let lb = vec![hit("TRUE", 0.95), hit("m1", 0.6), hit("m2", 0.4)];
    let rrf = rrf_fuse(&[la.clone(), lb.clone()], 60.0);
    let max = max_fuse(&[la, lb]);
    check(
        "fusion.rrf_robust_vs_max_fooled",
        rrf[0].id == "TRUE" && max[0].id == "ADV",
    );

    println!("---");
    if failures == 0 {
        println!("AMP conformance: ALL CHECKS PASS");
        std::process::exit(0);
    } else {
        eprintln!("AMP conformance: {failures} check(s) FAILED");
        std::process::exit(1);
    }
}
