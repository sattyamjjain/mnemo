use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::model::agent_profile::AgentProfile;
use crate::model::memory::{MemoryRecord, SourceType};
use crate::query::MnemoEngine;
use crate::storage::MemoryFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyCheckResult {
    pub is_anomalous: bool,
    pub score: f32,
    pub reasons: Vec<String>,
}

/// One row returned by [`replay_quarantine`].
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineReplayEntry {
    pub id: Uuid,
    pub agent_id: String,
    pub content: String,
    pub reason: String,
    pub created_at: String,
    pub source_type: SourceType,
    pub tags: Vec<String>,
}

/// Indirect-injection markers that commonly appear in MINJA-style attacks
/// when an LLM is tricked into writing a memory it shouldn't. When present
/// in records NOT written by a tool call (i.e. injected via a retrieved
/// web/document fragment) they are a strong signal of injection.
const SELF_REFERENTIAL_INSTRUCTION_MARKERS: &[&str] = &[
    "remember this",
    "remember the following",
    "in the future, always",
    "from now on, you",
    "from now on, always",
    "as the system, i",
    "as your system prompt",
    "permanently remember",
    "never forget that",
    "always respond with",
    "always answer by",
    "whenever asked about",
    "when you are asked",
];

/// Sources we trust to carry instruction-like content by design.
fn is_trusted_source(st: SourceType) -> bool {
    matches!(
        st,
        SourceType::ToolOutput
            | SourceType::System
            | SourceType::UserInput
            | SourceType::Human
            | SourceType::ModelResponse
    )
}

/// `source:<label>` tag marking a record as coming from an indirect
/// injection vector (a web page, a document, a third-party email).
fn looks_like_indirect_ingest(record: &MemoryRecord) -> bool {
    record.tags.iter().any(|t| {
        let lower = t.to_lowercase();
        lower == "source:web"
            || lower == "source:document"
            || lower == "source:email"
            || lower == "source:third_party"
            || lower == "source:retrieved"
    }) || matches!(record.source_type, SourceType::Retrieval | SourceType::Import)
}

/// Self-referential instruction marker check — MINJA-class indirect
/// injection signal. Returns a (matched_marker, is_suspicious_source) pair.
fn check_self_referential_injection(record: &MemoryRecord) -> Option<&'static str> {
    let lower = record.content.to_lowercase();
    let matched = SELF_REFERENTIAL_INSTRUCTION_MARKERS
        .iter()
        .find(|p| lower.contains(**p))
        .copied()?;
    // Self-referential phrasing from a trusted source (tool output, user
    // input, human) is legitimate; only flag it when the record arrived
    // via an indirect ingest path.
    if is_trusted_source(record.source_type) && !looks_like_indirect_ingest(record) {
        return None;
    }
    Some(matched)
}

/// Detect common prompt injection patterns in memory content.
///
/// These patterns attempt to override AI agent instructions when the
/// memory is recalled and included in an LLM context.
fn contains_prompt_injection_patterns(content: &str) -> bool {
    let lower = content.to_lowercase();
    let patterns = [
        "ignore all previous instructions",
        "ignore previous instructions",
        "disregard all prior",
        "disregard previous",
        "override system prompt",
        "you are now in",
        "new instructions:",
        "system: you are",
        "<<sys>>",
        "[system]",
        "```system",
    ];
    patterns.iter().any(|p| lower.contains(p))
}

/// Check a newly inserted memory record for anomaly indicators.
///
/// Scoring:
/// - Importance deviation >0.4 from agent mean → +0.3
/// - Content length >5x or <0.1x agent average → +0.3
/// - High-frequency burst (>3x normal rate in last minute) → +0.4
/// - Prompt injection patterns in content → +0.5
/// - Total score >= 0.5 → anomalous
pub async fn check_for_anomaly(
    engine: &MnemoEngine,
    record: &MemoryRecord,
) -> Result<AnomalyCheckResult> {
    let profile = engine.storage.get_agent_profile(&record.agent_id).await?;

    let mut score: f32 = 0.0;
    let mut reasons = Vec::new();

    if let Some(ref profile) = profile {
        // Check importance outlier
        let importance_deviation = (record.importance as f64 - profile.avg_importance).abs();
        if importance_deviation > 0.4 {
            score += 0.3;
            reasons.push(format!(
                "importance {:.2} deviates {:.2} from agent average {:.2}",
                record.importance, importance_deviation, profile.avg_importance
            ));
        }

        // Check content length outlier
        let content_len = record.content.len() as f64;
        if profile.avg_content_length > 0.0 {
            let ratio = content_len / profile.avg_content_length;
            if !(0.1..=5.0).contains(&ratio) {
                score += 0.3;
                reasons.push(format!(
                    "content length {} is {:.1}x agent average {:.0}",
                    record.content.len(),
                    ratio,
                    profile.avg_content_length
                ));
            }
        }

        // Check high-frequency burst: compare recent write count to expected rate
        // If agent has N memories over their lifetime, average rate = N / hours_active
        // A burst is >3x that rate in the last minute
        // Simplified: if total_memories > 10 and a new memory comes in very quickly
        // We approximate by checking if total_memories suggests rapid growth
        if profile.total_memories > 10 {
            // Parse last_updated to get time window
            if let Ok(last_updated) = chrono::DateTime::parse_from_rfc3339(&profile.last_updated)
                && let Ok(created) = chrono::DateTime::parse_from_rfc3339(&record.created_at)
            {
                let seconds_since_update = (created - last_updated).num_seconds().max(1);
                // If profile was updated less than 1 second ago, it's a burst
                if seconds_since_update < 1 {
                    score += 0.4;
                    reasons.push("high-frequency burst detected".to_string());
                }
            }
        }
    }
    // If no profile exists yet, we can't detect anomalies — treat as normal

    // Check for prompt injection patterns in content
    if contains_prompt_injection_patterns(&record.content) {
        score += 0.5;
        reasons.push("content contains prompt injection patterns".to_string());
    }

    // MINJA-class: self-referential instruction phrasing in a record that
    // arrived through an indirect-ingest path (retrieved doc, web page,
    // tagged `source:*`). Strong signal — see arXiv:2503.03704.
    if let Some(marker) = check_self_referential_injection(record) {
        score += 0.6;
        reasons.push(format!(
            "self-referential injection marker '{marker}' in indirectly-ingested record"
        ));
    }

    Ok(AnomalyCheckResult {
        is_anomalous: score >= 0.5,
        score,
        reasons,
    })
}

/// List every quarantined memory for `agent_id` with `created_at >= since`.
/// Returns them in chronological order so operators can walk a review
/// queue deterministically.
pub async fn replay_quarantine(
    engine: &MnemoEngine,
    agent_id: &str,
    since: Option<&str>,
) -> Result<Vec<QuarantineReplayEntry>> {
    let filter = MemoryFilter {
        agent_id: Some(agent_id.to_string()),
        // Quarantined records may be soft-deleted if an operator later
        // hard-purged them via `forget_subject`; we still want visibility.
        include_deleted: true,
        ..Default::default()
    };
    let records = engine
        .storage
        .list_memories(&filter, super::MAX_BATCH_QUERY_LIMIT, 0)
        .await?;
    let mut out: Vec<QuarantineReplayEntry> = records
        .into_iter()
        .filter(|r| r.quarantined)
        .filter(|r| match since {
            None => true,
            Some(cutoff) => r.created_at.as_str() >= cutoff,
        })
        .map(|r| QuarantineReplayEntry {
            id: r.id,
            agent_id: r.agent_id,
            content: r.content,
            reason: r
                .quarantine_reason
                .unwrap_or_else(|| "unspecified".to_string()),
            created_at: r.created_at,
            source_type: r.source_type,
            tags: r.tags,
        })
        .collect();
    out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(out)
}

/// Mark a memory as quarantined with a reason.
pub async fn quarantine_memory(engine: &MnemoEngine, id: Uuid, reason: &str) -> Result<()> {
    if let Some(mut record) = engine.storage.get_memory(id).await? {
        record.quarantined = true;
        record.quarantine_reason = Some(reason.to_string());
        record.updated_at = chrono::Utc::now().to_rfc3339();
        engine.storage.update_memory(&record).await?;
    }
    Ok(())
}

/// Update the agent profile with statistics from the new memory.
pub async fn update_agent_profile(engine: &MnemoEngine, record: &MemoryRecord) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let existing = engine.storage.get_agent_profile(&record.agent_id).await?;

    let profile = match existing {
        Some(mut p) => {
            // Incremental mean update
            let n = p.total_memories as f64;
            p.avg_importance = (p.avg_importance * n + record.importance as f64) / (n + 1.0);
            p.avg_content_length =
                (p.avg_content_length * n + record.content.len() as f64) / (n + 1.0);
            p.total_memories += 1;
            p.last_updated = now;
            p
        }
        None => AgentProfile {
            agent_id: record.agent_id.clone(),
            avg_importance: record.importance as f64,
            avg_content_length: record.content.len() as f64,
            total_memories: 1,
            last_updated: now,
        },
    };

    engine
        .storage
        .insert_or_update_agent_profile(&profile)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anomaly_result_default() {
        let result = AnomalyCheckResult {
            is_anomalous: false,
            score: 0.0,
            reasons: vec![],
        };
        assert!(!result.is_anomalous);
        assert_eq!(result.score, 0.0);
    }
}
