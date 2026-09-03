//! PostgreSQL parity for the concurrency reproduction in
//! `crates/mnemo-core/tests/concurrent_chain_linkage.rs`.
//!
//! The DuckDB and PostgreSQL backends fix the same defect with different
//! mechanisms — an in-process sharded lock versus `pg_advisory_xact_lock` — so
//! passing on one proves nothing about the other. This is the PostgreSQL half.
//!
//! Gated at runtime on `MNEMO_TEST_POSTGRES_URL`: without it the test **skips
//! (passes)** so `cargo test --workspace` stays green with no database. It says
//! so on stdout rather than passing silently, because a skipped check that reads
//! as a pass is a failure mode this repository has been bitten by more than once.
//!
//! ```bash
//! MNEMO_TEST_POSTGRES_URL=postgres://postgres:pw@localhost:5432/postgres \
//!   cargo test -p mnemo-postgres --test concurrent_chain_linkage_pg -- --nocapture
//! ```
//!
//! One thing this does **not** prove: that the advisory lock orders appends
//! across *processes*. Every task here shares one `PgPool` in one process. What
//! it does prove is that the ordering is done by the database rather than by a
//! process-local mutex — there is no in-process lock in the PostgreSQL backend
//! for these tasks to be accidentally serialised by.

use std::sync::Arc;

use async_trait::async_trait;
use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::Result as MnResult;
use mnemo_core::hash::compute_chain_hash;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::remember::RememberRequest;
use mnemo_postgres::{PgStorage, PgVectorIndex};

const DIM: usize = 4;
const WRITERS: usize = 16;

struct ConstEmbedding;

#[async_trait]
impl EmbeddingProvider for ConstEmbedding {
    async fn embed(&self, _t: &str) -> MnResult<Vec<f32>> {
        Ok(vec![1.0, 0.0, 0.0, 0.0])
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnResult<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0, 0.0, 0.0, 0.0]).collect())
    }
    fn dimensions(&self) -> usize {
        DIM
    }
}

/// One link, reduced to the fields that define it.
struct Link {
    label: String,
    content_hash: Vec<u8>,
    prev_hash: Option<Vec<u8>>,
}

/// Walk `links` as a chain. `Ok(())` when they form exactly one path covering
/// every element.
fn diagnose(links: &[Link]) -> Result<(), String> {
    let n = links.len();
    let is_head = |l: &Link| -> bool {
        l.prev_hash
            .as_deref()
            .is_some_and(|p| p == compute_chain_hash(&l.content_hash, None).as_slice())
    };
    let heads: Vec<_> = links.iter().filter(|l| is_head(l)).collect();
    if heads.len() != 1 {
        return Err(format!(
            "{} head(s) and {} linked record(s) across {n} concurrent write(s); expected 1 head \
             and {} links.\nheads: {}",
            heads.len(),
            n - heads.len(),
            n - 1,
            heads
                .iter()
                .map(|l| l.label.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut walked = 1usize;
    let mut cursor = heads[0].content_hash.clone();
    loop {
        let next: Vec<_> = links
            .iter()
            .filter(|l| {
                l.prev_hash.as_deref()
                    == Some(compute_chain_hash(&l.content_hash, Some(&cursor)).as_slice())
            })
            .collect();
        match next.len() {
            0 => break,
            1 => {
                walked += 1;
                cursor = next[0].content_hash.clone();
            }
            k => {
                return Err(format!(
                    "the chain forks: {k} records claim the same predecessor after {walked} step(s)"
                ));
            }
        }
    }
    if walked != n {
        return Err(format!(
            "the chain covers {walked} of {n} record(s); {} are unreachable from the head",
            n - walked
        ));
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn pg_concurrent_remember_produces_one_chain() {
    let Ok(url) = std::env::var("MNEMO_TEST_POSTGRES_URL") else {
        eprintln!(
            "SKIP: set MNEMO_TEST_POSTGRES_URL to exercise the PostgreSQL half of the \
             concurrency fix; the DuckDB half proves nothing about it"
        );
        return;
    };
    let storage = Arc::new(
        PgStorage::connect(&url, DIM)
            .await
            .expect("connect + run migrations"),
    );
    // A fresh agent per run: the CI database is shared and reused, and a chain
    // seeded by an earlier run would make "one head" a statement about history
    // rather than about this run.
    let agent = format!("concurrency-{}", uuid::Uuid::now_v7());

    let index = Arc::new(PgVectorIndex::with_pool(storage.pool(), DIM));
    let engine = Arc::new(MnemoEngine::new(
        storage.clone(),
        index,
        Arc::new(ConstEmbedding),
        agent.clone(),
        None,
    ));

    let mut tasks = Vec::with_capacity(WRITERS);
    for i in 0..WRITERS {
        let engine = Arc::clone(&engine);
        let agent = agent.clone();
        tasks.push(tokio::spawn(async move {
            let mut r = RememberRequest::new(format!("pg concurrent write #{i:02}"));
            r.agent_id = Some(agent);
            engine.remember(r).await
        }));
    }
    for (i, t) in tasks.into_iter().enumerate() {
        t.await
            .unwrap_or_else(|e| panic!("writer {i} panicked: {e}"))
            .unwrap_or_else(|e| panic!("writer {i} failed: {e}"));
    }

    // --- events ---
    let events = mnemo_core::storage::StorageBackend::list_events(&*storage, &agent, 10_000, 0)
        .await
        .expect("list events");
    let links: Vec<_> = events
        .iter()
        .map(|e| Link {
            label: e.id.to_string(),
            content_hash: e.content_hash.clone(),
            prev_hash: e.prev_hash.clone(),
        })
        .collect();
    assert_eq!(links.len(), WRITERS, "one MemoryWrite event per write");
    if let Err(d) = diagnose(&links) {
        panic!(
            "the agent_events chain is not a chain after {WRITERS} concurrent remember() calls \
             against PostgreSQL.\n\n{d}\n\n\
             The append is ordered by pg_advisory_xact_lock inside the inserting transaction, \
             and the tip is read from chain_heads. If this fails, one of those two is not doing \
             its job."
        );
    }

    // --- memories ---
    let records = mnemo_core::storage::StorageBackend::list_memories_by_agent_ordered(
        &*storage, &agent, None, 10_000,
    )
    .await
    .expect("list memories");
    let links: Vec<_> = records
        .iter()
        .map(|r| Link {
            label: r.id.to_string(),
            content_hash: r.content_hash.clone(),
            prev_hash: r.prev_hash.clone(),
        })
        .collect();
    assert_eq!(links.len(), WRITERS);
    if let Err(d) = diagnose(&links) {
        panic!("the memories chain is not a chain after {WRITERS} concurrent writes.\n\n{d}");
    }
}
