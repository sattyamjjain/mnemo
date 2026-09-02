//! Concurrent `remember()` calls for one `(agent_id, thread_id)` must produce a
//! single hash chain with exactly one head.
//!
//! # What this pins
//!
//! `remember()` reads the current chain head and then inserts. Between those two
//! points it does real work — TTL resolution, record construction, opaque-reasoning
//! detection, encryption. Two calls that overlap in that window both read the same
//! head and both write themselves as a head, so the log stops being a chain and
//! becomes a pile of unlinked records.
//!
//! That is not a hypothetical. `docs/verify-my-log.md` measured it against a real
//! MCP session: three `tools/call` requests produced three heads and zero links,
//! while the same three issued one per invocation produced `OK: 3 records, chain
//! intact`. The per-record content hashes still caught edits, but *ordering* — the
//! property that makes removal and reordering visible, and the only reason to
//! build a chain rather than a set of hashes — was absent.
//!
//! The assertions below are structural, not statistical: exactly one head, and
//! every other event reachable from it by following links, once. A chain that
//! forks fails. A chain that is missing an event fails. There is no threshold to
//! tune and no flake budget to spend.
//!
//! # Why `agent_events` and not `memories`
//!
//! Both chains have the same defect and the fix covers both, but the event log is
//! the one an auditor is handed. It is what `mnemo audit export` walks and what
//! `tools/verify_mnemo_chain.py` verifies. A memory can be soft-deleted; the event
//! recording that it was written cannot. The memory chain is asserted too, further
//! down, so neither can regress alone.

use std::collections::HashMap;
use std::sync::Arc;

use mnemo_core::embedding::DeterministicEmbedding;
use mnemo_core::hash::compute_chain_hash;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::event::EventType;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::duckdb::DuckDbStorage;

const AGENT: &str = "concurrency-probe";

/// Enough writers to overlap reliably on any CI runner, small enough that the
/// test stays sub-second.
const WRITERS: usize = 16;

/// Where the test database lives.
///
/// In-memory by default, so the suite leaves nothing behind. Set
/// `MNEMO_CONCURRENCY_DB` to a path to get a file instead — that is how the
/// database in the pull request was produced, so that
/// `tools/verify_mnemo_chain.py` could be pointed at something this test
/// actually wrote rather than at a hand-built fixture.
fn engine() -> Arc<MnemoEngine> {
    let storage: Arc<DuckDbStorage> = match std::env::var("MNEMO_CONCURRENCY_DB") {
        Ok(path) if !path.is_empty() => {
            Arc::new(DuckDbStorage::open(std::path::Path::new(&path)).expect("open duckdb file"))
        }
        _ => Arc::new(DuckDbStorage::open_in_memory().expect("open in-memory duckdb")),
    };
    let index = Arc::new(UsearchIndex::new(64).expect("index"));
    let embedding = Arc::new(DeterministicEmbedding::new(64));
    Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        AGENT.to_string(),
        None,
    ))
}

/// One link in the chain, reduced to the two fields that define it.
struct Link {
    label: String,
    /// Carried only so a failure can say whether the write timestamps were
    /// distinct.
    ///
    /// Finding the chain head by `ORDER BY <timestamp> DESC LIMIT 1` fails two
    /// ways, and a fork report is only actionable if it says which: tied
    /// timestamps leave the ordering ambiguous, and *distinct* ones still name
    /// the write that STARTED last rather than the one inserted last. The
    /// second is what actually bit — all 16 were distinct — and it is the one a
    /// reader will not think of.
    ts: String,
    content_hash: Vec<u8>,
    prev_hash: Option<Vec<u8>>,
}

/// Walk `links` as a chain and describe what is actually there.
///
/// Returns `Ok(())` when the links form exactly one path covering every element,
/// and `Err(diagnosis)` otherwise. The diagnosis leads with head and link counts
/// because those are the numbers the defect was originally measured in.
fn diagnose(links: &[Link]) -> Result<(), String> {
    let n = links.len();

    // A head is a record that names no predecessor: prev_hash == H(content_hash).
    let is_head = |l: &Link| -> bool {
        l.prev_hash
            .as_deref()
            .is_some_and(|p| p == compute_chain_hash(&l.content_hash, None).as_slice())
    };
    let heads: Vec<&Link> = links.iter().filter(|l| is_head(l)).collect();

    // Index every record by the predecessor it claims, so "who follows X" is a
    // lookup rather than a scan. Collisions here are forks and are reported.
    let mut by_prev: HashMap<Vec<u8>, Vec<&Link>> = HashMap::new();
    for l in links.iter().filter(|l| !is_head(l)) {
        if let Some(ref p) = l.prev_hash {
            by_prev.entry(p.clone()).or_default().push(l);
        }
    }
    let linked = links.len() - heads.len();

    if heads.len() != 1 {
        return Err(format!(
            "{} head(s) and {} linked record(s) across {} concurrent write(s); expected 1 head \
             and {} links.\n\
             Every head is a record written as if the log were empty, so nothing orders it \
             against its siblings: a removal or a reordering between them is undetectable.\n\
             heads: {}",
            heads.len(),
            linked,
            n,
            n - 1,
            heads
                .iter()
                .map(|l| l.label.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // Follow the single head forward. Each step must find exactly one successor.
    let mut walked = 1usize;
    let mut cursor = heads[0].content_hash.clone();
    let mut seen = vec![heads[0].label.clone()];
    loop {
        let expected: Vec<_> = links
            .iter()
            .filter(|l| {
                l.prev_hash.as_deref()
                    == Some(compute_chain_hash(&l.content_hash, Some(&cursor)).as_slice())
            })
            .collect();
        match expected.len() {
            0 => break,
            1 => {
                walked += 1;
                cursor = expected[0].content_hash.clone();
                seen.push(expected[0].label.clone());
            }
            k => {
                let mut stamps: Vec<&str> = links.iter().map(|l| l.ts.as_str()).collect();
                stamps.sort_unstable();
                let distinct = {
                    let mut d = stamps.clone();
                    d.dedup();
                    d.len()
                };
                return Err(format!(
                    "the chain forks: {k} records claim the same predecessor after {walked} \
                     step(s).\n\
                     {distinct} distinct write timestamp(s) across {n} record(s). If that is \
                     fewer than {n}, the head lookup's `ORDER BY <timestamp> DESC LIMIT 1` has \
                     a tie to break and no way to break it, so it can return a record that is \
                     not the tip — a fork that no amount of mutual exclusion prevents.\n\
                     walked: {}\n\
                     stamps: {}",
                    seen.join(" -> "),
                    stamps.join(", ")
                ));
            }
        }
    }

    if walked != n {
        return Err(format!(
            "the chain covers {walked} of {n} record(s); {} are unreachable from the head. \
             by_prev groups: {}\nwalked: {}",
            n - walked,
            by_prev.len(),
            seen.join(" -> ")
        ));
    }
    Ok(())
}

/// Fire `WRITERS` concurrent `remember()` calls at one key and hand back the
/// engine for inspection. Multi-threaded on purpose: a current-thread runtime
/// would still interleave at await points, but a multi-threaded one also
/// overlaps the `spawn_blocking` DuckDB work, which is where the window is.
async fn write_concurrently(thread_id: Option<&str>) -> Arc<MnemoEngine> {
    let engine = engine();
    let mut tasks = Vec::with_capacity(WRITERS);
    for i in 0..WRITERS {
        let engine = Arc::clone(&engine);
        let thread_id = thread_id.map(|s| s.to_string());
        tasks.push(tokio::spawn(async move {
            // Distinct content per writer: identical content at an identical
            // microsecond would collide on content_hash, which would be a
            // different bug hiding this one.
            let mut req = RememberRequest::new(format!("concurrent write #{i:02}"));
            req.agent_id = Some(AGENT.to_string());
            req.thread_id = thread_id;
            engine.remember(req).await
        }));
    }
    for (i, t) in tasks.into_iter().enumerate() {
        t.await
            .unwrap_or_else(|e| panic!("writer {i} panicked: {e}"))
            .unwrap_or_else(|e| panic!("writer {i} failed: {e}"));
    }
    engine
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_remember_produces_one_event_chain() {
    let engine = write_concurrently(None).await;

    let events = engine
        .storage
        .list_events(AGENT, 10_000, 0)
        .await
        .expect("list events");
    let writes: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == EventType::MemoryWrite)
        .collect();
    assert_eq!(
        writes.len(),
        WRITERS,
        "every write must emit exactly one MemoryWrite event before the chain shape means anything"
    );

    let links: Vec<Link> = writes
        .iter()
        .map(|e| Link {
            label: e.id.to_string(),
            ts: e.timestamp.clone(),
            content_hash: e.content_hash.clone(),
            prev_hash: e.prev_hash.clone(),
        })
        .collect();

    if let Err(diagnosis) = diagnose(&links) {
        panic!(
            "the agent_events chain is not a chain after {WRITERS} concurrent remember() calls \
             on one (agent_id, thread_id).\n\n{diagnosis}\n\n\
             remember() reads the head and then inserts; overlapping calls read the same head. \
             Until that sequence is serialised per key, an exported log verifies record-by-record \
             but proves nothing about order."
        );
    }
}

/// The same property on the memory chain. `mnemo audit export` walks `memories`,
/// so this is the chain `tools/verify_mnemo_chain.py` is pointed at.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_remember_produces_one_memory_chain() {
    let engine = write_concurrently(None).await;

    let records = engine
        .storage
        .list_memories_by_agent_ordered(AGENT, None, 10_000)
        .await
        .expect("list memories");
    assert_eq!(records.len(), WRITERS);

    let links: Vec<Link> = records
        .iter()
        .map(|r| Link {
            label: r.id.to_string(),
            ts: r.created_at.clone(),
            content_hash: r.content_hash.clone(),
            prev_hash: r.prev_hash.clone(),
        })
        .collect();

    if let Err(diagnosis) = diagnose(&links) {
        panic!(
            "the memories chain is not a chain after {WRITERS} concurrent writes.\n\n{diagnosis}"
        );
    }
}

/// Scoping the lock to `(agent_id, thread_id)` is the whole point of the fix, so
/// an explicit thread must chain too — a per-agent-only lock would pass the tests
/// above and still leave every thread racing.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_remember_chains_within_an_explicit_thread() {
    let engine = write_concurrently(Some("thread-alpha")).await;

    let events = engine
        .storage
        .get_events_by_thread("thread-alpha", 10_000)
        .await
        .expect("list thread events");
    let links: Vec<Link> = events
        .iter()
        .filter(|e| e.event_type == EventType::MemoryWrite)
        .map(|e| Link {
            label: e.id.to_string(),
            ts: e.timestamp.clone(),
            content_hash: e.content_hash.clone(),
            prev_hash: e.prev_hash.clone(),
        })
        .collect();
    assert_eq!(links.len(), WRITERS);

    if let Err(diagnosis) = diagnose(&links) {
        panic!("the thread-scoped event chain is not a chain.\n\n{diagnosis}");
    }
}
