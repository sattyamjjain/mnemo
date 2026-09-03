//! What does the per-key chain lock cost?
//!
//! ```bash
//! cargo run --release -p mnemo-core --example concurrent_append_throughput
//! ```
//!
//! `append_memory_chained` / `append_event_chained` serialise the read of the
//! chain head and the insert that names it, per `(chain, agent_id, thread_id)`.
//! That is a correctness requirement — without it concurrent writes fork the
//! chain — but serialisation is exactly the kind of fix that can quietly cost an
//! order of magnitude, so the cost is measured rather than assumed.
//!
//! Three shapes, same total work (16 writes), so the numbers are comparable:
//!
//! * **contended**  — 16 concurrent writers, ONE key. Every writer queues behind
//!   every other. This is the worst case and the shape of the reproduction test.
//! * **sharded**    — 16 concurrent writers, 16 DISTINCT keys. Nothing should
//!   queue; this is what the sharding buys, and it is the shape a multi-agent
//!   deployment actually has.
//! * **serial**     — 16 writes one after another, no concurrency. The floor
//!   that "serialised" is often assumed to mean.
//!
//! Reported as median-of-R wall time and writes/sec. Medians, not means: one
//! slow run (a page fault, the index growing) should not move the headline.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mnemo_core::embedding::DeterministicEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::duckdb::DuckDbStorage;

const WRITERS: usize = 16;
const REPEATS: usize = 9;

fn engine() -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("duckdb"));
    let index = Arc::new(UsearchIndex::new(64).expect("index"));
    let embedding = Arc::new(DeterministicEmbedding::new(64));
    Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        "throughput".to_string(),
        None,
    ))
}

/// `agent` per writer: `Some(i)` gives every writer its own key (sharded),
/// `None` puts them all on one key (contended).
async fn concurrent(distinct_keys: bool) -> Duration {
    let engine = engine();
    let t0 = Instant::now();
    let mut tasks = Vec::with_capacity(WRITERS);
    for i in 0..WRITERS {
        let engine = Arc::clone(&engine);
        tasks.push(tokio::spawn(async move {
            let mut r = RememberRequest::new(format!("write {i}"));
            r.agent_id = Some(if distinct_keys {
                format!("agent-{i}")
            } else {
                "agent-shared".to_string()
            });
            engine.remember(r).await.expect("remember")
        }));
    }
    for t in tasks {
        t.await.expect("join");
    }
    t0.elapsed()
}

async fn serial() -> Duration {
    let engine = engine();
    let t0 = Instant::now();
    for i in 0..WRITERS {
        let mut r = RememberRequest::new(format!("write {i}"));
        r.agent_id = Some("agent-shared".to_string());
        engine.remember(r).await.expect("remember");
    }
    t0.elapsed()
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort_unstable();
    v[v.len() / 2]
}

fn report(label: &str, d: Duration) {
    let per_sec = WRITERS as f64 / d.as_secs_f64();
    println!(
        "{label:<12} {:>9.2} ms   {per_sec:>9.0} writes/sec",
        d.as_secs_f64() * 1000.0
    );
}

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() {
    // One untimed pass so the first measurement does not also pay for lazy
    // initialisation inside DuckDB and the index.
    let _ = concurrent(false).await;

    let mut contended = Vec::new();
    let mut sharded = Vec::new();
    let mut ser = Vec::new();
    for _ in 0..REPEATS {
        contended.push(concurrent(false).await);
        sharded.push(concurrent(true).await);
        ser.push(serial().await);
    }

    println!("{WRITERS} writes, median of {REPEATS} runs\n");
    report("contended", median(contended.clone()));
    report("sharded", median(sharded.clone()));
    report("serial", median(ser.clone()));

    let c = median(contended).as_secs_f64();
    let s = median(sharded).as_secs_f64();
    let q = median(ser).as_secs_f64();
    println!();
    println!(
        "contended / sharded = {:.2}x  (what one-key contention costs)",
        c / s
    );
    println!(
        "contended / serial  = {:.2}x  (>1 means the lock made it slower than not being concurrent at all)",
        c / q
    );
}
