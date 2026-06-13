//! v0.4.0-rc3 (Task B3) — LongMemEval_M-shaped recall bench with a
//! `--with-provenance` toggle.
//!
//! The repo ships a small synthesized dataset under
//! `benches/data/longmemeval_m.jsonl` so the bench is hermetic and
//! self-contained. The shape mirrors LongMemEval_M (multi-turn
//! medical dialogues, ~3 turns per conversation, ~15 conversations)
//! without redistributing the published gated dataset. Real-dataset
//! runs swap the file via `MNEMO_LONGMEMEVAL_PATH=<path>`.
//!
//! Two criterion groups run by default:
//!
//! 1. `longmemeval/recall_no_provenance` — baseline recall latency
//!    against the seeded dataset.
//! 2. `longmemeval/recall_with_provenance` — same workload with a
//!    `ProvenanceSigner` attached and `with_provenance=Some(true)`,
//!    so each recall returns a verifiable HMAC receipt (B1).
//!
//! The delta between groups is the per-recall provenance overhead
//! published in CHANGELOG release notes.

use std::path::PathBuf;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use serde::Deserialize;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::provenance::ProvenanceSigner;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::duckdb::DuckDbStorage;

#[derive(Debug, Deserialize)]
struct LongMemRecord {
    id: String,
    conversation_id: String,
    turn: u32,
    content: String,
    tags: Vec<String>,
    query: String,
    #[allow(dead_code)]
    expected: String,
}

fn dataset_path() -> PathBuf {
    if let Ok(p) = std::env::var("MNEMO_LONGMEMEVAL_PATH") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches")
        .join("data")
        .join("longmemeval_m.jsonl")
}

fn load_dataset() -> Vec<LongMemRecord> {
    let path = dataset_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read longmemeval dataset at {path:?}: {e}"));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str::<LongMemRecord>(l)
                .unwrap_or_else(|e| panic!("invalid LongMem record: {e}; line: {l}"))
        })
        .collect()
}

/// 32-byte HMAC key for the provenance arm. Hard-coded because this
/// is a benchmark — production keystore is the operator's
/// responsibility (B2 hardened mode).
const BENCH_HMAC_KEY: &[u8; 32] = b"mnemo-longmem-bench-key-32bytes!";

fn make_engine_with_provenance(with_signer: bool) -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(3).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(3));
    let mut eng = MnemoEngine::new(
        storage,
        index,
        embedding,
        "longmemeval-bench-agent".to_string(),
        None,
    );
    if with_signer {
        let signer = ProvenanceSigner::new("longmem-bench-key", BENCH_HMAC_KEY);
        eng = eng.with_provenance_signer(Arc::new(signer));
    }
    eng
}

fn seed(engine: &MnemoEngine, dataset: &[LongMemRecord], rt: &tokio::runtime::Runtime) {
    rt.block_on(async {
        for r in dataset {
            let req = RememberRequest {
                content: r.content.clone(),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: Some(0.5),
                tags: Some(r.tags.clone()),
                metadata: Some(serde_json::json!({
                    "conversation_id": r.conversation_id,
                    "turn": r.turn,
                    "lme_id": r.id,
                })),
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: Some(r.conversation_id.clone()),
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            };
            engine.remember(req).await.unwrap();
        }
    });
}

fn build_recall(query: &str, with_provenance: Option<bool>) -> RecallRequest {
    RecallRequest {
        query: query.to_string(),
        agent_id: None,
        limit: Some(5),
        memory_type: None,
        memory_types: None,
        scope: None,
        min_importance: None,
        tags: None,
        org_id: None,
        strategy: Some("semantic".to_string()),
        temporal_range: None,
        recency_half_life_hours: None,
        hybrid_weights: None,
        rrf_k: None,
        as_of: None,
        explain: None,
        with_provenance,
        mode: None,
        current_fact_resolver: None,
        orientation_cache: None,
        evidence_budget: None,
        retained_token_budget: None,
        domain_scope: None,
    }
}

fn longmemeval_recall(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dataset = load_dataset();
    assert!(
        !dataset.is_empty(),
        "longmemeval dataset is empty — check MNEMO_LONGMEMEVAL_PATH or the bundled file"
    );

    // Arm 1: baseline (no provenance signer attached, no receipt
    // requested). This is what a v0.3.x deployment looks like.
    let engine_plain = make_engine_with_provenance(false);
    seed(&engine_plain, &dataset, &rt);
    let mut group = c.benchmark_group("longmemeval");
    let queries: Vec<String> = dataset.iter().map(|r| r.query.clone()).collect();

    let mut q_iter = queries.iter().cycle();
    group.bench_function("recall_no_provenance", |b| {
        b.iter(|| {
            let q = q_iter.next().unwrap();
            rt.block_on(async {
                engine_plain.recall(build_recall(q, None)).await.unwrap();
            });
        });
    });

    // Arm 2: provenance signer attached + with_provenance=Some(true).
    // This is what a v0.4.0-rc3 hardened deployment looks like.
    let engine_signed = make_engine_with_provenance(true);
    seed(&engine_signed, &dataset, &rt);
    let mut q_iter2 = queries.iter().cycle();
    group.bench_function("recall_with_provenance", |b| {
        b.iter(|| {
            let q = q_iter2.next().unwrap();
            rt.block_on(async {
                let resp = engine_signed
                    .recall(build_recall(q, Some(true)))
                    .await
                    .unwrap();
                // Sanity: the receipt must be present in arm 2 so we
                // are actually paying the HMAC cost. A None here
                // means the bench is silently skipping signing —
                // refuse to publish numbers from that.
                assert!(
                    resp.provenance.is_some(),
                    "with_provenance arm returned no receipt — bench would mis-report overhead"
                );
            });
        });
    });

    group.finish();
}

criterion_group!(benches, longmemeval_recall);
criterion_main!(benches);
