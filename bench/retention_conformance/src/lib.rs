//! Processing-log **retention-conformance** harness — offline, deterministic.
//!
//! # What this proves
//!
//! A retention obligation (India DPDP Rules 2025 → 1 year; EU AI Act Art.19/26(6)
//! → 6 months; HIPAA §164.312(b)/§164.316(b)(2) → 6 years) is only as good as the
//! guarantee that the processing log survives every routine deletion path. This
//! harness drives **every** path in mnemo-core that could plausibly drop or
//! rewrite an `agent_events` row and asserts, with the shipped
//! [`mnemo_compliance::RetentionProfile`], that the retention floor held:
//!
//! - `forget` — SoftDelete / HardDelete / Redact / Archive (incl. cold-tier)
//! - `run_ttl_sweep` — hard-expiry of a past-due memory
//! - `run_decay_pass` — decay/archival housekeeping
//! - `run_consolidation` — cluster consolidation
//!
//! Each path deletes/edits **memory content** and *appends* an audit event; none
//! removes an event. The harness snapshots the event log before and after each
//! path and verifies (a) no event inside the floor was dropped, (b) retained
//! events are byte-identical, and (c) **traffic/processing metadata** (DPDP's
//! "traffic data and logs", separate from personal data) is retained. It also
//! runs the fail-loud backend gate.
//!
//! Everything is offline (no network, no LLM); the emitted artifact carries only
//! counts / pass-fail, so it is **byte-stable** across runs and machines.

use std::sync::Arc;

use mnemo_compliance::{RetentionFinding, RetentionProfile, RetentionReport};
use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::hash::{compute_chain_hash, compute_content_hash};
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::event::{AgentEvent, EventType};
use mnemo_core::model::memory::MemoryRecord;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy};
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::cold::{ColdStorageConfig, InMemoryColdStorage};
use mnemo_core::storage::duckdb::DuckDbStorage;

pub const AGENT: &str = "retention-conformance-agent";
const EMBED_DIM: usize = 16;

/// Harness parameters. Deterministic — the same config yields the same
/// pass/fail counts, so the rendered report is byte-stable.
#[derive(Clone, Debug)]
pub struct Config {
    /// Retention profile: `"dpdp"`, `"eu-ai-act-art19"`, or `"hipaa"`.
    pub profile: String,
    /// Optional floor override in days.
    pub floor_days: Option<u32>,
    /// Memory-write events seeded per path.
    pub records: usize,
    /// Traffic-bearing (model-response) events seeded per path.
    pub traffic_events: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: "dpdp".to_string(),
            floor_days: None,
            records: 24,
            traffic_events: 4,
        }
    }
}

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

fn build_engine() -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("in-memory duckdb"));
    let index = Arc::new(UsearchIndex::new(EMBED_DIM).expect("usearch index"));
    let embedding = Arc::new(NoopEmbedding::new(EMBED_DIM));
    // A real cold-tier so the Archive path genuinely migrates to cold storage —
    // and we can prove it still does not remove an event.
    let cold = Arc::new(InMemoryColdStorage::new(ColdStorageConfig {
        bucket: "retention-bench".to_string(),
        prefix: "cold/".to_string(),
        endpoint: None,
        region: "local".to_string(),
    }));
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_cold_storage(cold)
}

/// Seed `records` memory-write events + `traffic` model-response events (which
/// carry traffic/processing metadata). Returns the seeded memory ids.
async fn seed(
    engine: &MnemoEngine,
    records: usize,
    traffic: usize,
) -> Result<Vec<uuid::Uuid>, BoxErr> {
    let mut ids = Vec::new();
    for i in 0..records {
        let resp = engine
            .remember(RememberRequest::new(format!(
                "processing record {i}: clinician updated dosage / access granted"
            )))
            .await?;
        ids.push(resp.id);
    }
    for j in 0..traffic {
        let now = chrono::Utc::now().to_rfc3339();
        let content = format!("model response {j}");
        let content_hash = compute_content_hash(&content, AGENT, &now);
        let prev = engine.storage.get_latest_event_hash(AGENT, None).await?;
        let prev_hash = Some(compute_chain_hash(&content_hash, prev.as_deref()));
        let mut ev = AgentEvent::new(
            AGENT.to_string(),
            EventType::AssistantMessage,
            serde_json::json!({ "seq": j }),
            now,
            content_hash,
        );
        ev.prev_hash = prev_hash;
        ev.model = Some("claude-opus-4-8".to_string());
        ev.tokens_input = Some(120 + j as i64);
        ev.tokens_output = Some(240 + j as i64);
        ev.latency_ms = Some(35 + j as i64);
        ev.cost_usd = Some(0.012);
        ev.trace_id = Some(format!("trace-{j}"));
        engine.storage.insert_event(&ev).await?;
    }
    Ok(ids)
}

/// Export the event log in chronological order (verify wants oldest-first).
async fn export(engine: &MnemoEngine) -> Result<Vec<AgentEvent>, BoxErr> {
    let mut events = engine.storage.list_events(AGENT, 1_000_000, 0).await?;
    events.reverse(); // list_events is DESC
    Ok(events)
}

async fn run_path<F, Fut>(
    profile: &RetentionProfile,
    cfg: &Config,
    path: &str,
    now: &str,
    action: F,
) -> Result<(RetentionFinding, RetentionFinding), BoxErr>
where
    F: FnOnce(MnemoEngine, Vec<uuid::Uuid>) -> Fut,
    Fut: std::future::Future<Output = Result<MnemoEngine, BoxErr>>,
{
    let engine = build_engine();
    let ids = seed(&engine, cfg.records, cfg.traffic_events).await?;
    let before = export(&engine).await?;
    let engine = action(engine, ids).await?;
    let after = export(&engine).await?;
    let retention = profile.verify_path(path, &before, &after, now)?;
    let mut traffic = profile.verify_traffic_metadata_retained(&before, &after);
    traffic.path = format!("{path}::traffic_metadata");
    Ok((retention, traffic))
}

fn profile_for(cfg: &Config) -> Result<RetentionProfile, BoxErr> {
    let mut p = match cfg.profile.as_str() {
        "dpdp" => RetentionProfile::dpdp_rules(),
        "eu-ai-act-art19" => RetentionProfile::eu_ai_act_art19(),
        "hipaa" => RetentionProfile::hipaa_164_312b(),
        other => return Err(format!("unknown profile '{other}'").into()),
    };
    if let Some(days) = cfg.floor_days {
        p = p.with_floor_days(days);
    }
    Ok(p)
}

/// Drive every deletion path and return the aggregate conformance report.
pub async fn run_report(cfg: &Config) -> Result<RetentionReport, BoxErr> {
    let profile = profile_for(cfg)?;
    let now = chrono::Utc::now().to_rfc3339();
    let backend = build_engine().storage.backend_name();
    let mut findings: Vec<RetentionFinding> = Vec::new();

    let append_only = build_engine().storage.events_are_append_only();
    findings.push(RetentionFinding {
        path: "backend_append_only_gate".to_string(),
        pass: profile
            .assert_backend_can_retain(backend, append_only)
            .is_ok(),
        detail: format!(
            "backend '{backend}' events_are_append_only={append_only}; floor={} days",
            profile.floor_days
        ),
    });

    macro_rules! push2 {
        ($pair:expr) => {{
            let (a, b) = $pair;
            findings.push(a);
            findings.push(b);
        }};
    }

    push2!(
        run_path(
            &profile,
            cfg,
            "forget_soft_delete",
            &now,
            |engine, ids| async move {
                engine
                    .forget(ForgetRequest {
                        memory_ids: vec![ids[0]],
                        agent_id: Some(AGENT.to_string()),
                        strategy: Some(ForgetStrategy::SoftDelete),
                        criteria: None,
                    })
                    .await?;
                Ok(engine)
            }
        )
        .await?
    );
    push2!(
        run_path(
            &profile,
            cfg,
            "forget_hard_delete",
            &now,
            |engine, ids| async move {
                engine
                    .forget(ForgetRequest {
                        memory_ids: vec![ids[0]],
                        agent_id: Some(AGENT.to_string()),
                        strategy: Some(ForgetStrategy::HardDelete),
                        criteria: None,
                    })
                    .await?;
                Ok(engine)
            }
        )
        .await?
    );
    push2!(
        run_path(
            &profile,
            cfg,
            "forget_redact",
            &now,
            |engine, ids| async move {
                engine
                    .forget(ForgetRequest {
                        memory_ids: vec![ids[0]],
                        agent_id: Some(AGENT.to_string()),
                        strategy: Some(ForgetStrategy::Redact),
                        criteria: None,
                    })
                    .await?;
                Ok(engine)
            }
        )
        .await?
    );
    push2!(
        run_path(
            &profile,
            cfg,
            "forget_archive_cold_tier",
            &now,
            |engine, ids| async move {
                engine
                    .forget(ForgetRequest {
                        memory_ids: vec![ids[0]],
                        agent_id: Some(AGENT.to_string()),
                        strategy: Some(ForgetStrategy::Archive),
                        criteria: None,
                    })
                    .await?;
                Ok(engine)
            }
        )
        .await?
    );
    push2!(
        run_path(
            &profile,
            cfg,
            "ttl_sweep_hard_expiry",
            &now,
            |engine, _ids| async move {
                let mut rec =
                    MemoryRecord::new(AGENT.to_string(), "expired processing record".to_string());
                rec.expires_at = Some("2000-01-01T00:00:00Z".to_string());
                engine.storage.insert_memory(&rec).await?;
                engine.run_ttl_sweep().await?;
                Ok(engine)
            }
        )
        .await?
    );
    push2!(
        run_path(
            &profile,
            cfg,
            "decay_pass",
            &now,
            |engine, _ids| async move {
                engine
                    .run_decay_pass(Some(AGENT.to_string()), 0.9, 0.99)
                    .await?;
                Ok(engine)
            }
        )
        .await?
    );
    push2!(
        run_path(
            &profile,
            cfg,
            "consolidation",
            &now,
            |engine, _ids| async move {
                engine.run_consolidation(Some(AGENT.to_string()), 2).await?;
                Ok(engine)
            }
        )
        .await?
    );

    Ok(RetentionReport::new(&profile, backend, findings))
}

/// Byte-stable Markdown report (counts / pass-fail only — no timestamps/hashes).
pub fn render_markdown(report: &RetentionReport, cfg: &Config, date: &str) -> String {
    let mut s = String::new();
    s.push_str("# mnemo retention-conformance report\n\n");
    s.push_str(&format!(
        "> **Deterministic, offline proof** that mnemo's append-only `agent_events` log survives \
         every deletion / compaction / cold-tier path within the **{}** retention floor of \
         **{} days**. Built entirely on shipped primitives (`forget`, `run_ttl_sweep`, \
         `run_decay_pass`, `run_consolidation`, cold archive) scored by \
         `mnemo_compliance::RetentionProfile`. No network, no LLM. This file is **byte-stable**: \
         re-run and `diff` — it will not change.\n\n",
        report.profile, report.floor_days
    ));
    s.push_str(&format!(
        "Reproduce: `cargo run --release -p mnemo-retention-conformance-bench -- --profile {}`\n\n",
        cfg.profile
    ));
    s.push_str(&format!(
        "**Obligation:** {}\n\n**Commencement:** {} · **Source:** {}\n\n**Backend:** `{}` · \
         **Seed per path:** {} memory-write + {} traffic events.\n\n",
        report.obligation,
        report.commencement,
        report.source_url,
        report.backend,
        cfg.records,
        cfg.traffic_events
    ));

    s.push_str("## Conformance — one row per deletion path\n\n");
    s.push_str("| path / check | verdict | detail |\n|---|---|---|\n");
    for f in &report.findings {
        s.push_str(&format!(
            "| `{}` | {} | {} |\n",
            f.path,
            if f.pass { "✅ PASS" } else { "❌ FAIL" },
            f.detail
        ));
    }
    s.push_str(&format!(
        "\n**Overall: {}.**\n\n",
        if report.conformant {
            "CONFORMANT"
        } else {
            "NON-CONFORMANT"
        }
    ));

    s.push_str("## What this does and does NOT claim\n\n");
    s.push_str(
        "- **Does:** every deletion path edits *memory content* and *appends* an audit event; none \
         removes or rewrites an `agent_events` row, so the processing log — including \
         traffic/processing metadata — is retained across the floor. The backend gate fails loud \
         (`ComplianceError::RetentionFloorUnsupported`) on a backend that cannot guarantee this.\n\
         - **Does NOT:** enforce a calendar clock, delete data *after* the floor, or constitute \
         legal compliance. It is a **conformance check for** the named obligation's retention \
         mechanism — not a certification. See \
         [`docs/compliance/dpdp-2027.md`](../../../docs/compliance/dpdp-2027.md) and \
         [`docs/compliance/eu-ai-act-art12.md`](../../../docs/compliance/eu-ai-act-art12.md).\n",
    );
    s.push_str(&format!("\nReport dated {date}.\n"));
    s
}
