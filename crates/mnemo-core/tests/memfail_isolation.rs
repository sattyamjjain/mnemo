//! End-to-end integration test for the MemFail-style fault-isolation
//! harness in [`mnemo_core::eval::memfail`].
//!
//! The harness's three per-operation probe sets and the canonical
//! stale-context fixture are exercised against a real
//! `MnemoEngine` (in-memory DuckDB + USearch + Tantivy + NoopEmbedding)
//! and the attribution shape is asserted.

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::eval::memfail::{
    Stage, run_retrieve_probes, run_stale_context_fixture, run_store_probes, run_summarize_probes,
};
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

const AGENT: &str = "memfail-itest-agent";

fn build_engine() -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("duckdb open"));
    let index = Arc::new(UsearchIndex::new(3).expect("usearch new"));
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy open"));
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft)
}

#[tokio::test]
async fn three_stage_probes_each_pass_independently() {
    let engine = build_engine();
    let store = run_store_probes(&engine, AGENT).await.expect("store");
    let summarize = run_summarize_probes(&engine, AGENT)
        .await
        .expect("summarize");
    let retrieve = run_retrieve_probes(&engine, AGENT).await.expect("retrieve");

    assert!(
        store.passed(),
        "store probes must pass on a well-formed engine; failing: {:?}",
        store.failing_probes()
    );
    assert!(
        summarize.passed(),
        "summarize probes must pass on a well-formed engine; failing: {:?}",
        summarize.failing_probes()
    );
    assert!(
        retrieve.passed(),
        "retrieve probes must pass on a well-formed engine; failing: {:?}",
        retrieve.failing_probes()
    );

    // Each stage carries the expected probe count: 3 + 3 + 2.
    assert_eq!(store.probes.len(), 3);
    assert_eq!(summarize.probes.len(), 3);
    assert_eq!(retrieve.probes.len(), 2);
}

#[tokio::test]
async fn stale_context_recall_is_attributed_to_retrieve_not_summarize() {
    let engine = build_engine();
    let report = run_stale_context_fixture(&engine, AGENT)
        .await
        .expect("stale fixture ran");

    // Upstream probes must have all passed — otherwise the harness
    // could not isolate one stage and the test offers no signal.
    assert!(
        report.store_report.passed(),
        "store stage must be clean before we trust attribution: {:?}",
        report.store_report.failing_probes()
    );
    assert!(
        report.summarize_report.passed(),
        "summarize stage must be clean before we trust attribution: {:?}",
        report.summarize_report.failing_probes()
    );
    assert!(
        report.isolated,
        "stale-context fixture must isolate one stage: {report:?}"
    );

    // The canonical MemFail "isolate the operation" output: stale
    // recall attributed to retrieve, NOT to summarize.
    assert_eq!(
        report.attributed_stage,
        Stage::Retrieve,
        "stale-context recall must be attributed to retrieve, observed {:?}: {report:?}",
        report.attributed_stage,
    );
    assert_ne!(
        report.attributed_stage,
        Stage::Summarize,
        "summarize must NOT be blamed: no consolidation ran in this fixture"
    );

    // Evidence must include the recall.top_id field so an operator
    // reading the report knows which record won the ranking.
    assert!(
        report
            .evidence
            .iter()
            .any(|e| e.starts_with("recall.top_id")),
        "evidence missing recall.top_id row: {:?}",
        report.evidence
    );
}
