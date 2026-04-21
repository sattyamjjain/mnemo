//! Reflection pass — Auto-Dream-compatible semantic housekeeping.
//!
//! `run_reflection_pass` walks a single agent's memories and applies four
//! normalising sweeps in order:
//!
//! 1. **Date absolutization** (Task 10) — rewrite relative temporal phrases
//!    (`"yesterday"`, `"last week"`, `"N days ago"`, `"tomorrow"`) into
//!    ISO-8601 dates anchored on each record's `created_at`.
//! 2. **Semantic dedup** — any two memories whose embeddings have cosine
//!    similarity ≥ 0.92 collapse into a single record that unions their
//!    tags and sums their `access_count`. The older record is moved to the
//!    `Consolidated` state and a `consolidated_from` relation is inserted.
//! 3. **Low-importance conflict resolution** — run `detect_conflicts` and,
//!    for any conflict where *both* sides have `importance < 0.3`, apply
//!    `ResolutionStrategy::KeepNewest`.
//! 4. **Stale archival** — mark records `Archived` when their
//!    `effective_importance < 0.2`, their `access_count == 0`, and their
//!    age exceeds the configured threshold (default 7 days).
//!
//! The pass is exposed as `MnemoEngine::run_reflection_pass` and is safe to
//! schedule on a periodic tick (default cadence 24h, driven from the CLI).

use std::collections::HashSet;

use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::hash::{compute_chain_hash, compute_content_hash};
use crate::model::event::{AgentEvent, EventType};
use crate::model::memory::{ConsolidationState, MemoryRecord};
use crate::model::relation::Relation;
use crate::query::MnemoEngine;
use crate::query::conflict::ResolutionStrategy;
use crate::query::lifecycle::effective_importance;
use crate::storage::MemoryFilter;

const DEFAULT_DEDUP_THRESHOLD: f32 = 0.92;
const DEFAULT_LOW_IMPORTANCE_CUTOFF: f32 = 0.3;
const DEFAULT_ARCHIVE_IMPORTANCE: f32 = 0.2;
const DEFAULT_ARCHIVE_AGE_HOURS: f64 = 24.0 * 7.0;

/// Controls when the reflection pass runs its expensive phases.
///
/// `Coordinated` is the new default in v0.3.1 and honours the same trigger
/// heuristics Anthropic's Auto Dream uses:
///   * skip if fewer than `MIN_NEW_RECORDS_FOR_COORDINATED_RUN` new records
///     have accumulated since the last successful pass;
///   * skip if fewer than `MIN_HOURS_BETWEEN_COORDINATED_RUNS` have elapsed
///     since the last successful pass for this agent;
///   * accept any Auto-Dream-flagged records (`metadata.dreamed_at`)
///     unconditionally — Auto Dream already did the consolidation work.
///
/// `Always` is the pre-v0.3.1 behaviour: run every phase every time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReflectionMode {
    #[default]
    Coordinated,
    Always,
}

/// Minimum number of newly-written records (since `last_reflection_at`) that
/// must exist before a `Coordinated` pass runs. Matches Auto Dream's floor.
pub const MIN_NEW_RECORDS_FOR_COORDINATED_RUN: usize = 5;

/// Minimum interval (in hours) between `Coordinated` passes for one agent.
/// Matches Auto Dream's 24h cadence.
pub const MIN_HOURS_BETWEEN_COORDINATED_RUNS: i64 = 24;

/// Why a `Coordinated` pass decided to skip, when it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    TooSoon,
    NotEnoughNewRecords,
    AutoDreamAlreadyRan,
}

/// Result of a single reflection pass.
#[non_exhaustive]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReflectionReport {
    /// Number of duplicate pairs consolidated into a surviving record.
    pub consolidated: usize,
    /// Number of records whose content had at least one relative date
    /// rewritten to ISO-8601.
    pub absolutized_dates: usize,
    /// Number of externally-rewritten records (e.g. Auto Dream) accepted
    /// and re-embedded during this pass.
    pub dreamed_accepted: usize,
    /// Number of records moved to `Archived` state.
    pub archived: usize,
    /// Number of conflict pairs auto-resolved.
    pub conflicts_resolved: usize,
    /// Total records scanned.
    pub total_scanned: usize,
    /// Populated when `Coordinated` skipped the run; `None` means the pass
    /// actually executed.
    pub skipped: Option<SkipReason>,
    /// Merged/removed/re-indexed counts pulled from an Auto Dream
    /// organization-report trailer, when present. Emitted as a single
    /// `DreamReportIngested` event per agent; subsequent passes see the
    /// idempotent `dream_report_ingested_at` marker in metadata and skip
    /// re-parsing.
    pub dream_report_ingested: usize,
}

/// Run a reflection pass honouring `mode`.
///
/// `ReflectionMode::Coordinated` (the default) checks three gates before
/// running the expensive phases; see [`ReflectionMode`] for details.
/// When a gate trips, the returned report has `skipped = Some(reason)`
/// and no state is mutated. Pass `force=true` to override cadence gates.
pub async fn run_reflection_pass_with_mode(
    engine: &MnemoEngine,
    agent_id: &str,
    mode: ReflectionMode,
    force: bool,
) -> Result<ReflectionReport> {
    if mode == ReflectionMode::Coordinated && !force {
        if let Some(reason) = coordinated_skip_reason(engine, agent_id).await? {
            return Ok(ReflectionReport {
                skipped: Some(reason),
                ..Default::default()
            });
        }
    }
    let mut report = run_reflection_pass_inner(engine, agent_id).await?;
    emit_reflection_completed(engine, agent_id, &report).await;
    // Ingest any Auto Dream organization-report trailer now that the
    // record list reflects this pass. Idempotent via metadata.
    report.dream_report_ingested = ingest_dream_reports(engine, agent_id).await?;
    Ok(report)
}

/// Back-compat entry point — runs the reflection pass unconditionally
/// (equivalent to `run_reflection_pass_with_mode(_, _, Always, true)`).
pub async fn run_reflection_pass(engine: &MnemoEngine, agent_id: &str) -> Result<ReflectionReport> {
    run_reflection_pass_with_mode(engine, agent_id, ReflectionMode::Always, true).await
}

/// The original pass body, now private. `run_reflection_pass_with_mode`
/// wraps this with the Coordinated gates.
async fn run_reflection_pass_inner(
    engine: &MnemoEngine,
    agent_id: &str,
) -> Result<ReflectionReport> {
    let filter = MemoryFilter {
        agent_id: Some(agent_id.to_string()),
        include_deleted: false,
        ..Default::default()
    };
    let records = engine
        .storage
        .list_memories(&filter, super::MAX_BATCH_QUERY_LIMIT, 0)
        .await?;

    let total_scanned = records.len();
    let mut report = ReflectionReport {
        total_scanned,
        ..Default::default()
    };

    // -- 1. Date absolutization ---------------------------------------------
    let mut after_absolutization: Vec<MemoryRecord> = Vec::with_capacity(records.len());
    for mut record in records {
        let rewritten = absolutize_dates(&record.content, &record.created_at);
        if let Some(new_content) = rewritten {
            let prev_hash = record.content_hash.clone();
            record.content = new_content;
            record.updated_at = chrono::Utc::now().to_rfc3339();
            record.content_hash =
                compute_content_hash(&record.content, &record.agent_id, &record.updated_at);
            // Re-embed on content change. Embedding failure is non-fatal —
            // the cached embedding still beats a skipped reflection.
            if let Ok(emb) = engine.embedding.embed(&record.content).await {
                record.embedding = Some(emb.clone());
                let _ = engine.index.add(record.id, &emb);
            }
            engine.storage.update_memory(&record).await?;
            emit_rewrite_event(
                engine,
                agent_id,
                record.id,
                "date_absolutization",
                &prev_hash,
                &record.content_hash,
            )
            .await;
            report.absolutized_dates += 1;
        }
        after_absolutization.push(record);
    }

    // -- 2. Auto-Dream accept ----------------------------------------------
    // An external rewrite is signalled by `metadata.dreamed_at`: the Claude
    // Agent SDK bridge writes this when it detects an Opus-driven edit on a
    // memory file. We re-embed and emit a MemoryDreamed audit event but do
    // NOT rewrite the content — the bridge already did that.
    for record in &mut after_absolutization {
        if record
            .metadata
            .get("dreamed_at")
            .and_then(|v| v.as_str())
            .is_some()
            && record
                .metadata
                .get("dreamed_processed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                == false
        {
            let prev_hash = record.content_hash.clone();
            record.content_hash =
                compute_content_hash(&record.content, &record.agent_id, &record.updated_at);
            if let Ok(emb) = engine.embedding.embed(&record.content).await {
                record.embedding = Some(emb.clone());
                let _ = engine.index.add(record.id, &emb);
            }
            if let Some(obj) = record.metadata.as_object_mut() {
                obj.insert(
                    "dreamed_processed".to_string(),
                    serde_json::Value::Bool(true),
                );
            }
            engine.storage.update_memory(record).await?;
            emit_rewrite_event(
                engine,
                agent_id,
                record.id,
                "auto_dream",
                &prev_hash,
                &record.content_hash,
            )
            .await;
            report.dreamed_accepted += 1;
        }
    }

    // -- 3. Semantic dedup --------------------------------------------------
    let consolidated_ids = consolidate_duplicates(engine, &mut after_absolutization).await?;
    report.consolidated = consolidated_ids.len();

    // -- 4. Low-importance conflict resolution ------------------------------
    let conflicts = engine
        .detect_conflicts(Some(agent_id.to_string()), DEFAULT_DEDUP_THRESHOLD)
        .await?;
    for pair in &conflicts.conflicts {
        let (a, b) = match (
            after_absolutization.iter().find(|r| r.id == pair.memory_a),
            after_absolutization.iter().find(|r| r.id == pair.memory_b),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        if a.importance < DEFAULT_LOW_IMPORTANCE_CUTOFF
            && b.importance < DEFAULT_LOW_IMPORTANCE_CUTOFF
            && engine
                .resolve_conflict(pair, ResolutionStrategy::KeepNewest)
                .await
                .is_ok()
        {
            report.conflicts_resolved += 1;
        }
    }
    let _ = &conflicts; // keep the borrow alive for the closure above

    // -- 5. Stale archival --------------------------------------------------
    let now = chrono::Utc::now();
    for record in after_absolutization {
        if consolidated_ids.contains(&record.id) {
            continue;
        }
        if record.consolidation_state == ConsolidationState::Archived {
            continue;
        }
        if record.access_count > 0 {
            continue;
        }
        if effective_importance(&record) >= DEFAULT_ARCHIVE_IMPORTANCE {
            continue;
        }
        let Ok(created) = chrono::DateTime::parse_from_rfc3339(&record.created_at) else {
            continue;
        };
        let age_hours = (now - created.with_timezone(&chrono::Utc)).num_seconds() as f64 / 3600.0;
        if age_hours < DEFAULT_ARCHIVE_AGE_HOURS {
            continue;
        }
        let mut updated = record.clone();
        updated.consolidation_state = ConsolidationState::Archived;
        updated.updated_at = now.to_rfc3339();
        if engine.storage.update_memory(&updated).await.is_ok() {
            report.archived += 1;
        }
    }

    Ok(report)
}

/// Absolutize relative temporal expressions in `content`. Returns `Some` when
/// the content was modified.
pub fn absolutize_dates(content: &str, created_at_rfc3339: &str) -> Option<String> {
    let anchor = chrono::DateTime::parse_from_rfc3339(created_at_rfc3339)
        .ok()?
        .with_timezone(&chrono::Utc);
    let mut out = content.to_string();
    let mut modified = false;

    // Whole-word replacements.
    let simple: &[(&str, i64)] = &[
        ("yesterday", -1),
        ("today", 0),
        ("tomorrow", 1),
        ("last week", -7),
        ("next week", 7),
    ];
    for (needle, days) in simple {
        let re = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(needle))).ok()?;
        if re.is_match(&out) {
            let target = anchor + chrono::Duration::days(*days);
            out = re
                .replace_all(&out, target.format("%Y-%m-%d").to_string())
                .into_owned();
            modified = true;
        }
    }

    // "<N> days/weeks ago" and "in <N> days/weeks"
    let re_ago = Regex::new(r"(?i)\b(\d+)\s+(day|days|week|weeks)\s+ago\b").ok()?;
    out = re_ago
        .replace_all(&out, |caps: &regex::Captures<'_>| {
            let n: i64 = caps[1].parse().unwrap_or(0);
            let unit = caps[2].to_lowercase();
            let days = if unit.starts_with("week") { n * 7 } else { n };
            let target = anchor - chrono::Duration::days(days);
            modified = true;
            target.format("%Y-%m-%d").to_string()
        })
        .into_owned();

    let re_in = Regex::new(r"(?i)\bin\s+(\d+)\s+(day|days|week|weeks)\b").ok()?;
    out = re_in
        .replace_all(&out, |caps: &regex::Captures<'_>| {
            let n: i64 = caps[1].parse().unwrap_or(0);
            let unit = caps[2].to_lowercase();
            let days = if unit.starts_with("week") { n * 7 } else { n };
            let target = anchor + chrono::Duration::days(days);
            modified = true;
            target.format("%Y-%m-%d").to_string()
        })
        .into_owned();

    if modified { Some(out) } else { None }
}

/// Cosine similarity between two float vectors. Returns 0.0 when either is
/// empty or the norm is zero.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

/// Consolidate near-duplicate memories. The newer record wins; the older
/// one is marked `Consolidated` and a `consolidated_from` relation linking
/// them is stored for audit. Returns the ids that were collapsed (older
/// sides).
async fn consolidate_duplicates(
    engine: &MnemoEngine,
    records: &mut [MemoryRecord],
) -> Result<HashSet<Uuid>> {
    let mut consolidated: HashSet<Uuid> = HashSet::new();

    for i in 0..records.len() {
        if consolidated.contains(&records[i].id) {
            continue;
        }
        for j in (i + 1)..records.len() {
            if consolidated.contains(&records[j].id) {
                continue;
            }
            let (Some(emb_i), Some(emb_j)) =
                (records[i].embedding.as_ref(), records[j].embedding.as_ref())
            else {
                continue;
            };
            if cosine(emb_i, emb_j) < DEFAULT_DEDUP_THRESHOLD {
                continue;
            }

            // Newer side wins. Ties break toward `records[i]` so the scan is
            // deterministic.
            let (keeper_idx, victim_idx) = match records[i].created_at.cmp(&records[j].created_at) {
                std::cmp::Ordering::Less => (j, i),
                _ => (i, j),
            };

            // Union of tags; sum of access_count.
            let mut keeper = records[keeper_idx].clone();
            let victim = records[victim_idx].clone();
            for tag in &victim.tags {
                if !keeper.tags.contains(tag) {
                    keeper.tags.push(tag.clone());
                }
            }
            keeper.access_count = keeper.access_count.saturating_add(victim.access_count);
            keeper.updated_at = chrono::Utc::now().to_rfc3339();
            engine.storage.update_memory(&keeper).await?;

            let mut v_updated = victim.clone();
            v_updated.consolidation_state = ConsolidationState::Consolidated;
            v_updated.updated_at = keeper.updated_at.clone();
            engine.storage.update_memory(&v_updated).await?;

            let rel = Relation {
                id: Uuid::now_v7(),
                source_id: keeper.id,
                target_id: victim.id,
                relation_type: "consolidated_from".to_string(),
                weight: 1.0,
                metadata: serde_json::json!({"reason": "semantic_dedup"}),
                created_at: keeper.updated_at.clone(),
            };
            let _ = engine.storage.insert_relation(&rel).await;

            consolidated.insert(victim.id);
            // Replace the slice entry so subsequent iterations see the
            // merged state.
            records[keeper_idx] = keeper;
        }
    }

    Ok(consolidated)
}

async fn emit_rewrite_event(
    engine: &MnemoEngine,
    agent_id: &str,
    memory_id: Uuid,
    reason: &str,
    prev_hash: &[u8],
    new_hash: &[u8],
) {
    let now = chrono::Utc::now().to_rfc3339();
    let content_hash =
        compute_content_hash(&format!("rewrite:{memory_id}:{reason}"), agent_id, &now);
    let prev_event_hash = engine
        .storage
        .get_latest_event_hash(agent_id, None)
        .await
        .ok()
        .flatten();
    let event = AgentEvent {
        id: Uuid::now_v7(),
        agent_id: agent_id.to_string(),
        thread_id: None,
        run_id: None,
        parent_event_id: None,
        event_type: if reason == "auto_dream" {
            EventType::MemoryRedact
        } else {
            EventType::MemoryWrite
        },
        payload: serde_json::json!({
            "memory_id": memory_id.to_string(),
            "reason": reason,
            "prev_hash": hex_encode(prev_hash),
            "new_hash": hex_encode(new_hash),
        }),
        trace_id: None,
        span_id: None,
        model: None,
        tokens_input: None,
        tokens_output: None,
        latency_ms: None,
        cost_usd: None,
        timestamp: now,
        logical_clock: 0,
        content_hash: content_hash.clone(),
        prev_hash: Some(compute_chain_hash(
            &content_hash,
            prev_event_hash.as_deref(),
        )),
        embedding: None,
    };
    let _ = engine.storage.insert_event(&event).await;
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Read the last `ReflectionCompleted` event for `agent_id` and return its
/// timestamp. When no such event exists, returns `None`.
async fn last_reflection_at(
    engine: &MnemoEngine,
    agent_id: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    let events = engine.storage.list_events(agent_id, 1000, 0).await?;
    for event in events {
        if event.event_type == EventType::ReflectionCompleted
            && let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&event.timestamp)
        {
            return Ok(Some(ts.with_timezone(&chrono::Utc)));
        }
    }
    Ok(None)
}

/// Gate the Coordinated path on cadence + new-record floor. Returns
/// `Some(reason)` when the pass should skip.
async fn coordinated_skip_reason(
    engine: &MnemoEngine,
    agent_id: &str,
) -> Result<Option<SkipReason>> {
    let last = last_reflection_at(engine, agent_id).await?;
    let now = chrono::Utc::now();
    if let Some(last_ts) = last {
        if (now - last_ts).num_hours() < MIN_HOURS_BETWEEN_COORDINATED_RUNS {
            return Ok(Some(SkipReason::TooSoon));
        }
    }

    // Count records created since the last reflection (or all records if
    // this is the first pass).
    let since = last.map(|t| t.to_rfc3339());
    let filter = MemoryFilter {
        agent_id: Some(agent_id.to_string()),
        include_deleted: false,
        ..Default::default()
    };
    let records = engine
        .storage
        .list_memories(&filter, super::MAX_BATCH_QUERY_LIMIT, 0)
        .await?;
    let new_count = records
        .iter()
        .filter(|r| match since.as_deref() {
            None => true,
            Some(cutoff) => r.created_at.as_str() > cutoff,
        })
        .count();
    if new_count < MIN_NEW_RECORDS_FOR_COORDINATED_RUN {
        return Ok(Some(SkipReason::NotEnoughNewRecords));
    }

    // If Auto Dream already rewrote records we'd otherwise consolidate
    // this cycle, skip our own consolidation to avoid double-work. The
    // signal is any record whose metadata.dreamed_at is newer than
    // `last_reflection_at` AND hasn't been dreamed-processed yet. We
    // still want the pass to RUN (to re-embed + ingest the report), so
    // this branch doesn't actually skip — return None.
    Ok(None)
}

/// Emit a hash-linked `ReflectionCompleted` event so the next Coordinated
/// run can read its timestamp.
async fn emit_reflection_completed(
    engine: &MnemoEngine,
    agent_id: &str,
    report: &ReflectionReport,
) {
    let now = chrono::Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "consolidated": report.consolidated,
        "absolutized_dates": report.absolutized_dates,
        "dreamed_accepted": report.dreamed_accepted,
        "archived": report.archived,
        "conflicts_resolved": report.conflicts_resolved,
        "total_scanned": report.total_scanned,
    });
    let content_hash = compute_content_hash(&payload.to_string(), agent_id, &now);
    let prev_event_hash = engine
        .storage
        .get_latest_event_hash(agent_id, None)
        .await
        .ok()
        .flatten();
    let event = AgentEvent {
        id: Uuid::now_v7(),
        agent_id: agent_id.to_string(),
        thread_id: None,
        run_id: None,
        parent_event_id: None,
        event_type: EventType::ReflectionCompleted,
        payload,
        trace_id: None,
        span_id: None,
        model: None,
        tokens_input: None,
        tokens_output: None,
        latency_ms: None,
        cost_usd: None,
        timestamp: now,
        logical_clock: 0,
        content_hash: content_hash.clone(),
        prev_hash: Some(compute_chain_hash(
            &content_hash,
            prev_event_hash.as_deref(),
        )),
        embedding: None,
    };
    let _ = engine.storage.insert_event(&event).await;
}

/// Auto Dream organization-report trailer. Parser is permissive — matches
/// whichever of these three keys appears: `consolidated`, `removed`,
/// `reindexed` (case-insensitive, values are base-10 integers after a
/// colon or equals sign). Missing keys default to zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DreamReport {
    pub consolidated: u32,
    pub removed: u32,
    pub reindexed: u32,
}

/// Parse an Auto Dream organization-report trailer from free text. Returns
/// `None` when no `## Organization Report` header is present.
pub fn parse_organization_report(text: &str) -> Option<DreamReport> {
    let lower = text.to_lowercase();
    let marker = "## organization report";
    let start = lower.find(marker)?;
    let trailer = &text[start + marker.len()..];
    let re_consolidated =
        regex::Regex::new(r"(?i)\bconsolidated\b\s*[:=]\s*(\d+)").ok()?;
    let re_removed = regex::Regex::new(r"(?i)\bremoved\b\s*[:=]\s*(\d+)").ok()?;
    let re_reindexed =
        regex::Regex::new(r"(?i)\bre[-_ ]?indexed\b\s*[:=]\s*(\d+)").ok()?;
    let consolidated = re_consolidated
        .captures(trailer)
        .and_then(|c| c.get(1).and_then(|m| m.as_str().parse().ok()))
        .unwrap_or(0);
    let removed = re_removed
        .captures(trailer)
        .and_then(|c| c.get(1).and_then(|m| m.as_str().parse().ok()))
        .unwrap_or(0);
    let reindexed = re_reindexed
        .captures(trailer)
        .and_then(|c| c.get(1).and_then(|m| m.as_str().parse().ok()))
        .unwrap_or(0);
    Some(DreamReport {
        consolidated,
        removed,
        reindexed,
    })
}

/// Walk records whose content carries an Auto Dream organization-report
/// trailer; extract the counts, emit `DreamReportIngested`, and mark the
/// record so the next pass skips it. Returns the number of records
/// processed this pass.
async fn ingest_dream_reports(engine: &MnemoEngine, agent_id: &str) -> Result<usize> {
    let filter = MemoryFilter {
        agent_id: Some(agent_id.to_string()),
        include_deleted: false,
        ..Default::default()
    };
    let records = engine
        .storage
        .list_memories(&filter, super::MAX_BATCH_QUERY_LIMIT, 0)
        .await?;
    let mut ingested = 0usize;
    for mut record in records {
        if record
            .metadata
            .get("dream_report_ingested_at")
            .and_then(|v| v.as_str())
            .is_some()
        {
            continue;
        }
        let Some(report) = parse_organization_report(&record.content) else {
            continue;
        };
        let now = chrono::Utc::now().to_rfc3339();
        if let Some(obj) = record.metadata.as_object_mut() {
            obj.insert(
                "dream_report_ingested_at".to_string(),
                serde_json::Value::String(now.clone()),
            );
        }
        record.updated_at = now.clone();
        if engine.storage.update_memory(&record).await.is_ok() {
            ingested += 1;
            let payload = serde_json::json!({
                "memory_id": record.id.to_string(),
                "consolidated": report.consolidated,
                "removed": report.removed,
                "reindexed": report.reindexed,
            });
            let content_hash = compute_content_hash(&payload.to_string(), agent_id, &now);
            let prev_event_hash = engine
                .storage
                .get_latest_event_hash(agent_id, None)
                .await
                .ok()
                .flatten();
            let event = AgentEvent {
                id: Uuid::now_v7(),
                agent_id: agent_id.to_string(),
                thread_id: None,
                run_id: None,
                parent_event_id: None,
                event_type: EventType::DreamReportIngested,
                payload,
                trace_id: None,
                span_id: None,
                model: None,
                tokens_input: None,
                tokens_output: None,
                latency_ms: None,
                cost_usd: None,
                timestamp: now,
                logical_clock: 0,
                content_hash: content_hash.clone(),
                prev_hash: Some(compute_chain_hash(
                    &content_hash,
                    prev_event_hash.as_deref(),
                )),
                embedding: None,
            };
            let _ = engine.storage.insert_event(&event).await;
        }
    }
    Ok(ingested)
}

// Unit tests live alongside the engine integration suite so the reflection
// pass can exercise the whole remember → list → reflect round-trip.
