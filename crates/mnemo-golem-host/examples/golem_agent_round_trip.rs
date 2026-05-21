//! v0.4.6 — end-to-end REMEMBER → RECALL → DELETE round-trip
//! showing a golem agent driving the `mnemo-golem-wit` provider
//! through the `MnemoGolemHost` Rust API.
//!
//! Today's vertical slice exercises the *Rust-native* trait shape
//! ([`MnemoGolemProvider`]). The wasmtime-component-loader wiring
//! that bridges the bindgen-generated host bindings to this trait
//! is documented as a v0.5.x follow-up in
//! [`docs/research/golem-vector-wit-provider.md`](../../../docs/research/golem-vector-wit-provider.md);
//! today's example demonstrates the full mnemo round trip
//! through the *Rust* shape, which is byte-identical to what the
//! wasmtime host will eventually dispatch into.
//!
//! Run:
//!     cargo run --example golem_agent_round_trip -p mnemo-golem-host

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::storage::duckdb::DuckDbStorage;

use mnemo_golem_host::{MnemoGolemHost, MnemoGolemProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Stand up an mnemo engine. In production this is the same
    //    `MnemoEngine` the rest of mnemo's MCP server / REST API /
    //    gRPC service shares — the golem-vector provider becomes
    //    *one of many* surfaces over the same substrate.
    let storage = Arc::new(DuckDbStorage::open_in_memory()?);
    let index = Arc::new(UsearchIndex::new(8)?);
    let embedding = Arc::new(NoopEmbedding::new(8));
    let engine = Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        "golem-agent-demo".to_string(),
        None,
    ));
    let host = MnemoGolemHost::new(engine);

    // 2. The golem agent issues REMEMBER (upsert-vector) calls. In
    //    a real deployment the WASM component would call
    //    `host-upsert(...)` and this Rust path would execute on
    //    the wasmtime host side.
    println!("=== REMEMBER (upsert-vector × 3) ===");
    let docs = [
        ("doc-a", vec![0.10_f32; 8]),
        ("doc-b", vec![0.40_f32; 8]),
        ("doc-c", vec![0.90_f32; 8]),
    ];
    for (id, vector) in &docs {
        host.upsert_vector("agent-demo".to_string(), id.to_string(), vector.clone())
            .await?;
        println!("  upserted golem id={id}");
    }

    // 3. The agent issues RECALL (search-vectors).
    println!("\n=== RECALL (search-vectors with limit=5) ===");
    let hits = host
        .search_vectors("agent-demo", vec![0.15_f32; 8], 5)
        .await?;
    for (id, score) in &hits {
        println!("  hit golem id={id:<6}  score={score:.4}");
    }
    if hits.is_empty() {
        println!("  (no hits returned)");
    }

    // 4. The agent issues DELETE (delete-vectors) on two of the
    //    three.
    println!("\n=== DELETE (delete-vectors id=[doc-a, doc-b]) ===");
    let removed = host
        .delete_vectors("agent-demo", vec!["doc-a".to_string(), "doc-b".to_string()])
        .await?;
    println!("  removed = {removed} record(s)");

    // 5. Re-RECALL — should only see doc-c.
    println!("\n=== RECALL after delete (limit=5) ===");
    let post_hits = host
        .search_vectors("agent-demo", vec![0.15_f32; 8], 5)
        .await?;
    for (id, score) in &post_hits {
        println!("  hit golem id={id:<6}  score={score:.4}");
    }

    println!(
        "\nround-trip complete. Substrate exercised: 3 upserts, 1 search, 1 delete (2 removed), 1 post-delete search.\nUnderlying mnemo path: RememberRequest → recall::execute → ForgetRequest with HardDelete strategy."
    );
    Ok(())
}
