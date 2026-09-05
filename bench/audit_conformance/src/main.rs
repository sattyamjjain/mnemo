//! Regulated-memory **audit-conformance** proof — offline, deterministic.
//!
//! # What this proves
//!
//! Regulated deployments (EU AI Act Art.12 record-keeping, DPDPA record of
//! processing, HIPAA §164.312(b) audit controls) need a memory store whose
//! write log is **tamper-evident** and **externally verifiable without trusting
//! the store**. This bench demonstrates — deterministically, offline, with no
//! network and no LLM — that mnemo's *already-shipped* primitives deliver that:
//!
//! 1. **write-chain verifies** — every memory written through the real
//!    [`MnemoEngine::remember`] path carries a SHA-256 content hash chained to
//!    its predecessor; an external verifier ([`mnemo_core::hash::verify_chain`],
//!    run here as a pure function over the *exported* records — the store is not
//!    consulted) accepts the pristine log.
//! 2. **event-log verifies** — the append-only `agent_events` log is itself a
//!    hash chain ([`verify_event_chain`]); the verifier accepts it.
//! 3. **tamper is detected** — over many trials, flipping a single byte of any
//!    record's content makes the offline verifier reject the log and name the
//!    first broken record. Detection rate is reported with a **Wilson 95%**
//!    interval (shared [`mnemo_locomo_bench::stats::wilson_95`]).
//! 4. **append-only retention** — `forget` does not erase: it appends a signed
//!    `MemoryDelete` event (the event count only grows, the event chain still
//!    verifies) and the original write row is retained (recoverable via an
//!    `include_deleted` query). The record of *what was written and when*
//!    survives its own deletion — the property a retention obligation needs.
//! 5. **concurrent writes form one chain** — properties 1-4 write one record at
//!    a time, so they say nothing about overlapping writes, which is where the
//!    chain actually broke until v0.5.29 (every racing write inserted itself as
//!    a fresh head; see `docs/verify-my-log.md`). A concurrency arm writes
//!    `--concurrent-writers x --writes-per-writer` records at once and reports
//!    the **linkage rate** — records naming a predecessor, over the count a
//!    correct chain would have — in the same successes/denominator + Wilson 95%
//!    shape as property 3, so the two read side by side. It additionally asserts
//!    structurally, with no interval to hide behind, that there is exactly one
//!    head, no fork, nothing dangling, and every record reachable from that head.
//!
//! Plus a **fixed, recomputable crypto vector**: SHA-256 content/chain hashes
//! over hard-coded inputs, so anyone can recompute the exact hex offline and
//! confirm the algorithm. Because every input is fixed, the emitted report is
//! **byte-stable** across runs and machines — diff two runs and they match.
//!
//! # It builds on shipped code, it does not re-implement it
//!
//! Every hash and every verification call in this bench is a public function
//! from `mnemo-core` (`hash::compute_content_hash`, `hash::compute_chain_hash`,
//! `hash::verify_chain`, `hash::verify_event_chain`,
//! `MnemoEngine::verify_integrity`, `MnemoEngine::verify_event_integrity`). The
//! bench only *drives* and *reports*; the cryptography lives in the library.
//!
//! Reproduce: `cargo run --release -p mnemo-audit-conformance-bench`

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::hash::{
    ChainVerificationResult, compute_chain_hash, compute_content_hash, verify_chain,
    verify_event_chain,
};
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::memory::{ConsolidationState, MemoryRecord, MemoryType, Scope, SourceType};
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy};
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::MemoryFilter;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_locomo_bench::stats::wilson_95;

const AGENT: &str = "audit-conformance-agent";

/// The concurrency arm writes under its own agent id so its chain is a separate
/// chain. Sharing `AGENT` would interleave concurrent writes into the serial
/// arm's log and make the serial figure a measurement of something else.
const CONCURRENT_AGENT: &str = "audit-conformance-concurrent";

const EMBED_DIM: usize = 16;

/// Runtime worker threads. Named as a constant, and printed into the report,
/// because a concurrency figure without a thread count is not reproducible: on
/// one worker thread the writers never overlap and the arm passes trivially.
///
/// This is why `main` builds its runtime by hand rather than using
/// `#[tokio::main(worker_threads = ...)]` — that attribute takes a literal, so
/// the number the report prints could drift from the number the runtime used.
const WORKER_THREADS: usize = 8;

#[derive(Parser, Debug)]
#[command(
    name = "audit_conformance",
    about = "Offline, deterministic proof that mnemo's memory-write log is tamper-evident and externally verifiable."
)]
struct Cli {
    /// Memories written through the real engine to build the audited chain.
    #[arg(long, default_value_t = 64)]
    records: usize,
    /// Independent single-byte tamper trials (each flips one record and asks
    /// the offline verifier to catch it). Fixed → the reported rate is stable.
    #[arg(long, default_value_t = 256)]
    tamper_trials: usize,
    /// Concurrent writers in the concurrency arm. Each writer issues
    /// `--writes-per-writer` `remember()` calls against one shared engine.
    #[arg(long, default_value_t = 16)]
    concurrent_writers: usize,
    /// Writes per concurrent writer. `concurrent_writers × writes_per_writer`
    /// is the arm's record count; the linkage denominator is one less than
    /// that, because exactly one record is legitimately the head.
    #[arg(long, default_value_t = 16)]
    writes_per_writer: usize,
    /// Output directory for the byte-stable conformance report.
    #[arg(long, default_value = "bench/audit_conformance/results")]
    out_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Engine (in-memory, offline, deterministic — Noop embedder, no network)
// ---------------------------------------------------------------------------

fn build_engine_for(agent: &str) -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("in-memory duckdb"));
    let index = Arc::new(UsearchIndex::new(EMBED_DIM).expect("usearch index"));
    let embedding = Arc::new(NoopEmbedding::new(EMBED_DIM));
    MnemoEngine::new(storage, index, embedding, agent.to_string(), None)
}

fn build_engine() -> MnemoEngine {
    build_engine_for(AGENT)
}

// ---------------------------------------------------------------------------
// A single conformance property + its outcome (all deterministic).
// ---------------------------------------------------------------------------

struct Property {
    key: &'static str,
    pass: bool,
    detail: String,
}

/// Build one fixed `MemoryRecord` with fully deterministic fields so the crypto
/// vector (and thus the report) is byte-stable. Chains to `prev_content_hash`
/// exactly the way [`verify_chain`] expects.
fn fixed_record(
    n: u128,
    content: &str,
    ts: &str,
    prev_content_hash: Option<&[u8]>,
) -> MemoryRecord {
    let content_hash = compute_content_hash(content, AGENT, ts);
    let prev_hash = Some(compute_chain_hash(&content_hash, prev_content_hash));
    MemoryRecord {
        id: uuid::Uuid::from_u128(n),
        agent_id: AGENT.to_string(),
        content: content.to_string(),
        memory_type: MemoryType::Semantic,
        scope: Scope::Private,
        importance: 0.5,
        tags: vec![],
        metadata: serde_json::json!({}),
        embedding: None,
        content_hash,
        prev_hash,
        source_type: SourceType::System,
        source_id: None,
        consolidation_state: ConsolidationState::Raw,
        access_count: 0,
        org_id: None,
        thread_id: None,
        created_at: ts.to_string(),
        updated_at: ts.to_string(),
        last_accessed_at: None,
        expires_at: None,
        deleted_at: None,
        decay_rate: None,
        created_by: None,
        version: 1,
        prev_version_id: None,
        quarantined: false,
        quarantine_reason: None,
        decay_function: None,
    }
}

/// Fixed inputs → fixed SHA-256 → byte-stable hex anyone can recompute offline.
/// Returns (properties, json-serialisable vector description).
fn crypto_vector() -> (Vec<Property>, serde_json::Value) {
    // Three hard-coded writes, an audit trail a regulator would recognise.
    let inputs = [
        (
            "2026-01-01T00:00:00Z",
            "patient record created: intake note",
        ),
        (
            "2026-01-01T00:00:01Z",
            "dosage adjusted to 5mg by clinician",
        ),
        ("2026-01-01T00:00:02Z", "discharge summary finalised"),
    ];
    let mut records: Vec<MemoryRecord> = Vec::new();
    let mut prev_ch: Option<Vec<u8>> = None;
    for (i, (ts, content)) in inputs.iter().enumerate() {
        let rec = fixed_record((i as u128) + 1, content, ts, prev_ch.as_deref());
        prev_ch = Some(rec.content_hash.clone());
        records.push(rec);
    }

    // The offline verifier accepts the pristine fixed chain...
    let pristine = verify_chain(&records);
    // ...and rejects a one-byte content flip in the middle record, naming it.
    let mut tampered = records.clone();
    tampered[1].content = "dosage adjusted to 50mg by clinician".to_string();
    let broken = verify_chain(&tampered);
    let detected_at_middle = !broken.valid && broken.first_broken_at == Some(records[1].id);

    let props = vec![
        Property {
            key: "crypto_vector_pristine_verifies",
            pass: pristine.valid && pristine.verified_records == records.len(),
            detail: format!(
                "fixed 3-write chain verifies ({}/{} records)",
                pristine.verified_records,
                records.len()
            ),
        },
        Property {
            key: "crypto_vector_tamper_detected",
            pass: detected_at_middle,
            detail: format!(
                "one-byte content flip rejected; first_broken_at = fixed uuid {} (record #2)",
                records[1].id
            ),
        },
    ];

    let vector = serde_json::json!({
        "agent_id": AGENT,
        "inputs": inputs.iter().map(|(ts, c)| serde_json::json!({"created_at": ts, "content": c})).collect::<Vec<_>>(),
        "content_hash_sha256_hex": records.iter().map(|r| hex::encode(&r.content_hash)).collect::<Vec<_>>(),
        "chain_hash_sha256_hex": records.iter().map(|r| hex::encode(r.prev_hash.as_deref().unwrap_or_default())).collect::<Vec<_>>(),
        "recompute": "content_hash[i] = SHA256(content[i] || agent_id || created_at[i]); chain_hash[0] = SHA256(content_hash[0]); chain_hash[i>0] = SHA256(content_hash[i] || content_hash[i-1])",
        "tamper": {
            "mutated_record_index": 1,
            "detected": detected_at_middle,
            "first_broken_at_uuid": records[1].id.to_string(),
        },
    });
    (props, vector)
}

// ---------------------------------------------------------------------------
// Live-engine properties
// ---------------------------------------------------------------------------

async fn ordered_records(engine: &MnemoEngine) -> Vec<MemoryRecord> {
    engine
        .storage
        .list_memories_by_agent_ordered(AGENT, None, 1_000_000)
        .await
        .expect("ordered memories export")
}

async fn event_count(engine: &MnemoEngine) -> usize {
    engine
        .storage
        .list_events(AGENT, 1_000_000, 0)
        .await
        .expect("event export")
        .len()
}

/// Run one tamper trial: flip a byte in `records[idx]` and confirm the offline
/// verifier rejects the log AND fingers exactly that record.
fn tamper_is_caught(records: &[MemoryRecord], idx: usize) -> bool {
    let mut copy = records.to_vec();
    // Deterministic single-character mutation (append a marker byte).
    copy[idx].content.push('\u{1}');
    let result = verify_chain(&copy);
    !result.valid && result.first_broken_at == Some(records[idx].id)
}

// ---------------------------------------------------------------------------
// Concurrency arm
// ---------------------------------------------------------------------------

/// Outcome of the concurrency arm, in the same units as the serial figure: a
/// count of successes over an explicit denominator.
struct Linkage {
    /// Records the arm actually wrote.
    records: usize,
    /// Records naming a predecessor. A correct chain has exactly `records - 1`.
    linked: usize,
    /// Records naming no predecessor. A correct chain has exactly one.
    heads: usize,
    /// Records reachable by walking links from the head. Catches a chain that
    /// has one head and the right link count but is split into a path plus a
    /// detached ring — which `linked` alone would score as perfect.
    reachable: usize,
    /// Content hashes claimed as predecessor by more than one record. Every one
    /// of these is a fork: two records asserting they follow the same write.
    forks: usize,
    /// Non-head records whose claimed predecessor is absent from the set.
    dangling: usize,
    /// Heads in the parallel `agent_events` chain — the log an auditor is handed.
    event_heads: usize,
}

impl Linkage {
    /// Links a correct chain would have. One record is legitimately the head, so
    /// the denominator is one less than the record count: a rate over `records`
    /// could never reach 100% and would understate a working chain forever.
    fn denominator(&self) -> usize {
        self.records.saturating_sub(1)
    }

    fn rate(&self) -> f64 {
        if self.denominator() == 0 {
            1.0
        } else {
            self.linked as f64 / self.denominator() as f64
        }
    }
}

/// A head is a record naming no predecessor: `prev_hash == H(content_hash)`.
/// Same predicate as `crates/mnemo-core/tests/concurrent_chain_linkage.rs`.
fn is_head(content_hash: &[u8], prev_hash: Option<&[u8]>) -> bool {
    prev_hash.is_some_and(|p| p == compute_chain_hash(content_hash, None).as_slice())
}

/// What a record set looks like when read as a chain. Order-independent: it is
/// derived from the hashes, not from the order the records arrived in.
struct Diagnosis {
    /// Records naming no predecessor. A correct chain has exactly one.
    heads: usize,
    /// Content hashes claimed as predecessor by more than one record. Each is
    /// two records asserting they follow the same write.
    forks: usize,
    /// Records reachable by walking links from a single head. Catches a set with
    /// the right head and link counts that is nonetheless a path plus a detached
    /// component — which counting alone scores as perfect. Zero when the head
    /// count is not exactly one, because there is then no walk to take.
    reachable: usize,
    /// Non-head records whose claimed predecessor is not in the set at all —
    /// what a record removed from the middle leaves behind.
    dangling: usize,
}

/// Read `records` as a chain and count what is actually there.
///
/// Kept pure and separate from the writing so it can be tested against a chain
/// that is *broken*; a measurement that has only ever been run against a correct
/// chain is not evidence that it would notice a wrong one. See the tests below.
fn diagnose_chain(records: &[MemoryRecord]) -> Diagnosis {
    let heads: Vec<&MemoryRecord> = records
        .iter()
        .filter(|r| is_head(&r.content_hash, r.prev_hash.as_deref()))
        .collect();

    // Resolve each non-head record to the record it actually follows, then count
    // predecessors claimed more than once.
    //
    // The link is `S.prev_hash == H(S.content_hash ‖ P.content_hash)`, which mixes
    // in S's OWN content hash. Two records following the same predecessor
    // therefore carry *different* `prev_hash` values, so grouping records by
    // `prev_hash` — the obvious implementation, and the one written here first —
    // can never observe a fork at all. It has to be resolved, not keyed. The
    // test below is what caught that.
    let mut claimed: std::collections::HashMap<uuid::Uuid, usize> =
        std::collections::HashMap::new();
    let mut dangling = 0usize;
    for s in records
        .iter()
        .filter(|r| !is_head(&r.content_hash, r.prev_hash.as_deref()))
    {
        let predecessor = records.iter().find(|p| {
            p.id != s.id
                && s.prev_hash.as_deref().is_some_and(|h| {
                    h == compute_chain_hash(&s.content_hash, Some(&p.content_hash)).as_slice()
                })
        });
        match predecessor {
            Some(p) => *claimed.entry(p.id).or_default() += 1,
            None => dangling += 1,
        }
    }
    let forks = claimed.values().filter(|n| **n > 1).count();

    // The successor of `cur` is the record S with
    // `S.prev_hash == H(S.content_hash ‖ cur.content_hash)`. That depends on S's
    // own content hash, so it cannot be a map lookup keyed on `cur` alone — the
    // scan is the same quadratic walk `mnemo audit export` documents.
    let mut reachable = 0usize;
    if heads.len() == 1 {
        let mut seen: std::collections::HashSet<uuid::Uuid> = std::collections::HashSet::new();
        let mut cursor: Option<&MemoryRecord> = Some(heads[0]);
        while let Some(cur) = cursor {
            if !seen.insert(cur.id) {
                break; // a repeat means the walk is not a simple path
            }
            reachable += 1;
            cursor = records.iter().find(|r| {
                !seen.contains(&r.id)
                    && r.prev_hash.as_deref().is_some_and(|p| {
                        p == compute_chain_hash(&r.content_hash, Some(&cur.content_hash)).as_slice()
                    })
            });
        }
    }

    Diagnosis {
        heads: heads.len(),
        forks,
        reachable,
        dangling,
    }
}

/// `writers` tasks issuing `per_writer` `remember()` calls each, concurrently,
/// against one engine on a [`WORKER_THREADS`]-thread runtime.
///
/// The serial arm above writes one record at a time, so it says nothing about
/// what happens when two writes overlap. That was a real defect until v0.5.29 —
/// every overlapping write inserted itself as a fresh head, and the log became a
/// pile of unlinked records rather than a chain (see `docs/verify-my-log.md`).
/// This arm measures the property that failed.
///
/// It is measured **structurally**, on head and link counts, rather than by
/// running [`verify_chain`] over the export. `verify_chain` takes records in
/// chain order, and `list_memories_by_agent_ordered` returns them in
/// `created_at` order — which under concurrency is a different order, because a
/// write's timestamp records when it *started* and the chain records what was
/// *inserted*. Re-deriving chain order is `mnemo audit export`'s job and is
/// tested there; duplicating it here would mean this bench re-implementing
/// shipped code, which is exactly what it does not do.
async fn concurrency_arm(writers: usize, per_writer: usize) -> Linkage {
    let engine = Arc::new(build_engine_for(CONCURRENT_AGENT));

    let mut tasks = Vec::with_capacity(writers);
    for w in 0..writers {
        let engine = engine.clone();
        tasks.push(tokio::spawn(async move {
            for i in 0..per_writer {
                engine
                    .remember(RememberRequest::new(format!(
                        "concurrent audit write w{w}#{i}: regulated action logged for record-keeping"
                    )))
                    .await
                    .expect("concurrent remember");
            }
        }));
    }
    for t in tasks {
        t.await.expect("writer task panicked");
    }

    let records = engine
        .storage
        .list_memories_by_agent_ordered(CONCURRENT_AGENT, None, 1_000_000)
        .await
        .expect("concurrent memory export");
    let Diagnosis {
        heads,
        forks,
        reachable,
        dangling,
    } = diagnose_chain(&records);

    let events = engine
        .storage
        .list_events(CONCURRENT_AGENT, 1_000_000, 0)
        .await
        .expect("concurrent event export");
    let event_heads = events
        .iter()
        .filter(|e| is_head(&e.content_hash, e.prev_hash.as_deref()))
        .count();

    Linkage {
        records: records.len(),
        linked: records.len() - heads,
        heads,
        reachable,
        forks,
        dangling,
        event_heads,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Built by hand rather than via `#[tokio::main]` so that [`WORKER_THREADS`]
    // is one value, used by the runtime and printed in the report.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(WORKER_THREADS)
        .enable_all()
        .build()?;
    rt.block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut props: Vec<Property> = Vec::new();

    let engine = build_engine();

    // --- Emit: write N memories through the real shipped remember() path. ---
    for i in 0..cli.records {
        engine
            .remember(RememberRequest::new(format!(
                "audit write #{i}: regulated action logged for record-keeping"
            )))
            .await?;
    }

    // --- Property 1: external verifier accepts the exported write chain. ---
    let records = ordered_records(&engine).await;
    let mem_chain: ChainVerificationResult = verify_chain(&records);
    // Cross-check the engine's own wrapper agrees with the standalone verifier.
    let engine_mem = engine.verify_integrity(None, None).await?;
    props.push(Property {
        key: "write_chain_verifies",
        pass: mem_chain.valid
            && mem_chain.verified_records == cli.records
            && records.len() == cli.records
            && engine_mem.valid,
        detail: format!(
            "{}/{} exported records verify (SHA-256 content+prev_hash chain); engine.verify_integrity agrees={}",
            mem_chain.verified_records, cli.records, engine_mem.valid
        ),
    });

    // --- Property 2: the append-only event log is itself a valid hash chain. ---
    let mut events = engine.storage.list_events(AGENT, 1_000_000, 0).await?;
    events.reverse(); // list_events is DESC; verify wants chronological
    let evt_chain = verify_event_chain(&events);
    let engine_evt = engine.verify_event_integrity(None, None).await?;
    let events_before = events.len();
    props.push(Property {
        key: "event_log_verifies",
        pass: evt_chain.valid && engine_evt.valid && events_before == cli.records,
        detail: format!(
            "{events_before} append-only events verify (one MemoryWrite per remember); engine.verify_event_integrity agrees={}",
            engine_evt.valid
        ),
    });

    // --- Property 3: offline verifier detects post-hoc mutation (Wilson CI). ---
    let mut detections = 0usize;
    for t in 0..cli.tamper_trials {
        if tamper_is_caught(&records, t % records.len().max(1)) {
            detections += 1;
        }
    }
    let (tl, th) = wilson_95(detections, cli.tamper_trials);
    props.push(Property {
        key: "tamper_is_detected",
        pass: detections == cli.tamper_trials && cli.tamper_trials > 0,
        detail: format!(
            "{detections}/{} single-byte mutations caught (rate {:.1}%, Wilson95 [{:.1}%, {:.1}%])",
            cli.tamper_trials,
            detections as f64 / cli.tamper_trials.max(1) as f64 * 100.0,
            tl * 100.0,
            th * 100.0
        ),
    });

    // --- Property 4: forget is append-only retention, not erasure. ---
    let last_id = records.last().map(|r| r.id).expect("at least one record");
    engine
        .forget(ForgetRequest {
            memory_ids: vec![last_id],
            agent_id: None,
            strategy: Some(ForgetStrategy::SoftDelete),
            criteria: None,
        })
        .await?;
    let events_after = event_count(&engine).await;
    // Event chain must still verify after the appended delete.
    let mut events2 = engine.storage.list_events(AGENT, 1_000_000, 0).await?;
    events2.reverse();
    let evt_chain_after = verify_event_chain(&events2);
    // Original write row retained (recoverable via include_deleted).
    let all_incl_deleted = engine
        .storage
        .list_memories(
            &MemoryFilter {
                agent_id: Some(AGENT.to_string()),
                include_deleted: true,
                ..Default::default()
            },
            1_000_000,
            0,
        )
        .await?;
    let retained = all_incl_deleted
        .iter()
        .find(|r| r.id == last_id)
        .map(|r| r.deleted_at.is_some())
        .unwrap_or(false);
    // Active chain (deleted row excluded) is still contiguous + valid.
    let active_after = ordered_records(&engine).await;
    let active_chain_after = verify_chain(&active_after);
    props.push(Property {
        key: "append_only_retention",
        pass: events_after == events_before + 1
            && evt_chain_after.valid
            && retained
            && active_chain_after.valid
            && active_after.len() == cli.records - 1,
        detail: format!(
            "forget appended exactly 1 event ({events_before}→{events_after}), event chain still verifies={}, \
             original write row retained (deleted_at set)={}, active chain still valid={}",
            evt_chain_after.valid, retained, active_chain_after.valid
        ),
    });

    // --- Property 5: concurrent writes produce ONE chain, not N heads. ---
    let expected_records = cli.concurrent_writers * cli.writes_per_writer;
    let link = concurrency_arm(cli.concurrent_writers, cli.writes_per_writer).await;
    let (ll, lh) = wilson_95(link.linked, link.denominator());
    props.push(Property {
        key: "concurrent_writes_chain",
        pass: link.records == expected_records
            && link.heads == 1
            && link.forks == 0
            && link.dangling == 0
            && link.reachable == link.records
            && link.linked == link.denominator(),
        detail: format!(
            "{}/{} concurrent writes linked (rate {:.1}%, Wilson95 [{:.1}%, {:.1}%]) from {} writers × {} writes on {} worker threads; \
             {} head, {} fork(s), {} dangling, {}/{} reachable from the head; event chain {} head",
            link.linked,
            link.denominator(),
            link.rate() * 100.0,
            ll * 100.0,
            lh * 100.0,
            cli.concurrent_writers,
            cli.writes_per_writer,
            WORKER_THREADS,
            link.heads,
            link.forks,
            link.dangling,
            link.reachable,
            link.records,
            link.event_heads,
        ),
    });

    // --- Fixed, recomputable crypto vector (byte-stable hex). ---
    let (crypto_props, crypto_json) = crypto_vector();
    props.extend(crypto_props);

    let conformant = props.iter().all(|p| p.pass);
    write_report(&cli, &props, &crypto_json, conformant, detections, &link)?;

    // Byte-stability self-check: hash the emitted report body with the SAME
    // shipped SHA-256 primitive (agent="" ts="" → digest is SHA256(body)).
    let body = std::fs::read_to_string(cli.out_dir.join("conformance.md"))?;
    let digest = hex::encode(compute_content_hash(&body, "", ""));

    println!("\n=== mnemo audit-conformance ===");
    for p in &props {
        println!(
            "  [{}] {:<32} {}",
            if p.pass { "PASS" } else { "FAIL" },
            p.key,
            p.detail
        );
    }
    println!(
        "\noverall: {}",
        if conformant {
            "CONFORMANT"
        } else {
            "NON-CONFORMANT"
        }
    );
    println!("report SHA-256 (byte-stable across runs): {digest}");
    println!("wrote {}", cli.out_dir.join("conformance.md").display());
    println!("wrote {}", cli.out_dir.join("conformance.json").display());

    if !conformant {
        // Fail loud — never emit a green artifact for a broken chain.
        std::process::exit(1);
    }
    Ok(())
}

fn write_report(
    cli: &Cli,
    props: &[Property],
    crypto_json: &serde_json::Value,
    conformant: bool,
    detections: usize,
    link: &Linkage,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&cli.out_dir)?;

    let mut rows = String::new();
    for p in props {
        rows.push_str(&format!(
            "| `{}` | {} | {} |\n",
            p.key,
            if p.pass { "✅ PASS" } else { "❌ FAIL" },
            p.detail
        ));
    }
    let (tl, th) = wilson_95(detections, cli.tamper_trials);
    let (cll, clh) = wilson_95(link.linked, link.denominator());

    // NOTE: byte-stable — no timestamps, no run-varying hashes in this body.
    // The concurrency arm is timing-dependent in *how* it runs but not in what
    // it reports: a correct chain of K records has exactly one head and K-1
    // links however the writers interleave. Wall-clock is deliberately not
    // reported, because that would not be stable.
    let md = format!(
        "# mnemo audit-conformance report\n\n\
         > **Deterministic, offline proof** that mnemo's memory-write log is tamper-evident and \
         externally verifiable without trusting the store. Built entirely on shipped `mnemo-core` \
         primitives (`hash::verify_chain`, `hash::verify_event_chain`, `MnemoEngine::verify_integrity`, \
         `verify_event_integrity`). No network, no LLM. This file is **byte-stable**: re-run and \
         `diff` — it will not change.\n\n\
         Reproduce: `cargo run --release -p mnemo-audit-conformance-bench`\n\n\
         **Parameters:** {records} records written serially through the real `remember()` path; \
         {trials} single-byte tamper trials; concurrency arm of {cw} writers × {wpw} writes \
         ({crecords} records) on {threads} runtime worker threads.\n\n\
         ## Conformance\n\n\
         | property | verdict | detail |\n\
         |---|---|---|\n\
         {rows}\n\
         **Overall: {overall}.**\n\n\
         ## Serial and concurrent, side by side\n\n\
         Both rates are successes over an explicit denominator with a Wilson 95% interval, so they \
         read against each other — but they are **different properties**, and the middle column \
         says which. A finite sample cannot *prove* 100%; the Wilson lower bound is the honest \
         floor.\n\n\
         | measurement | one trial is | successes / trials | rate | Wilson 95% |\n\
         |---|---|---|---|---|\n\
         | Tamper detection, **serially** written log | one single-byte mutation of one record, \
         caught by the offline verifier and attributed to the right record | {detections} / {trials} | \
         {rate:.1}% | [{tl:.1}%, {th:.1}%] |\n\
         | Chain linkage, **concurrently** written log | one record naming its predecessor, in a log \
         written by {cw} writers at once | {clinked} / {cdenom} | {crate_pct:.1}% | [{cll:.1}%, {clh:.1}%] |\n\n\
         The linkage denominator is {cdenom}, not {crecords}: exactly one record is legitimately the \
         head, so a rate over the record count could never reach 100%.\n\n\
         The concurrency arm also asserts, **structurally rather than statistically**, what a rate \
         cannot express: exactly **{cheads}** head (not {cw}), **{cforks}** fork, \
         **{cdangling}** dangling, **{creach}/{crecords}** records reachable by walking links from that head, and \
         **{cevent_heads}** head in the parallel `agent_events` chain. Those have no confidence \
         interval — they either hold or the bench exits non-zero. Until v0.5.29 this arm produced \
         {cw} heads and 0 links; see `docs/verify-my-log.md`.\n\n\
         ## Recomputable crypto vector\n\n\
         Fixed inputs → fixed SHA-256, so you can recompute the hex offline with any SHA-256 tool \
         and confirm the chaining algorithm:\n\n\
         ```json\n{vector}\n```\n\n\
         ## What this does and does NOT claim\n\n\
         - **Does:** the write log is an append-only SHA-256 hash chain; an external verifier \
         detects any post-hoc mutation and names the first broken record; `forget` appends a signed \
         delete event and retains the original write (row + event), so the audit trail survives \
         deletion.\n\
         - **Does NOT:** enforce a calendar retention window (e.g. the EU AI Act Art.26(6) six-month \
         clock) — that is a deployment policy on top of this log — and does NOT itself constitute \
         legal compliance. It proves the *mechanism* a record-keeping obligation depends on. See \
         [`docs/compliance/eu-ai-act-art12.md`](../../../docs/compliance/eu-ai-act-art12.md) and \
         [`docs/compliance/dpdp-2027.md`](../../../docs/compliance/dpdp-2027.md).\n",
        records = cli.records,
        trials = cli.tamper_trials,
        detections = detections,
        cw = cli.concurrent_writers,
        wpw = cli.writes_per_writer,
        threads = WORKER_THREADS,
        crecords = link.records,
        clinked = link.linked,
        cdenom = link.denominator(),
        crate_pct = link.rate() * 100.0,
        cll = cll * 100.0,
        clh = clh * 100.0,
        cheads = link.heads,
        cforks = link.forks,
        cdangling = link.dangling,
        creach = link.reachable,
        cevent_heads = link.event_heads,
        rows = rows,
        overall = if conformant {
            "CONFORMANT"
        } else {
            "NON-CONFORMANT"
        },
        rate = detections as f64 / cli.tamper_trials.max(1) as f64 * 100.0,
        tl = tl * 100.0,
        th = th * 100.0,
        vector = serde_json::to_string_pretty(crypto_json)?,
    );

    let json = serde_json::json!({
        "bench": "audit_conformance",
        "deterministic": true,
        "offline": true,
        "records_written": cli.records,
        "tamper_trials": cli.tamper_trials,
        "tamper_detections": detections,
        "tamper_detection_ci95": [tl, th],
        "concurrency": {
            "writers": cli.concurrent_writers,
            "writes_per_writer": cli.writes_per_writer,
            "worker_threads": WORKER_THREADS,
            "records": link.records,
            "linked": link.linked,
            "linkage_denominator": link.denominator(),
            "linkage_rate": link.rate(),
            "linkage_ci95": [cll, clh],
            "heads": link.heads,
            "forks": link.forks,
            "dangling": link.dangling,
            "reachable_from_head": link.reachable,
            "event_chain_heads": link.event_heads,
        },
        "properties": props.iter().map(|p| serde_json::json!({
            "key": p.key, "pass": p.pass, "detail": p.detail,
        })).collect::<Vec<_>>(),
        "conformant": conformant,
        "crypto_vector": crypto_json,
        "built_on": [
            "mnemo_core::hash::compute_content_hash",
            "mnemo_core::hash::compute_chain_hash",
            "mnemo_core::hash::verify_chain",
            "mnemo_core::hash::verify_event_chain",
            "mnemo_core::query::MnemoEngine::verify_integrity",
            "mnemo_core::query::MnemoEngine::verify_event_integrity",
        ],
    });

    std::fs::write(cli.out_dir.join("conformance.md"), md)?;
    std::fs::write(
        cli.out_dir.join("conformance.json"),
        serde_json::to_string_pretty(&json)?,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests for the measurement itself
// ---------------------------------------------------------------------------
//
// The concurrency arm reports "1 head, 0 forks, all reachable" against a chain
// that is correct. On its own that is not evidence: a diagnosis that always
// returns those numbers would report exactly the same thing. These tests run it
// in the failing direction, including against the precise shape of the defect it
// exists to detect.
#[cfg(test)]
mod tests {
    use super::*;

    const TS: &str = "2026-01-01T00:00:00Z";

    /// `n` records chained head → tail, the shape a correct log has.
    fn chained(n: u128) -> Vec<MemoryRecord> {
        let mut out: Vec<MemoryRecord> = Vec::new();
        let mut prev: Option<Vec<u8>> = None;
        for i in 0..n {
            let r = fixed_record(i + 1, &format!("record {i}"), TS, prev.as_deref());
            prev = Some(r.content_hash.clone());
            out.push(r);
        }
        out
    }

    #[test]
    fn a_correct_chain_reads_as_one_path() {
        let d = diagnose_chain(&chained(8));
        assert_eq!(d.heads, 1, "a correct chain has exactly one head");
        assert_eq!(d.forks, 0);
        assert_eq!(
            d.reachable, 8,
            "every record reachable by walking from head"
        );
    }

    /// The pre-v0.5.29 defect: every concurrent write inserted itself as a fresh
    /// head. If the diagnosis cannot see this, the concurrency arm proves nothing.
    #[test]
    fn the_original_defect_reads_as_n_heads_and_zero_links() {
        let all_heads: Vec<MemoryRecord> = (0..8)
            .map(|i| fixed_record(i + 1, &format!("record {i}"), TS, None))
            .collect();
        let d = diagnose_chain(&all_heads);
        assert_eq!(
            d.heads, 8,
            "16-writers-16-heads is the shape that must fail"
        );
        assert_eq!(
            d.reachable, 0,
            "with no single head there is no walk, so nothing is reachable"
        );
        // And the arm's headline rate collapses with it: 0 links over 7.
        assert_eq!(
            all_heads.len() - d.heads,
            0,
            "no record names a predecessor"
        );
    }

    /// Two records claiming the same predecessor — a fork, which head and link
    /// counts alone would score as perfect.
    #[test]
    fn two_records_claiming_one_predecessor_read_as_a_fork() {
        let head = fixed_record(1, "head", TS, None);
        let left = fixed_record(2, "left", TS, Some(&head.content_hash));
        let right = fixed_record(3, "right", TS, Some(&head.content_hash));
        let d = diagnose_chain(&[head, left, right]);
        assert_eq!(d.heads, 1, "a fork still has one head — that is the point");
        assert_eq!(
            d.forks, 1,
            "the fork must be counted, not hidden by the head count"
        );
    }

    /// A record removed from the middle of an otherwise intact chain. Head and
    /// fork counts stay clean; only reachability notices.
    #[test]
    fn a_removed_record_leaves_its_successors_unreachable() {
        let mut records = chained(5);
        records.remove(2); // drop the middle record from the exported set
        let d = diagnose_chain(&records);
        assert_eq!(d.heads, 1);
        assert_eq!(d.forks, 0);
        assert_eq!(
            d.reachable, 2,
            "the walk stops at the gap: only the two records before it are reachable"
        );
        assert!(
            d.reachable < records.len(),
            "reachability is the check that catches a removal"
        );
    }
}
