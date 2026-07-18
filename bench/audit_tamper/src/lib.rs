//! Adversarial **audit-log tamper-evidence** bench for mnemo's hash-chained
//! `agent_events` log.
//!
//! # What this measures
//!
//! Regulated deployments (EU AI Act Art.12 record-keeping) need proof that the
//! event log is **tamper-evident**: an attacker who edits the log after the fact
//! should be caught. This bench builds a **real** `agent_events` chain through
//! [`MnemoEngine::remember`], exports it, applies four named post-hoc attacks to
//! the exported copy, and scores each with the *shipped* verifier
//! [`mnemo_core::hash::verify_event_chain`] (the same function
//! `MnemoEngine::verify_event_integrity` runs). It reports a **detection rate**
//! with a **Wilson 95%** interval per attack, plus a **benign control** (a
//! legitimately-appended chain must NOT be flagged).
//!
//! # Honest threat model — what the chain does and does NOT catch
//!
//! The event chain binds each event's `content_hash` (a SHA-256 of the
//! operation's source string) to its predecessor's via `prev_hash`
//! ([`mnemo_core::query::event_builder`]). Consequences, reported as-is:
//!
//! - **delete (mid-chain)** and **reorder** break the `prev_hash` linkage → the
//!   verifier names the first orphaned event. **Detected.**
//! - **forge (integrity field)** — rewriting the hashed `content_hash` breaks the
//!   successor's `prev_hash`. **Detected.**
//! - **forge (payload only)** — `verify_event_chain` does **not** recompute
//!   `content_hash` from the arbitrary `payload` JSON, so a payload-only rewrite
//!   that leaves `content_hash`/`prev_hash` intact is **NOT detected** by the
//!   pure chain verifier. Disclosed, not oversold. Mitigations that mnemo ships:
//!   the underlying memory record's content *is* hash-bound (`verify_chain`
//!   recomputes it), and PostgreSQL's `prevent_event_modification` trigger blocks
//!   in-place `UPDATE`. Binding the full event into `content_hash` would close it
//!   in the pure verifier too.
//! - **truncate (tail)** — the surviving prefix is itself a valid chain, and a
//!   pure chain verifier has no expected length/head anchor, so tail truncation
//!   is **NOT detected**. Mitigations mnemo ships: a signed `checkpoint` records
//!   the expected latest hash + count, and the PostgreSQL append-only trigger
//!   blocks tail deletion.
//!
//! Everything is deterministic and offline (no network, no LLM); the emitted
//! report carries only counts/rates/intervals, so it is **byte-stable** across
//! runs and machines.

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::hash::verify_event_chain;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::event::AgentEvent;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::StorageBackend;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_locomo_bench::stats::wilson_95;
use serde::Serialize;

/// Agent whose `agent_events` chain is built and attacked.
pub const AGENT: &str = "audit-tamper-bench-agent";
const EMBED_DIM: usize = 16;

/// Bench parameters. Deterministic — the same `(trials, chain_len)` always yields
/// the same detection counts, so the rendered report is byte-stable.
#[derive(Clone, Copy, Debug)]
pub struct BenchConfig {
    /// Independent tamper trials per attack (each attacks a different position).
    pub trials: usize,
    /// Length of the legitimate `agent_events` chain that gets attacked.
    pub chain_len: usize,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            trials: 200,
            chain_len: 64,
        }
    }
}

fn build() -> (MnemoEngine, Arc<DuckDbStorage>) {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("in-memory duckdb"));
    let index = Arc::new(UsearchIndex::new(EMBED_DIM).expect("usearch index"));
    let embedding = Arc::new(NoopEmbedding::new(EMBED_DIM));
    let engine = MnemoEngine::new(storage.clone(), index, embedding, AGENT.to_string(), None);
    (engine, storage)
}

/// Seed a legitimate chain of `n` events through the real `remember()` path and
/// return it in chronological order (the order `verify_event_chain` expects).
async fn seed_and_export(n: usize) -> Vec<AgentEvent> {
    let (engine, storage) = build();
    for i in 0..n {
        engine
            .remember(RememberRequest::new(format!(
                "audit event {i}: clinician adjusted dosage / access granted / record updated"
            )))
            .await
            .expect("remember appends a hash-chained event");
    }
    // list_events returns DESC (newest first); reverse to chronological.
    let mut events = storage
        .list_events(AGENT, 1_000_000, 0)
        .await
        .expect("list_events");
    events.reverse();
    events
}

/// True iff the shipped verifier rejects this (attacked) event list.
fn is_detected(events: &[AgentEvent]) -> bool {
    !verify_event_chain(events).valid
}

// --- Attacks: each clones the pristine chain and mutates one copy. ----------

/// (a) Delete one event from the middle of the log.
fn attack_delete_mid(base: &[AgentEvent], t: usize) -> Vec<AgentEvent> {
    let mut e = base.to_vec();
    let span = e.len().saturating_sub(2).max(1);
    let i = 1 + (t % span); // never the head (head deletion == front truncation)
    e.remove(i);
    e
}

/// (b) Swap two adjacent events (reorder).
fn attack_reorder(base: &[AgentEvent], t: usize) -> Vec<AgentEvent> {
    let mut e = base.to_vec();
    let span = e.len().saturating_sub(3).max(1);
    let i = 1 + (t % span);
    e.swap(i, i + 1);
    e
}

/// (c) Rewrite an event's `payload` JSON but leave the hashed integrity fields
/// (`content_hash`, `prev_hash`) intact — the disclosed blind spot.
fn attack_forge_payload(base: &[AgentEvent], t: usize) -> Vec<AgentEvent> {
    let mut e = base.to_vec();
    let i = t % e.len();
    e[i].payload = serde_json::json!({
        "forged": true,
        "note": "attacker-rewritten payload; content_hash left untouched",
        "position": i,
    });
    e
}

/// (c') Tamper the hashed integrity field (`content_hash`) directly.
fn attack_forge_content_hash(base: &[AgentEvent], t: usize) -> Vec<AgentEvent> {
    let mut e = base.to_vec();
    let i = t % e.len();
    if let Some(byte) = e[i].content_hash.first_mut() {
        *byte ^= 0xFF;
    }
    e
}

/// (d) Truncate the tail: drop the last `k` events.
fn attack_truncate_tail(base: &[AgentEvent], t: usize) -> Vec<AgentEvent> {
    let mut e = base.to_vec();
    let k = 1 + (t % (e.len() / 4).max(1));
    e.truncate(e.len() - k);
    e
}

/// One attack class and its measured detection.
#[derive(Serialize, Clone)]
pub struct AttackRow {
    pub name: String,
    pub threat: String,
    pub detected: usize,
    pub n: usize,
    /// Whether the pure chain verifier catches this class (drives the honesty column).
    pub caught_by_chain: bool,
    pub note: String,
}

impl AttackRow {
    pub fn rate(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.detected as f64 / self.n as f64
        }
    }
    pub fn ci(&self) -> (f64, f64) {
        wilson_95(self.detected, self.n)
    }
}

#[derive(Serialize)]
pub struct BenchOutcome {
    pub chain_len: usize,
    pub trials: usize,
    pub attacks: Vec<AttackRow>,
    /// Legitimate appends falsely flagged as tampering (must be 0).
    pub benign_false_positives: usize,
    pub benign_n: usize,
}

fn run_attack(
    base: &[AgentEvent],
    trials: usize,
    f: impl Fn(&[AgentEvent], usize) -> Vec<AgentEvent>,
) -> usize {
    (0..trials).filter(|&t| is_detected(&f(base, t))).count()
}

/// Run the full bench: seed a real chain, apply every attack `trials` times,
/// and measure the benign control on a legitimately-extended chain.
pub async fn run_bench(cfg: &BenchConfig) -> BenchOutcome {
    let base = seed_and_export(cfg.chain_len).await;
    assert!(
        verify_event_chain(&base).valid,
        "the pristine seeded chain must verify"
    );

    // Benign control: a legitimately-appended chain (chain_len + extra real
    // appends). A trustworthy verifier must accept every legitimate event.
    let benign = seed_and_export(cfg.chain_len + 8).await;
    let benign_res = verify_event_chain(&benign);
    let benign_n = benign.len();
    let benign_false_positives = benign_n - benign_res.verified_records;

    let attacks = vec![
        AttackRow {
            name: "delete (mid-chain)".into(),
            threat: "remove one event from the middle of the log".into(),
            detected: run_attack(&base, cfg.trials, attack_delete_mid),
            n: cfg.trials,
            caught_by_chain: true,
            note: "the successor's prev_hash no longer links to its new predecessor; \
                   verifier names the first orphaned event"
                .into(),
        },
        AttackRow {
            name: "reorder (swap two events)".into(),
            threat: "swap two adjacent events to change ordering".into(),
            detected: run_attack(&base, cfg.trials, attack_reorder),
            n: cfg.trials,
            caught_by_chain: true,
            note: "both swapped positions' prev_hash linkage break".into(),
        },
        AttackRow {
            name: "forge (integrity field content_hash)".into(),
            threat: "rewrite the hashed content_hash of one event".into(),
            detected: run_attack(&base, cfg.trials, attack_forge_content_hash),
            n: cfg.trials,
            caught_by_chain: true,
            note: "tampering the hashed field breaks the successor's prev_hash".into(),
        },
        AttackRow {
            name: "forge (payload only)".into(),
            threat: "rewrite an event's payload JSON, leaving content_hash intact".into(),
            detected: run_attack(&base, cfg.trials, attack_forge_payload),
            n: cfg.trials,
            caught_by_chain: false,
            note: "GAP: verify_event_chain does not recompute content_hash from the payload, \
                   so a payload-only rewrite is not caught. Mitigations mnemo ships: the memory \
                   record's content is hash-bound (verify_chain recomputes it); Postgres' \
                   prevent_event_modification trigger blocks in-place UPDATE"
                .into(),
        },
        AttackRow {
            name: "truncate (tail)".into(),
            threat: "drop the last k events from the log".into(),
            detected: run_attack(&base, cfg.trials, attack_truncate_tail),
            n: cfg.trials,
            caught_by_chain: false,
            note: "GAP: the surviving prefix is a valid chain; a pure verifier has no expected \
                   length/head anchor. Mitigations mnemo ships: a signed checkpoint records the \
                   expected latest hash + count; Postgres append-only trigger blocks tail deletion"
                .into(),
        },
    ];

    BenchOutcome {
        chain_len: cfg.chain_len,
        trials: cfg.trials,
        attacks,
        benign_false_positives,
        benign_n,
    }
}

fn pct(x: f64) -> f64 {
    (x * 1000.0).round() / 10.0
}

/// Byte-stable Markdown report (no timestamps/hashes — only counts/rates).
pub fn render_markdown(o: &BenchOutcome, date: &str) -> String {
    let mut s = String::new();
    s.push_str("# audit-log tamper-evidence — adversarial attacks vs. `verify_event_chain`\n\n");
    s.push_str(&format!(
        "> Post-hoc attacks on a real, {}-event `agent_events` hash chain, scored by mnemo's \
         shipped `verify_event_chain` (the verifier `verify_event_integrity` runs). **Detection \
         rate** with a **Wilson 95%** interval per attack; **honest** about the two classes the \
         pure chain verifier does not catch. Deterministic, offline, byte-stable.\n\n",
        o.chain_len
    ));
    s.push_str(&format!(
        "- Trials/attack: {}; chain length: {}; embedder: Noop (offline); backend: in-memory DuckDB.\n",
        o.trials, o.chain_len
    ));
    s.push_str("- Each attack mutates an exported copy of the chain and re-runs the verifier; the store is not consulted.\n\n");

    s.push_str("## Detection rate\n\n");
    s.push_str("| attack | threat | detection | Wilson 95% | caught by chain? |\n");
    s.push_str("|---|---|---:|---:|:--:|\n");
    for a in &o.attacks {
        let (lo, hi) = a.ci();
        s.push_str(&format!(
            "| {} | {} | {}/{} ({:.1}%) | [{:.1}%, {:.1}%] | {} |\n",
            a.name,
            a.threat,
            a.detected,
            a.n,
            pct(a.rate()),
            pct(lo),
            pct(hi),
            if a.caught_by_chain {
                "✅ yes"
            } else {
                "❌ NO"
            },
        ));
    }

    let (blo, bhi) = wilson_95(o.benign_false_positives, o.benign_n);
    s.push_str(&format!(
        "\n## Benign control\n\nLegitimately-appended chain of {} events: **{}/{} falsely flagged \
         ({:.1}%)** [Wilson 95% {:.1}%–{:.1}%]. A trustworthy verifier must accept every \
         legitimate append — the detection rates above are only meaningful at ~0% false positives.\n\n",
        o.benign_n,
        o.benign_false_positives,
        o.benign_n,
        pct(o.benign_false_positives as f64 / o.benign_n.max(1) as f64),
        pct(blo),
        pct(bhi),
    ));

    s.push_str("## Honest reading (do not oversell)\n\n");
    for a in &o.attacks {
        s.push_str(&format!("- **{}** — {}\n", a.name, a.note));
    }
    s.push_str(
        "\nThe pure chain verifier is tamper-**evidence** for deletion, reordering, and any edit of \
         the hashed integrity fields — not a guarantee against every edit. Payload-only forgery and \
         tail truncation are disclosed gaps with the shipped mitigations named above; they are not \
         claimed as caught. No \"best\"/\"first\" claim.\n\n",
    );
    s.push_str(&format!(
        "Reproduce: `cargo run --release -p mnemo-audit-tamper-bench` (report dated {date}).\n",
    ));
    s
}

/// JSON sidecar (same numbers, machine-readable).
pub fn render_json(o: &BenchOutcome, date: &str) -> serde_json::Value {
    serde_json::json!({
        "date": date,
        "chain_len": o.chain_len,
        "trials": o.trials,
        "attacks": o.attacks.iter().map(|a| {
            let (lo, hi) = a.ci();
            serde_json::json!({
                "name": a.name,
                "threat": a.threat,
                "detected": a.detected,
                "n": a.n,
                "rate_pct": pct(a.rate()),
                "wilson95_pct": [pct(lo), pct(hi)],
                "caught_by_chain": a.caught_by_chain,
                "note": a.note,
            })
        }).collect::<Vec<_>>(),
        "benign_false_positives": o.benign_false_positives,
        "benign_n": o.benign_n,
    })
}
