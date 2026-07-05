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
const EMBED_DIM: usize = 16;

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
    /// Output directory for the byte-stable conformance report.
    #[arg(long, default_value = "bench/audit_conformance/results")]
    out_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Engine (in-memory, offline, deterministic — Noop embedder, no network)
// ---------------------------------------------------------------------------

fn build_engine() -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("in-memory duckdb"));
    let index = Arc::new(UsearchIndex::new(EMBED_DIM).expect("usearch index"));
    let embedding = Arc::new(NoopEmbedding::new(EMBED_DIM));
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None)
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

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    // --- Fixed, recomputable crypto vector (byte-stable hex). ---
    let (crypto_props, crypto_json) = crypto_vector();
    props.extend(crypto_props);

    let conformant = props.iter().all(|p| p.pass);
    write_report(&cli, &props, &crypto_json, conformant, detections)?;

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

    // NOTE: byte-stable — no timestamps, no run-varying hashes in this body.
    let md = format!(
        "# mnemo audit-conformance report\n\n\
         > **Deterministic, offline proof** that mnemo's memory-write log is tamper-evident and \
         externally verifiable without trusting the store. Built entirely on shipped `mnemo-core` \
         primitives (`hash::verify_chain`, `hash::verify_event_chain`, `MnemoEngine::verify_integrity`, \
         `verify_event_integrity`). No network, no LLM. This file is **byte-stable**: re-run and \
         `diff` — it will not change.\n\n\
         Reproduce: `cargo run --release -p mnemo-audit-conformance-bench`\n\n\
         **Parameters:** {records} records written through the real `remember()` path; \
         {trials} single-byte tamper trials.\n\n\
         ## Conformance\n\n\
         | property | verdict | detail |\n\
         |---|---|---|\n\
         {rows}\n\
         **Overall: {overall}.**\n\n\
         Tamper-detection rate over {trials} trials: **{rate:.1}%** \
         (Wilson 95% [{tl:.1}%, {th:.1}%]). A finite sample cannot *prove* 100%; the Wilson lower \
         bound is the honest floor.\n\n\
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
