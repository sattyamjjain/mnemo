//! Postgres semantic recall must fail loud, never return an empty result set.
//!
//! # What this pins, and why it is not the unit test next door
//!
//! `crates/mnemo-postgres/src/pgvector_index.rs` already unit-tests that a
//! pool-less [`PgVectorIndex`] returns [`Error::BackendUnsupported`] from
//! `search` / `filtered_search`. That is the *index* contract.
//!
//! This test pins the **caller-visible** contract one layer up: that a
//! `semantic` recall issued through [`MnemoEngine`] surfaces that error to the
//! caller instead of flattening it into `Ok(vec![])`. Those are different
//! properties, and the second is the one that actually bites. `recall.rs`
//! reaches the index through four separate call sites (`semantic`, `auto`,
//! `graph`, `domain_scoped`); every one of them currently uses `.await?`, and a
//! single `.unwrap_or_default()` slipped into any of them would restore the
//! original bug — a caller receiving "no matches" when the truth is "this
//! backend cannot do that". A memory database that answers "nothing found" to a
//! question it cannot answer is worse than one that crashes, because the caller
//! writes the empty result down as a fact.
//!
//! # Why the store is deliberately non-empty
//!
//! The fixture writes a record before recalling. An empty store makes
//! `Ok(vec![])` and "not implemented" indistinguishable, which is exactly the
//! ambiguity under test. With a record present, an empty `Ok` can only mean the
//! error was swallowed.
//!
//! # Why DuckDB storage under a Postgres index
//!
//! The failure mode lives in the recall path's handling of a `VectorIndex`
//! error, not in Postgres row storage. Pairing in-memory DuckDB storage with the
//! real, pool-less `PgVectorIndex` reproduces it with **no live database**, so
//! this runs on every `cargo test --workspace` rather than skipping whenever
//! `MNEMO_TEST_POSTGRES_URL` is unset — which is precisely the condition under
//! which a silent-empty regression would otherwise reach a release.
//!
//! Live-database ANN behaviour is covered by `tests/pgvector_ann.rs`.

use std::sync::Arc;

use mnemo_core::embedding::DeterministicEmbedding;
use mnemo_core::error::Error;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_postgres::PgVectorIndex;

const DIM: usize = 8;
const AGENT: &str = "pg-fail-loud";

/// Engine wired to the real pool-less `PgVectorIndex` — the shape a caller gets
/// when the Postgres backend is selected but pgvector is not actually reachable.
///
/// Returns the engine and the id of the one record written, so a caller can
/// prove the store is non-empty without going through *any* recall strategy.
async fn engine_with_poolless_pgvector() -> (Arc<MnemoEngine>, uuid::Uuid) {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("in-memory duckdb"));
    let index = Arc::new(PgVectorIndex::new());
    let engine = Arc::new(MnemoEngine::new(
        storage,
        index,
        Arc::new(DeterministicEmbedding::new(DIM)),
        AGENT.to_string(),
        None,
    ));

    // Non-empty store: an empty `Ok` afterwards can only be a swallowed error.
    let id = engine
        .remember(RememberRequest::new(
            "a record that definitely exists".to_string(),
        ))
        .await
        .expect("remember must succeed — only the ANN read path is unsupported")
        .id;

    (engine, id)
}

fn assert_backend_unsupported(strategy: &str, result: Result<impl std::fmt::Debug, Error>) {
    match result {
        Ok(v) => panic!(
            "strategy=`{strategy}` returned Ok({v:?}) on a backend that cannot do ANN. \
             This is the silent-empty regression: the caller cannot tell \"no matches\" \
             from \"not implemented\", and will record the empty answer as a fact. \
             The recall path must propagate the index error (`.await?`), never \
             `.unwrap_or_default()` it."
        ),
        Err(Error::BackendUnsupported {
            backend,
            capability,
            detail,
        }) => {
            assert_eq!(
                backend, "postgres",
                "strategy=`{strategy}`: the error must name the backend so a caller \
                 knows which one is unsupported"
            );
            assert_eq!(
                capability, "semantic_recall",
                "strategy=`{strategy}`: the error must name the unsupported operation"
            );
            assert!(
                !detail.is_empty(),
                "strategy=`{strategy}`: the error must carry actionable detail"
            );
        }
        Err(other) => panic!(
            "strategy=`{strategy}`: expected the structured BackendUnsupported variant \
             (callers match on backend/capability rather than sniffing strings); got: {other}"
        ),
    }
}

/// Every recall strategy that reaches the vector index must error, not return
/// an empty set. These are the four `filtered_search` call sites in `recall.rs`.
#[tokio::test]
async fn semantic_recall_strategies_error_and_never_return_empty() {
    let (engine, _id) = engine_with_poolless_pgvector().await;

    for strategy in ["semantic", "auto", "graph", "domain_scoped"] {
        let mut req = RecallRequest::new("a record that definitely exists".to_string());
        req.strategy = Some(strategy.to_string());
        let result = engine.recall(req).await.map(|r| r.memories.len());
        assert_backend_unsupported(strategy, result);
    }
}

/// The same guarantee on a single-threaded runtime. `VectorIndex::search` went
/// async in v0.5.18 precisely because the old `block_on` bridge panicked here;
/// a future refactor that reintroduces a bridge must not be able to turn that
/// panic into a swallowed empty result either.
#[tokio::test(flavor = "current_thread")]
async fn semantic_recall_fails_loud_on_current_thread_runtime() {
    let (engine, _id) = engine_with_poolless_pgvector().await;

    let mut req = RecallRequest::new("a record that definitely exists".to_string());
    req.strategy = Some("semantic".to_string());
    let result = engine.recall(req).await.map(|r| r.memories.len());
    assert_backend_unsupported("semantic", result);
}

/// Control: the store really does hold the record, so the failures above are
/// about the ANN path and not about an empty fixture.
///
/// This reads straight from storage rather than through any recall strategy, on
/// purpose. A recall-based control would couple this proof to whichever
/// strategies happen to work on a bare engine — `lexical`, for instance,
/// returns empty here because no Tantivy full-text index is attached, which
/// says nothing about whether the record was written.
///
/// Without this control, every assertion above would still pass if `remember`
/// silently wrote nothing: the test would be measuring an empty database and
/// reporting success.
#[tokio::test]
async fn control_the_fixture_store_is_not_empty() {
    let (engine, id) = engine_with_poolless_pgvector().await;

    let record = engine
        .storage
        .get_memory(id)
        .await
        .expect("storage read must succeed");
    assert!(
        record.is_some(),
        "control failed: the fixture store has no record at {id}, so the \
         BackendUnsupported assertions above prove nothing about silent-empty"
    );
}
