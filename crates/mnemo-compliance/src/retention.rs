//! Processing-log **retention-conformance profiles**.
//!
//! Regulated deployments must keep their processing logs for a minimum period:
//! India's DPDP Rules 2025 require **personal data, traffic data and processing
//! logs** to be retained for **at least one year** (Seventh Schedule); the EU AI
//! Act requires deployers to keep automatically-generated logs for **at least
//! six months** (Art.19(1) / Art.26(6)); HIPAA requires documentation to be kept
//! for **six years** (45 CFR §164.316(b)(2), backing the §164.312(b) audit
//! controls).
//!
//! mnemo's `agent_events` log is **append-only by construction** — no code path
//! (FORGET, TTL sweep, decay/consolidation, cold-tier migration) deletes or
//! rewrites an event row; deletion targets *memory content* (`MemoryRecord`s)
//! and each deletion *appends* an event, so the log only grows. This is exactly
//! the DPDP distinction between **personal data** (erasable) and **traffic data
//! and logs** (retained). A [`RetentionProfile`] turns that architectural fact
//! into a **verifiable** conformance check.
//!
//! Like [`crate::audit`], this module is **pure**: it operates on exported
//! `&[AgentEvent]` snapshots and a backend-capability flag, returning
//! [`ComplianceError`] on failure — it never reaches into storage itself. A
//! harness ([`bench/retention_conformance`]) drives the real deletion paths and
//! feeds the before/after snapshots in.
//!
//! # Fail-loud, never silent
//!
//! If the active backend cannot guarantee an append-only log,
//! [`RetentionProfile::assert_backend_can_retain`] returns
//! [`ComplianceError::RetentionFloorUnsupported`] naming the backend — the same
//! posture as `mnemo_core::error::Error::EmbedderNotConfigured` (v0.5.13).

use mnemo_core::model::event::AgentEvent;
use serde::Serialize;

use crate::error::ComplianceError;

const SECONDS_PER_DAY: i64 = 86_400;

/// A named processing-log retention-conformance profile: an obligation, the
/// minimum retention floor it demands, its commencement date, and the primary
/// source. The floor is **configurable** via [`RetentionProfile::with_floor_days`].
///
/// Constructors ship the vetted defaults; nothing here is a legal
/// determination — the profile checks the *mechanism* (append-only retention),
/// not compliance.
#[derive(Debug, Clone, Serialize)]
pub struct RetentionProfile {
    /// Stable machine name, e.g. `"dpdp-rules"`.
    pub name: &'static str,
    /// One-line obligation the floor maps to.
    pub obligation: &'static str,
    /// Minimum retention floor, in days.
    pub floor_days: u32,
    /// Commencement / applicability date (ISO-8601), for operator planning.
    pub commencement: &'static str,
    /// Primary-source URL.
    pub source_url: &'static str,
}

impl RetentionProfile {
    /// India **DPDP Rules 2025** — personal data, traffic data and processing
    /// logs retained **≥ 1 year** (Seventh Schedule). Data-fiduciary
    /// obligations commence 2027-05-13 (Gazette G.S.R. 846(E), 2025-11-13;
    /// 18-month transition).
    pub const fn dpdp_rules() -> Self {
        Self {
            name: "dpdp-rules",
            obligation: "India DPDP Rules 2025 — retain personal data, traffic data and processing logs (Seventh Schedule)",
            floor_days: 365,
            commencement: "2027-05-13",
            source_url: "https://www.meity.gov.in/documents/act-and-policies/digital-personal-data-protection-rules-2025-gDOxUjMtQWa",
        }
    }

    /// **EU AI Act Art.19 / Art.26(6)** — deployers keep automatically-generated
    /// logs for **≥ 6 months**. High-risk obligations apply 2027-12-02
    /// (stand-alone Annex III) / 2028-08-02 (Annex I embedded) per the Digital
    /// Omnibus (Council final green light 2026-06-29).
    pub const fn eu_ai_act_art19() -> Self {
        Self {
            name: "eu-ai-act-art19",
            obligation: "EU AI Act Art.19/26(6) — keep automatically-generated logs for at least six months",
            floor_days: 180,
            commencement: "2027-12-02",
            source_url: "https://eur-lex.europa.eu/eli/reg/2024/1689/oj",
        }
    }

    /// **HIPAA §164.312(b) audit controls**, retained per §164.316(b)(2) for
    /// **six years** from creation or last-effective date.
    pub const fn hipaa_164_312b() -> Self {
        Self {
            name: "hipaa-164.312b",
            obligation: "HIPAA 45 CFR §164.312(b) audit controls — documentation retained six years (§164.316(b)(2))",
            floor_days: 2190,
            commencement: "in-force",
            source_url: "https://www.ecfr.gov/current/title-45/subtitle-A/subchapter-C/part-164/subpart-C/section-164.312",
        }
    }

    /// Override the retention floor (days). The obligation's default is the
    /// legal minimum; an operator may set a *longer* floor for its own policy.
    #[must_use]
    pub fn with_floor_days(mut self, days: u32) -> Self {
        self.floor_days = days;
        self
    }

    /// The floor as a number of seconds.
    pub fn floor_seconds(&self) -> i64 {
        i64::from(self.floor_days) * SECONDS_PER_DAY
    }

    /// **Fail loud** if the active storage backend cannot honour an append-only
    /// retention floor. `events_are_append_only` comes from
    /// `mnemo_core::storage::StorageBackend::events_are_append_only()`;
    /// `backend` from `StorageBackend::backend_name()`. Both shipped backends
    /// (DuckDB, PostgreSQL) return `true`; a backend that permits event deletion
    /// yields [`ComplianceError::RetentionFloorUnsupported`] naming itself.
    pub fn assert_backend_can_retain(
        &self,
        backend: &str,
        events_are_append_only: bool,
    ) -> Result<(), ComplianceError> {
        if events_are_append_only {
            Ok(())
        } else {
            Err(ComplianceError::RetentionFloorUnsupported {
                backend: backend.to_string(),
                floor_days: self.floor_days,
            })
        }
    }

    /// Verify that a deletion/compaction/migration `path` did **not** drop or
    /// rewrite any `agent_events` row inside the retention floor. Compares an
    /// event snapshot taken *before* the path ran against one taken *after*:
    ///
    /// - **append-only** — every event present before is still present after
    ///   (by id), and the count did not shrink;
    /// - **retention floor** — no event younger than the floor is missing;
    /// - **immutability** — retained events are byte-identical (`content_hash`,
    ///   `prev_hash`, and traffic metadata unchanged).
    ///
    /// `now_rfc3339` is the reference "now" for age computation. Returns a
    /// [`RetentionFinding`] (never panics); a malformed timestamp surfaces as
    /// [`ComplianceError::UnparseableTimestamp`].
    pub fn verify_path(
        &self,
        path: &str,
        before: &[AgentEvent],
        after: &[AgentEvent],
        now_rfc3339: &str,
    ) -> Result<RetentionFinding, ComplianceError> {
        let now = parse_rfc3339(now_rfc3339, "now")?;
        let after_by_id: std::collections::HashMap<uuid::Uuid, &AgentEvent> =
            after.iter().map(|e| (e.id, e)).collect();

        let mut dropped_in_floor = 0usize;
        let mut dropped_total = 0usize;
        let mut rewritten = 0usize;

        for ev in before {
            let ts = parse_rfc3339(&ev.timestamp, &ev.id.to_string())?;
            let age_secs = (now - ts).num_seconds();
            match after_by_id.get(&ev.id) {
                None => {
                    dropped_total += 1;
                    if age_secs < self.floor_seconds() {
                        dropped_in_floor += 1;
                    }
                }
                Some(post) => {
                    // Immutability: the retained row must not have been rewritten.
                    if post.content_hash != ev.content_hash
                        || post.prev_hash != ev.prev_hash
                        || !traffic_metadata_equal(post, ev)
                    {
                        rewritten += 1;
                    }
                }
            }
        }

        let grew = after.len() >= before.len();
        let pass = dropped_in_floor == 0 && dropped_total == 0 && rewritten == 0 && grew;

        let detail = format!(
            "{} events before, {} after (Δ{:+}); {} dropped ({} within {}-day floor), {} rewritten",
            before.len(),
            after.len(),
            after.len() as i64 - before.len() as i64,
            dropped_total,
            dropped_in_floor,
            self.floor_days,
            rewritten,
        );

        Ok(RetentionFinding {
            path: path.to_string(),
            pass,
            detail,
        })
    }

    /// Verify that **traffic / processing metadata** (not just memory content)
    /// is retained — DPDP names "personal data, traffic data and logs"
    /// separately. Every event in `before` that carried traffic metadata
    /// (`model`, `tokens_input`/`tokens_output`, `latency_ms`, `cost_usd`, or
    /// `trace_id`) must still be present in `after` with those fields
    /// byte-identical. Fails if any traffic-bearing event was dropped or its
    /// metadata rewritten, or if `before` carried no traffic metadata at all
    /// (nothing to prove — the harness must seed it).
    pub fn verify_traffic_metadata_retained(
        &self,
        before: &[AgentEvent],
        after: &[AgentEvent],
    ) -> RetentionFinding {
        let after_by_id: std::collections::HashMap<uuid::Uuid, &AgentEvent> =
            after.iter().map(|e| (e.id, e)).collect();

        let mut traffic_bearing = 0usize;
        let mut retained = 0usize;
        for ev in before.iter().filter(|e| has_traffic_metadata(e)) {
            traffic_bearing += 1;
            if let Some(post) = after_by_id.get(&ev.id)
                && traffic_metadata_equal(post, ev)
            {
                retained += 1;
            }
        }

        let pass = traffic_bearing > 0 && retained == traffic_bearing;
        let detail = if traffic_bearing == 0 {
            "no traffic/processing-metadata events present to verify".to_string()
        } else {
            format!(
                "{retained}/{traffic_bearing} traffic-metadata events retained with fields intact"
            )
        };

        RetentionFinding {
            path: "traffic_metadata_retained".to_string(),
            pass,
            detail,
        }
    }
}

/// One retention check and its outcome.
#[derive(Debug, Clone, Serialize)]
pub struct RetentionFinding {
    /// The deletion/compaction path (or check) exercised, e.g.
    /// `"forget_hard_delete"`.
    pub path: String,
    pub pass: bool,
    pub detail: String,
}

/// Aggregate conformance report — the machine-readable artifact an auditor reads.
#[derive(Debug, Clone, Serialize)]
pub struct RetentionReport {
    pub profile: &'static str,
    pub obligation: &'static str,
    pub floor_days: u32,
    pub commencement: &'static str,
    pub source_url: &'static str,
    pub backend: String,
    pub findings: Vec<RetentionFinding>,
    /// True iff every finding passed.
    pub conformant: bool,
}

impl RetentionReport {
    /// Build a report from a profile, the backend name, and the collected
    /// findings. `conformant` is the AND of every finding.
    pub fn new(profile: &RetentionProfile, backend: &str, findings: Vec<RetentionFinding>) -> Self {
        let conformant = findings.iter().all(|f| f.pass);
        Self {
            profile: profile.name,
            obligation: profile.obligation,
            floor_days: profile.floor_days,
            commencement: profile.commencement,
            source_url: profile.source_url,
            backend: backend.to_string(),
            findings,
            conformant,
        }
    }
}

fn has_traffic_metadata(e: &AgentEvent) -> bool {
    e.model.is_some()
        || e.tokens_input.is_some()
        || e.tokens_output.is_some()
        || e.latency_ms.is_some()
        || e.cost_usd.is_some()
        || e.trace_id.is_some()
        || e.span_id.is_some()
}

fn traffic_metadata_equal(a: &AgentEvent, b: &AgentEvent) -> bool {
    a.model == b.model
        && a.tokens_input == b.tokens_input
        && a.tokens_output == b.tokens_output
        && a.latency_ms == b.latency_ms
        && a.cost_usd == b.cost_usd
        && a.trace_id == b.trace_id
        && a.span_id == b.span_id
}

fn parse_rfc3339(
    ts: &str,
    context_id: &str,
) -> Result<chrono::DateTime<chrono::Utc>, ComplianceError> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| ComplianceError::UnparseableTimestamp {
            event_id: context_id.to_string(),
            timestamp: ts.to_string(),
            reason: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnemo_core::model::event::EventType;

    fn event_at(id_seed: u128, ts: &str, with_traffic: bool) -> AgentEvent {
        AgentEvent {
            id: uuid::Uuid::from_u128(id_seed),
            agent_id: "retention-agent".to_string(),
            thread_id: None,
            run_id: None,
            parent_event_id: None,
            event_type: EventType::MemoryWrite,
            payload: serde_json::json!({ "seq": id_seed }),
            trace_id: with_traffic.then(|| "trace-abc".to_string()),
            span_id: None,
            model: with_traffic.then(|| "claude-opus-4-8".to_string()),
            tokens_input: with_traffic.then_some(100),
            tokens_output: with_traffic.then_some(200),
            latency_ms: with_traffic.then_some(42),
            cost_usd: with_traffic.then_some(0.01),
            timestamp: ts.to_string(),
            logical_clock: id_seed as i64,
            content_hash: vec![id_seed as u8; 32],
            prev_hash: None,
            embedding: None,
        }
    }

    #[test]
    fn defaults_match_the_obligations() {
        assert_eq!(RetentionProfile::dpdp_rules().floor_days, 365);
        assert_eq!(RetentionProfile::eu_ai_act_art19().floor_days, 180);
        assert_eq!(RetentionProfile::hipaa_164_312b().floor_days, 2190);
    }

    #[test]
    fn floor_is_configurable() {
        let p = RetentionProfile::dpdp_rules().with_floor_days(730);
        assert_eq!(p.floor_days, 730);
        assert_eq!(p.floor_seconds(), 730 * 86_400);
    }

    #[test]
    fn append_only_log_conforms() {
        let p = RetentionProfile::dpdp_rules();
        let now = "2026-07-19T00:00:00Z";
        let before = vec![
            event_at(1, "2026-07-01T00:00:00Z", true),
            event_at(2, "2026-07-02T00:00:00Z", false),
        ];
        // A deletion path appended a MemoryDelete event; nothing was removed.
        let mut after = before.clone();
        after.push(event_at(3, "2026-07-19T00:00:00Z", false));

        let finding = p
            .verify_path("forget_hard_delete", &before, &after, now)
            .unwrap();
        assert!(
            finding.pass,
            "append-only path must conform: {}",
            finding.detail
        );
    }

    #[test]
    fn dropping_an_in_floor_event_fails() {
        let p = RetentionProfile::dpdp_rules();
        let now = "2026-07-19T00:00:00Z";
        let before = vec![
            event_at(1, "2026-07-01T00:00:00Z", false), // 18 days old < 365-day floor
            event_at(2, "2026-07-02T00:00:00Z", false),
        ];
        // Simulate a (hypothetical) path that dropped event #1.
        let after = vec![before[1].clone()];

        let finding = p
            .verify_path("hypothetical_purge", &before, &after, now)
            .unwrap();
        assert!(!finding.pass, "dropping an in-floor event must fail");
        assert!(finding.detail.contains("1 within 365-day floor"));
    }

    #[test]
    fn rewriting_a_retained_event_fails() {
        let p = RetentionProfile::eu_ai_act_art19();
        let now = "2026-07-19T00:00:00Z";
        let before = vec![event_at(1, "2026-07-01T00:00:00Z", true)];
        let mut after = before.clone();
        after[0].content_hash = vec![0xFF; 32]; // tampered
        let finding = p.verify_path("rewrite", &before, &after, now).unwrap();
        assert!(!finding.pass, "rewriting a retained event must fail");
        assert!(finding.detail.contains("1 rewritten"));
    }

    #[test]
    fn traffic_metadata_retention_is_checked() {
        let p = RetentionProfile::dpdp_rules();
        let before = vec![
            event_at(1, "2026-07-01T00:00:00Z", true),
            event_at(2, "2026-07-02T00:00:00Z", false),
        ];
        let after = before.clone();
        let ok = p.verify_traffic_metadata_retained(&before, &after);
        assert!(ok.pass, "{}", ok.detail);

        // Drop the traffic-bearing event.
        let after_dropped = vec![before[1].clone()];
        let bad = p.verify_traffic_metadata_retained(&before, &after_dropped);
        assert!(!bad.pass, "dropping traffic metadata must fail");
    }

    #[test]
    fn backend_gate_fails_loud_when_not_append_only() {
        let p = RetentionProfile::dpdp_rules();
        assert!(p.assert_backend_can_retain("duckdb", true).is_ok());
        match p.assert_backend_can_retain("ephemeral-cache", false) {
            Err(ComplianceError::RetentionFloorUnsupported {
                backend,
                floor_days,
            }) => {
                assert_eq!(backend, "ephemeral-cache");
                assert_eq!(floor_days, 365);
            }
            other => panic!("expected RetentionFloorUnsupported, got {other:?}"),
        }
    }

    #[test]
    fn report_aggregates_conformance() {
        let p = RetentionProfile::dpdp_rules();
        let findings = vec![
            RetentionFinding {
                path: "a".into(),
                pass: true,
                detail: "ok".into(),
            },
            RetentionFinding {
                path: "b".into(),
                pass: true,
                detail: "ok".into(),
            },
        ];
        let report = RetentionReport::new(&p, "duckdb", findings);
        assert!(report.conformant);
        assert_eq!(report.floor_days, 365);
        assert_eq!(report.backend, "duckdb");
    }
}
