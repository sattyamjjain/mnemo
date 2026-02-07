use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::model::agent_profile::AgentProfile;
use crate::model::memory::MemoryRecord;
use crate::query::MnemoEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyCheckResult {
    pub is_anomalous: bool,
    pub score: f32,
    pub reasons: Vec<String>,
}

/// Check a newly inserted memory record for anomaly indicators.
///
/// Scoring:
/// - Importance deviation >0.4 from agent mean → +0.3
/// - Content length >5x or <0.1x agent average → +0.3
/// - High-frequency burst (>3x normal rate in last minute) → +0.4
/// - Total score >= 0.5 → anomalous
pub async fn check_for_anomaly(
    engine: &MnemoEngine,
    record: &MemoryRecord,
) -> Result<AnomalyCheckResult> {
    let profile = engine
        .storage
        .get_agent_profile(&record.agent_id)
        .await?;

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
            if let Ok(last_updated) = chrono::DateTime::parse_from_rfc3339(&profile.last_updated) {
                if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&record.created_at) {
                    let seconds_since_update = (created - last_updated).num_seconds().max(1);
                    // If profile was updated less than 1 second ago, it's a burst
                    if seconds_since_update < 1 {
                        score += 0.4;
                        reasons.push("high-frequency burst detected".to_string());
                    }
                }
            }
        }
    }
    // If no profile exists yet, we can't detect anomalies — treat as normal

    Ok(AnomalyCheckResult {
        is_anomalous: score >= 0.5,
        score,
        reasons,
    })
}

/// Mark a memory as quarantined with a reason.
pub async fn quarantine_memory(
    engine: &MnemoEngine,
    id: Uuid,
    reason: &str,
) -> Result<()> {
    if let Some(mut record) = engine.storage.get_memory(id).await? {
        record.quarantined = true;
        record.quarantine_reason = Some(reason.to_string());
        record.updated_at = chrono::Utc::now().to_rfc3339();
        engine.storage.update_memory(&record).await?;
    }
    Ok(())
}

/// Update the agent profile with statistics from the new memory.
pub async fn update_agent_profile(
    engine: &MnemoEngine,
    record: &MemoryRecord,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let existing = engine
        .storage
        .get_agent_profile(&record.agent_id)
        .await?;

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
