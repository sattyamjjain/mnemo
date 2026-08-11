//! PostgreSQL parity for write provenance + FORGET BY PROVENANCE.
//!
//! Gated at runtime on `MNEMO_TEST_POSTGRES_URL`: without it the test **skips
//! (passes)** so `cargo test --workspace` stays green with no database. With a
//! live Postgres:
//!
//! ```bash
//! MNEMO_TEST_POSTGRES_URL=postgres://postgres:pw@localhost:5432/postgres \
//!   cargo test -p mnemo-postgres --test write_provenance_pg -- --nocapture
//! ```
//!
//! `PgStorage::connect` runs the schema migration, so this also exercises the
//! `write_provenance` migration on Postgres (the additive "up" direction).

use std::sync::Arc;

use async_trait::async_trait;
use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::Result as MnResult;
use mnemo_core::model::capability::CapabilityIssuer;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::forget::ForgetStrategy;
use mnemo_core::query::remember::RememberRequest;
use mnemo_postgres::{PgStorage, PgVectorIndex};

const DIM: usize = 4;

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

fn req(content: &str, created_by: &str, thread_id: &str) -> RememberRequest {
    let mut r = RememberRequest::new(content.to_string());
    r.created_by = Some(created_by.to_string());
    r.thread_id = Some(thread_id.to_string());
    r
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pg_write_provenance_query_and_forget() {
    let Ok(url) = std::env::var("MNEMO_TEST_POSTGRES_URL") else {
        eprintln!("skipping PG provenance test: set MNEMO_TEST_POSTGRES_URL to run it");
        return;
    };
    let storage = Arc::new(
        PgStorage::connect(&url, DIM)
            .await
            .expect("connect + run migrations"),
    );

    // Unique principals/session per run so the shared DB does not collide.
    let tag = uuid::Uuid::now_v7().to_string();
    let alice = format!("alice-{tag}");
    let bob = format!("bob-{tag}");
    let sess = format!("sess-{tag}");

    let index = Arc::new(PgVectorIndex::with_pool(storage.pool(), DIM));
    let issuer = Arc::new(CapabilityIssuer::new("pg-cap", &[3u8; 32]));
    let engine = Arc::new(
        MnemoEngine::new(
            storage.clone(),
            index,
            Arc::new(ConstEmbedding),
            "pg-agent".to_string(),
            None,
        )
        .with_capability_issuer(issuer),
    );

    let a1 = engine.remember(req("m1", &alice, &sess)).await.unwrap();
    engine.remember(req("m2", &alice, &sess)).await.unwrap();
    let b1 = engine.remember(req("m3", &bob, &sess)).await.unwrap();

    // provenance by memory id
    let p = engine
        .write_provenance_for(a1.id)
        .await
        .unwrap()
        .expect("provenance recorded");
    assert_eq!(p.principal, alice);
    assert!(p.content_hash_valid(), "content hash must verify");

    // provenance by principal
    let alice_writes = engine.writes_by_principal(&alice, 100).await.unwrap();
    assert_eq!(alice_writes.len(), 2);
    assert!(alice_writes.iter().all(|r| r.principal == alice));

    // FORGET BY PROVENANCE: revoke alice's writes, keep bob's.
    let resp = engine
        .forget_by_principal(&alice, ForgetStrategy::HardDelete)
        .await
        .unwrap();
    assert_eq!(resp.forgotten.len(), 2);
    assert!(engine.storage.get_memory(a1.id).await.unwrap().is_none());
    assert!(
        engine.storage.get_memory(b1.id).await.unwrap().is_some(),
        "bob's memory must survive forget-by-principal(alice)"
    );
}
