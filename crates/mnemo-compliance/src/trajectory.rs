//! GEM-aligned trajectory-correctness audit over the hash-chained
//! event log.
//!
//! # Anchor
//!
//! GEM (arXiv:2605.26252) frames memory correctness as four
//! *trajectory-level* failure modes that a per-record verifier
//! (mnemo's existing `verify_integrity` / `hash::verify_event_chain`
//! pair) cannot catch:
//!
//! - **Unregulated growth** — the active bank grows without a
//!   policy-driven ceiling, so the agent's working set expands until
//!   recall latency or eviction artifacts surface.
//! - **Missing semantic revision** — a fact is superseded by a newer
//!   contradictory write but the older row is never revised or
//!   forgotten; stale facts coexist with current ones in the bank.
//! - **Capacity-driven forgetting** — `MemoryDelete` events fire that
//!   are not labelled with one of the five named forget strategies
//!   (`soft_delete` / `hard_delete` / `decay` / `consolidate` /
//!   `archive`); typically a sign of out-of-policy eviction.
//! - **Read-only retrieval** — a scope only ever issues `MemoryRead`
//!   events, never `MemoryWrite` / `MemoryDelete` / `MemoryRedact`.
//!   The agent reads but never revises, so any drift in the source
//!   data is silently inherited.
//!
//! This module is the trajectory-shaped complement to
//! [`crate::export_audit_log`]: same sync-over-`&[AgentEvent]` shape,
//! same `ComplianceError` return type, and consumed by the protocol
//! crates the same way (`mnemo-mcp`, `mnemo-rest`, `mnemo-grpc`).
//!
//! # What this audit is NOT
//!
//! - **Not a record-level integrity check.** The hash-chain audit
//!   already covered by [`mnemo_core::hash::verify_event_chain`] and
//!   `MnemoEngine::verify_integrity` answers *"was a row tampered?"*
//!   This module answers the orthogonal *"did the trajectory
//!   regulate itself?"* question. Use both.
//! - **Not a recall-quality benchmark.** No retrieval is run; only
//!   the event log is consumed.
//! - **Not a write-side guard.** Findings are advisory — callers
//!   decide whether to gate `remember` / `forget` on them.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mnemo_core::model::event::{AgentEvent, EventType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ComplianceError;

/// Parameters for [`trajectory_audit`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryAuditRequest {
    /// Optional agent scope filter. When `None`, every distinct
    /// `agent_id` seen in the event slice contributes to each signal
    /// independently.
    pub agent_id: Option<String>,
    /// Optional thread scope filter applied alongside `agent_id`.
    pub thread_id: Option<String>,
    /// Active-bank-size ceiling used by signal (a). A timeline sample
    /// exceeding this value contributes to the `breach_count`. Default
    /// `1024`.
    #[serde(default = "default_ceiling")]
    pub active_bank_ceiling: usize,
    /// Payload key consulted by signal (b) to detect a "same fact,
    /// newer write" supersession. Defaults to `"fact_id"`, matching
    /// the convention used by the v0.4.7 current-fact resolver.
    #[serde(default = "default_fact_key")]
    pub fact_key: String,
    /// Forget strategies recognised as *policy-driven* by signal (c).
    /// Any `MemoryDelete` event whose payload `strategy` field is not
    /// in this set is flagged as capacity-driven. Defaults to the
    /// five canonical strategies.
    #[serde(default = "default_named_strategies")]
    pub named_forget_strategies: Vec<String>,
}

fn default_ceiling() -> usize {
    1024
}
fn default_fact_key() -> String {
    "fact_id".to_string()
}
fn default_named_strategies() -> Vec<String> {
    vec![
        "soft_delete".to_string(),
        "hard_delete".to_string(),
        "decay".to_string(),
        "consolidate".to_string(),
        "archive".to_string(),
    ]
}

impl Default for TrajectoryAuditRequest {
    fn default() -> Self {
        Self {
            agent_id: None,
            thread_id: None,
            active_bank_ceiling: default_ceiling(),
            fact_key: default_fact_key(),
            named_forget_strategies: default_named_strategies(),
        }
    }
}

/// Per-signal severity tier. `Ok` = no evidence; `Warn` = at least
/// one occurrence but bounded; `Fail` = breaches the threshold the
/// signal documents in its rustdoc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Ok,
    Warn,
    Fail,
}

/// Signal (a): active-bank size over time vs the caller's ceiling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnregulatedGrowthFinding {
    pub severity: Severity,
    /// Number of timeline samples whose active size > `ceiling`.
    pub breach_count: usize,
    /// Peak active-bank size observed across the event window.
    pub peak_active_size: usize,
    /// Active-bank-size timeline `(timestamp_rfc3339, size_after_event)`,
    /// emitted only at `MemoryWrite` / `MemoryDelete` / `MemoryExpired`
    /// / `MemoryRedact` boundaries (the events that mutate the bank).
    pub timeline: Vec<(String, usize)>,
}

/// Signal (b): facts superseded by newer contradictory writes that
/// were never revised or forgotten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingSemanticRevisionFinding {
    pub severity: Severity,
    /// Stale facts: one entry per (`fact_id`, list of older memory ids
    /// still in the bank after a newer write).
    pub stale_facts: Vec<StaleFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleFact {
    pub fact_id: String,
    /// `MemoryWrite` event ids that wrote this fact, in chronological
    /// order. All but the last are stale survivors when no
    /// `MemoryDelete` / `MemoryRedact` covers them.
    pub write_event_ids: Vec<Uuid>,
    /// Memory ids of the stale survivors (every write_event except the
    /// final one, minus any covered by a later `MemoryDelete` /
    /// `MemoryRedact`).
    pub stale_memory_ids: Vec<Uuid>,
}

/// Signal (c): forget events not tagged with one of the five named
/// strategies — typically capacity-driven eviction, which v0.4.x
/// considers out-of-policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityDrivenForgettingFinding {
    pub severity: Severity,
    /// `MemoryDelete` events with no `strategy` payload field, or a
    /// strategy outside `named_forget_strategies`.
    pub unlabelled_forget_event_ids: Vec<Uuid>,
    /// Histogram of strategies seen on `MemoryDelete` events,
    /// including the special `<unlabelled>` bucket.
    pub strategy_histogram: BTreeMap<String, usize>,
}

/// Signal (d): scopes that only ever recall and never revise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadOnlyRetrievalFinding {
    pub severity: Severity,
    /// Scope keys (`agent_id` or `agent_id::thread_id`) with at least
    /// one `MemoryRead` but zero write-shaped events.
    pub read_only_scopes: Vec<String>,
}

/// Complete trajectory-correctness audit report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryAuditReport {
    pub scope_label: String,
    pub event_count: usize,
    pub unregulated_growth: UnregulatedGrowthFinding,
    pub missing_semantic_revision: MissingSemanticRevisionFinding,
    pub capacity_driven_forgetting: CapacityDrivenForgettingFinding,
    pub read_only_retrieval: ReadOnlyRetrievalFinding,
}

impl TrajectoryAuditReport {
    /// `true` when every signal reports [`Severity::Ok`]. Useful as a
    /// one-liner gate in callers that wrap this audit behind a CI
    /// step.
    pub fn all_ok(&self) -> bool {
        matches!(self.unregulated_growth.severity, Severity::Ok)
            && matches!(self.missing_semantic_revision.severity, Severity::Ok)
            && matches!(self.capacity_driven_forgetting.severity, Severity::Ok)
            && matches!(self.read_only_retrieval.severity, Severity::Ok)
    }
}

/// Run the four-signal GEM trajectory-correctness audit.
///
/// `events` must be in chronological order — callers fetching via
/// `engine.storage.list_events(agent_id, limit, 0)` get DESC order and
/// must `.reverse()` first, exactly mirroring the contract of
/// [`crate::export_audit_log`].
pub fn trajectory_audit(
    events: &[AgentEvent],
    request: &TrajectoryAuditRequest,
) -> Result<TrajectoryAuditReport, ComplianceError> {
    let filtered: Vec<&AgentEvent> = events
        .iter()
        .filter(|e| match request.agent_id.as_deref() {
            Some(want) => e.agent_id == want,
            None => true,
        })
        .filter(|e| match request.thread_id.as_deref() {
            Some(want) => e.thread_id.as_deref() == Some(want),
            None => true,
        })
        .collect();

    if filtered.is_empty() {
        return Err(ComplianceError::EmptyAuditWindow);
    }

    let scope_label = match (&request.agent_id, &request.thread_id) {
        (Some(a), Some(t)) => format!("agent={a},thread={t}"),
        (Some(a), None) => format!("agent={a}"),
        (None, Some(t)) => format!("thread={t}"),
        (None, None) => "all".to_string(),
    };

    let unregulated_growth = audit_unregulated_growth(&filtered, request.active_bank_ceiling);
    let missing_semantic_revision = audit_missing_semantic_revision(&filtered, &request.fact_key);
    let capacity_driven_forgetting =
        audit_capacity_driven_forgetting(&filtered, &request.named_forget_strategies);
    let read_only_retrieval = audit_read_only_retrieval(&filtered);

    Ok(TrajectoryAuditReport {
        scope_label,
        event_count: filtered.len(),
        unregulated_growth,
        missing_semantic_revision,
        capacity_driven_forgetting,
        read_only_retrieval,
    })
}

// ---------------------------------------------------------------------------
// Signal (a): unregulated growth
// ---------------------------------------------------------------------------

fn audit_unregulated_growth(events: &[&AgentEvent], ceiling: usize) -> UnregulatedGrowthFinding {
    let mut active = 0_i64;
    let mut peak = 0_usize;
    let mut breaches = 0_usize;
    let mut timeline = Vec::new();

    for e in events {
        match e.event_type {
            EventType::MemoryWrite => active += 1,
            // SoftDelete / HardDelete / Archive / Consolidate all
            // pass through MemoryDelete; Expired + Redact remove the
            // record from the active bank as well.
            EventType::MemoryDelete | EventType::MemoryExpired | EventType::MemoryRedact => {
                active = (active - 1).max(0);
            }
            _ => continue,
        }
        let size = active as usize;
        if size > peak {
            peak = size;
        }
        if size > ceiling {
            breaches += 1;
        }
        timeline.push((e.timestamp.clone(), size));
    }

    let severity = if breaches == 0 {
        Severity::Ok
    } else if peak <= ceiling.saturating_mul(2) {
        Severity::Warn
    } else {
        Severity::Fail
    };

    UnregulatedGrowthFinding {
        severity,
        breach_count: breaches,
        peak_active_size: peak,
        timeline,
    }
}

// ---------------------------------------------------------------------------
// Signal (b): missing semantic revision
// ---------------------------------------------------------------------------

fn audit_missing_semantic_revision(
    events: &[&AgentEvent],
    fact_key: &str,
) -> MissingSemanticRevisionFinding {
    // For each fact_id, collect the chronological list of writes
    // (event_id + memory_id) and any deletes/redacts that retire a
    // memory.
    #[derive(Default)]
    struct FactState {
        writes: Vec<(Uuid, Uuid)>, // (event_id, memory_id)
    }
    let mut by_fact: HashMap<String, FactState> = HashMap::new();
    let mut retired_memory_ids: BTreeSet<Uuid> = BTreeSet::new();

    for e in events {
        match e.event_type {
            EventType::MemoryWrite => {
                let Some(fact_id) = payload_str(&e.payload, fact_key) else {
                    continue;
                };
                let Some(memory_id) = payload_uuid(&e.payload, "memory_id") else {
                    continue;
                };
                by_fact
                    .entry(fact_id)
                    .or_default()
                    .writes
                    .push((e.id, memory_id));
            }
            EventType::MemoryDelete | EventType::MemoryRedact | EventType::MemoryExpired => {
                if let Some(mid) = payload_uuid(&e.payload, "memory_id") {
                    retired_memory_ids.insert(mid);
                }
            }
            _ => continue,
        }
    }

    let mut stale_facts = Vec::new();
    for (fact_id, state) in by_fact {
        if state.writes.len() < 2 {
            continue;
        }
        // Every write except the most recent is a potential stale
        // survivor; subtract those that were explicitly retired.
        let last_idx = state.writes.len() - 1;
        let stale_memory_ids: Vec<Uuid> = state
            .writes
            .iter()
            .take(last_idx)
            .map(|(_, mid)| *mid)
            .filter(|mid| !retired_memory_ids.contains(mid))
            .collect();
        if stale_memory_ids.is_empty() {
            continue;
        }
        stale_facts.push(StaleFact {
            fact_id,
            write_event_ids: state.writes.iter().map(|(eid, _)| *eid).collect(),
            stale_memory_ids,
        });
    }
    stale_facts.sort_by(|a, b| a.fact_id.cmp(&b.fact_id));

    let severity = match stale_facts.len() {
        0 => Severity::Ok,
        1..=5 => Severity::Warn,
        _ => Severity::Fail,
    };

    MissingSemanticRevisionFinding {
        severity,
        stale_facts,
    }
}

// ---------------------------------------------------------------------------
// Signal (c): capacity-driven forgetting
// ---------------------------------------------------------------------------

fn audit_capacity_driven_forgetting(
    events: &[&AgentEvent],
    named_strategies: &[String],
) -> CapacityDrivenForgettingFinding {
    let named: BTreeSet<&str> = named_strategies.iter().map(|s| s.as_str()).collect();
    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
    let mut unlabelled = Vec::new();

    for e in events {
        if e.event_type != EventType::MemoryDelete {
            continue;
        }
        let strategy = payload_str(&e.payload, "strategy");
        match strategy {
            Some(s) if named.contains(s.as_str()) => {
                *histogram.entry(s).or_insert(0) += 1;
            }
            Some(s) => {
                *histogram.entry(s).or_insert(0) += 1;
                unlabelled.push(e.id);
            }
            None => {
                *histogram.entry("<unlabelled>".to_string()).or_insert(0) += 1;
                unlabelled.push(e.id);
            }
        }
    }

    let severity = match unlabelled.len() {
        0 => Severity::Ok,
        1..=3 => Severity::Warn,
        _ => Severity::Fail,
    };

    CapacityDrivenForgettingFinding {
        severity,
        unlabelled_forget_event_ids: unlabelled,
        strategy_histogram: histogram,
    }
}

// ---------------------------------------------------------------------------
// Signal (d): read-only retrieval
// ---------------------------------------------------------------------------

fn audit_read_only_retrieval(events: &[&AgentEvent]) -> ReadOnlyRetrievalFinding {
    // Bucket by scope key.
    let mut reads_by_scope: BTreeMap<String, usize> = BTreeMap::new();
    let mut writes_by_scope: BTreeMap<String, usize> = BTreeMap::new();
    for e in events {
        let key = scope_key(e);
        match e.event_type {
            EventType::MemoryRead | EventType::RetrievalQuery | EventType::RetrievalResult => {
                *reads_by_scope.entry(key).or_insert(0) += 1;
            }
            EventType::MemoryWrite
            | EventType::MemoryDelete
            | EventType::MemoryRedact
            | EventType::MemoryExpired
            | EventType::MemoryShare => {
                *writes_by_scope.entry(key).or_insert(0) += 1;
            }
            _ => continue,
        }
    }

    let mut read_only: Vec<String> = reads_by_scope
        .into_iter()
        .filter(|(k, _)| !writes_by_scope.contains_key(k))
        .map(|(k, _)| k)
        .collect();
    read_only.sort();

    let severity = match read_only.len() {
        0 => Severity::Ok,
        1..=2 => Severity::Warn,
        _ => Severity::Fail,
    };

    ReadOnlyRetrievalFinding {
        severity,
        read_only_scopes: read_only,
    }
}

fn scope_key(e: &AgentEvent) -> String {
    match e.thread_id.as_deref() {
        Some(t) => format!("{}::{}", e.agent_id, t),
        None => e.agent_id.clone(),
    }
}

fn payload_str(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn payload_uuid(payload: &serde_json::Value, key: &str) -> Option<Uuid> {
    let s = payload.get(key).and_then(|v| v.as_str())?;
    Uuid::parse_str(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use mnemo_core::model::event::EventType;

    fn write(offset_secs: i64, agent: &str, memory_id: Uuid, fact_id: Option<&str>) -> AgentEvent {
        let mut payload = serde_json::json!({ "memory_id": memory_id.to_string() });
        if let Some(f) = fact_id {
            payload
                .as_object_mut()
                .unwrap()
                .insert("fact_id".to_string(), serde_json::json!(f));
        }
        AgentEvent {
            id: Uuid::now_v7(),
            agent_id: agent.to_string(),
            thread_id: None,
            run_id: None,
            parent_event_id: None,
            event_type: EventType::MemoryWrite,
            payload,
            trace_id: None,
            span_id: None,
            model: None,
            tokens_input: None,
            tokens_output: None,
            latency_ms: None,
            cost_usd: None,
            timestamp: (Utc::now() + Duration::seconds(offset_secs)).to_rfc3339(),
            logical_clock: offset_secs,
            content_hash: vec![],
            prev_hash: None,
            embedding: None,
        }
    }

    fn delete(
        offset_secs: i64,
        agent: &str,
        memory_id: Uuid,
        strategy: Option<&str>,
    ) -> AgentEvent {
        let mut payload = serde_json::json!({ "memory_id": memory_id.to_string() });
        if let Some(s) = strategy {
            payload
                .as_object_mut()
                .unwrap()
                .insert("strategy".to_string(), serde_json::json!(s));
        }
        AgentEvent {
            id: Uuid::now_v7(),
            agent_id: agent.to_string(),
            thread_id: None,
            run_id: None,
            parent_event_id: None,
            event_type: EventType::MemoryDelete,
            payload,
            trace_id: None,
            span_id: None,
            model: None,
            tokens_input: None,
            tokens_output: None,
            latency_ms: None,
            cost_usd: None,
            timestamp: (Utc::now() + Duration::seconds(offset_secs)).to_rfc3339(),
            logical_clock: offset_secs,
            content_hash: vec![],
            prev_hash: None,
            embedding: None,
        }
    }

    fn read_event(offset_secs: i64, agent: &str, thread: Option<&str>) -> AgentEvent {
        AgentEvent {
            id: Uuid::now_v7(),
            agent_id: agent.to_string(),
            thread_id: thread.map(String::from),
            run_id: None,
            parent_event_id: None,
            event_type: EventType::MemoryRead,
            payload: serde_json::json!({}),
            trace_id: None,
            span_id: None,
            model: None,
            tokens_input: None,
            tokens_output: None,
            latency_ms: None,
            cost_usd: None,
            timestamp: (Utc::now() + Duration::seconds(offset_secs)).to_rfc3339(),
            logical_clock: offset_secs,
            content_hash: vec![],
            prev_hash: None,
            embedding: None,
        }
    }

    #[test]
    fn empty_window_errors() {
        let events: Vec<AgentEvent> = vec![];
        let err = trajectory_audit(&events, &TrajectoryAuditRequest::default()).unwrap_err();
        assert!(matches!(err, ComplianceError::EmptyAuditWindow));
    }

    #[test]
    fn happy_path_all_signals_ok() {
        // One mature scope: writes, then forgets with named strategy.
        let mid = Uuid::now_v7();
        let events = vec![
            write(0, "a", mid, Some("f-1")),
            delete(1, "a", mid, Some("soft_delete")),
        ];
        let req = TrajectoryAuditRequest {
            active_bank_ceiling: 100,
            ..Default::default()
        };
        let r = trajectory_audit(&events, &req).unwrap();
        assert_eq!(r.event_count, 2);
        assert!(
            r.all_ok(),
            "all signals must be Ok on the happy path: {r:?}"
        );
    }

    #[test]
    fn signal_a_unregulated_growth_flags_breach() {
        let mut events = Vec::new();
        for i in 0..6 {
            events.push(write(i, "a", Uuid::now_v7(), None));
        }
        let req = TrajectoryAuditRequest {
            active_bank_ceiling: 3,
            ..Default::default()
        };
        let r = trajectory_audit(&events, &req).unwrap();
        assert_eq!(r.unregulated_growth.peak_active_size, 6);
        assert_eq!(
            r.unregulated_growth.breach_count, 3,
            "writes 4/5/6 exceed ceiling 3"
        );
        // Peak 6 <= 2 * ceiling 3, so Warn rather than Fail.
        assert_eq!(r.unregulated_growth.severity, Severity::Warn);
        // The other signals are unaffected.
        assert_eq!(
            r.capacity_driven_forgetting.severity,
            Severity::Ok,
            "no deletes — must be Ok"
        );
    }

    #[test]
    fn signal_a_unregulated_growth_fails_when_peak_doubles_ceiling() {
        let mut events = Vec::new();
        for i in 0..21 {
            events.push(write(i, "a", Uuid::now_v7(), None));
        }
        let req = TrajectoryAuditRequest {
            active_bank_ceiling: 10,
            ..Default::default()
        };
        let r = trajectory_audit(&events, &req).unwrap();
        assert_eq!(r.unregulated_growth.peak_active_size, 21);
        assert_eq!(r.unregulated_growth.severity, Severity::Fail);
    }

    #[test]
    fn signal_b_missing_semantic_revision_flags_supersession() {
        // Same fact_id written twice; the older write is never deleted.
        let old_mid = Uuid::now_v7();
        let new_mid = Uuid::now_v7();
        let events = vec![
            write(0, "a", old_mid, Some("fact-42")),
            write(10, "a", new_mid, Some("fact-42")),
        ];
        let req = TrajectoryAuditRequest::default();
        let r = trajectory_audit(&events, &req).unwrap();
        assert_eq!(r.missing_semantic_revision.stale_facts.len(), 1);
        let stale = &r.missing_semantic_revision.stale_facts[0];
        assert_eq!(stale.fact_id, "fact-42");
        assert_eq!(stale.stale_memory_ids, vec![old_mid]);
        assert_eq!(r.missing_semantic_revision.severity, Severity::Warn);
    }

    #[test]
    fn signal_b_clears_when_supersession_is_followed_by_delete() {
        let old_mid = Uuid::now_v7();
        let new_mid = Uuid::now_v7();
        let events = vec![
            write(0, "a", old_mid, Some("fact-42")),
            write(10, "a", new_mid, Some("fact-42")),
            delete(15, "a", old_mid, Some("soft_delete")),
        ];
        let req = TrajectoryAuditRequest::default();
        let r = trajectory_audit(&events, &req).unwrap();
        assert!(
            r.missing_semantic_revision.stale_facts.is_empty(),
            "older write was revised — no stale facts"
        );
        assert_eq!(r.missing_semantic_revision.severity, Severity::Ok);
    }

    #[test]
    fn signal_c_capacity_driven_flags_unlabelled_and_unknown_strategies() {
        let mid_a = Uuid::now_v7();
        let mid_b = Uuid::now_v7();
        let mid_c = Uuid::now_v7();
        let events = vec![
            write(0, "a", mid_a, None),
            write(1, "a", mid_b, None),
            write(2, "a", mid_c, None),
            delete(3, "a", mid_a, Some("soft_delete")), // named
            delete(4, "a", mid_b, Some("capacity_evict")), // unknown
            delete(5, "a", mid_c, None),                // unlabelled
        ];
        let req = TrajectoryAuditRequest::default();
        let r = trajectory_audit(&events, &req).unwrap();
        // Two unlabelled / out-of-policy deletes.
        assert_eq!(
            r.capacity_driven_forgetting
                .unlabelled_forget_event_ids
                .len(),
            2
        );
        assert_eq!(r.capacity_driven_forgetting.severity, Severity::Warn);
        assert_eq!(
            r.capacity_driven_forgetting
                .strategy_histogram
                .get("soft_delete")
                .copied(),
            Some(1)
        );
        assert_eq!(
            r.capacity_driven_forgetting
                .strategy_histogram
                .get("<unlabelled>")
                .copied(),
            Some(1)
        );
    }

    #[test]
    fn signal_d_read_only_scope_flagged() {
        // agent-r only reads; agent-w writes (so it must not be
        // flagged).
        let events = vec![
            read_event(0, "agent-r", None),
            read_event(1, "agent-r", None),
            write(2, "agent-w", Uuid::now_v7(), None),
        ];
        let req = TrajectoryAuditRequest::default();
        let r = trajectory_audit(&events, &req).unwrap();
        assert_eq!(r.read_only_retrieval.read_only_scopes, vec!["agent-r"]);
        assert_eq!(r.read_only_retrieval.severity, Severity::Warn);
    }

    #[test]
    fn agent_id_filter_narrows_the_scope() {
        let events = vec![
            write(0, "a", Uuid::now_v7(), None),
            write(1, "b", Uuid::now_v7(), None),
        ];
        let req = TrajectoryAuditRequest {
            agent_id: Some("b".to_string()),
            ..Default::default()
        };
        let r = trajectory_audit(&events, &req).unwrap();
        assert_eq!(r.event_count, 1);
        assert_eq!(r.scope_label, "agent=b");
    }
}
