use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::model::memory::{ConsolidationState, MemoryRecord, MemoryType, SourceType};
use crate::model::relation::Relation;
use crate::query::MnemoEngine;
use crate::storage::MemoryFilter;

/// Custom decay function types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DecayFunction {
    /// Exponential: base * e^(-rate * hours)  (default, Ebbinghaus-inspired)
    Exponential,
    /// Linear: base * max(0, 1 - rate * hours)
    Linear,
    /// Step function: base importance until threshold hours, then 0
    StepFunction(f32),
    /// Power law: base / (1 + rate * hours)^alpha
    PowerLaw(f32),
}

impl DecayFunction {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "exponential" => Some(DecayFunction::Exponential),
            "linear" => Some(DecayFunction::Linear),
            s if s.starts_with("step:") => {
                s[5..].parse::<f32>().ok().map(DecayFunction::StepFunction)
            }
            s if s.starts_with("power_law:") => {
                s[10..].parse::<f32>().ok().map(DecayFunction::PowerLaw)
            }
            _ => None,
        }
    }
}

/// Compute effective importance using the specified or default decay curve.
/// Default (Exponential): `base_importance * e^(-decay_rate * hours) + 0.05 * ln(1 + access_count)`
pub fn effective_importance(record: &MemoryRecord) -> f32 {
    let decay_fn = record.decay_function.as_deref()
        .and_then(DecayFunction::from_str_opt)
        .unwrap_or(DecayFunction::Exponential);
    effective_importance_with(record, &decay_fn)
}

pub fn effective_importance_with(record: &MemoryRecord, decay_fn: &DecayFunction) -> f32 {
    let decay_rate = record.decay_rate.unwrap_or(0.01);
    let hours = hours_since_creation(&record.created_at);
    let access_boost = 0.05 * (1.0 + record.access_count as f32).ln();

    let base = match decay_fn {
        DecayFunction::Exponential => {
            record.importance * (-decay_rate * hours).exp()
        }
        DecayFunction::Linear => {
            record.importance * (1.0 - decay_rate * hours).max(0.0)
        }
        DecayFunction::StepFunction(threshold_hours) => {
            if hours < *threshold_hours {
                record.importance
            } else {
                0.0
            }
        }
        DecayFunction::PowerLaw(alpha) => {
            record.importance / (1.0 + decay_rate * hours).powf(*alpha)
        }
    };

    (base + access_boost).min(1.0)
}

fn hours_since_creation(created_at: &str) -> f32 {
    let now = chrono::Utc::now();
    match chrono::DateTime::parse_from_rfc3339(created_at) {
        Ok(dt) => {
            let age = now - dt.with_timezone(&chrono::Utc);
            (age.num_seconds() as f32 / 3600.0).max(0.0)
        }
        Err(_) => 0.0,
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayPassResult {
    pub archived: usize,
    pub forgotten: usize,
    pub total_processed: usize,
}

impl DecayPassResult {
    pub fn new(archived: usize, forgotten: usize, total_processed: usize) -> Self {
        Self {
            archived,
            forgotten,
            total_processed,
        }
    }
}

/// Run a decay pass over all active memories for the given agent.
/// Memories below `forget_threshold` are marked Forgotten.
/// Memories below `archive_threshold` (but above forget) are marked Archived.
pub async fn run_decay_pass(
    engine: &MnemoEngine,
    agent_id: &str,
    archive_threshold: f32,
    forget_threshold: f32,
) -> Result<DecayPassResult> {
    let filter = MemoryFilter {
        agent_id: Some(agent_id.to_string()),
        include_deleted: false,
        ..Default::default()
    };
    let memories = engine.storage.list_memories(&filter, super::MAX_BATCH_QUERY_LIMIT, 0).await?;

    let mut archived = 0;
    let mut forgotten = 0;
    let total_processed = memories.len();

    for mut record in memories {
        if record.consolidation_state == ConsolidationState::Forgotten
            || record.consolidation_state == ConsolidationState::Archived
        {
            continue;
        }

        let eff = effective_importance(&record);

        if eff < forget_threshold {
            record.consolidation_state = ConsolidationState::Forgotten;
            record.updated_at = chrono::Utc::now().to_rfc3339();
            engine.storage.update_memory(&record).await?;
            forgotten += 1;
        } else if eff < archive_threshold {
            record.consolidation_state = ConsolidationState::Archived;
            record.updated_at = chrono::Utc::now().to_rfc3339();
            engine.storage.update_memory(&record).await?;
            archived += 1;
        }
    }

    Ok(DecayPassResult {
        archived,
        forgotten,
        total_processed,
    })
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationResult {
    pub clusters_found: usize,
    pub new_memories_created: usize,
    pub originals_consolidated: usize,
}

impl ConsolidationResult {
    pub fn new(
        clusters_found: usize,
        new_memories_created: usize,
        originals_consolidated: usize,
    ) -> Self {
        Self {
            clusters_found,
            new_memories_created,
            originals_consolidated,
        }
    }
}

/// Consolidate episodic memories into semantic summaries.
/// Clusters by tag overlap and creates consolidated semantic memories.
pub async fn run_consolidation(
    engine: &MnemoEngine,
    agent_id: &str,
    min_cluster_size: usize,
) -> Result<ConsolidationResult> {
    let filter = MemoryFilter {
        agent_id: Some(agent_id.to_string()),
        memory_type: Some(MemoryType::Episodic),
        include_deleted: false,
        ..Default::default()
    };
    let memories = engine.storage.list_memories(&filter, super::MAX_BATCH_QUERY_LIMIT, 0).await?;

    // Only consider memories that are Raw or Active
    let active: Vec<MemoryRecord> = memories
        .into_iter()
        .filter(|m| {
            m.consolidation_state == ConsolidationState::Raw
                || m.consolidation_state == ConsolidationState::Active
        })
        .collect();

    // Cluster by tag overlap: group memories sharing at least one tag
    let mut clusters: Vec<Vec<&MemoryRecord>> = Vec::new();

    for record in &active {
        let mut found_cluster = false;
        for cluster in &mut clusters {
            // Check if this record shares any tag with any record in cluster
            if cluster.iter().any(|c| {
                c.tags.iter().any(|t| record.tags.contains(t))
            }) {
                cluster.push(record);
                found_cluster = true;
                break;
            }
        }
        if !found_cluster {
            clusters.push(vec![record]);
        }
    }

    let mut clusters_found = 0;
    let mut new_memories_created = 0;
    let mut originals_consolidated = 0;

    for cluster in &clusters {
        if cluster.len() < min_cluster_size {
            continue;
        }
        clusters_found += 1;

        // Create a consolidated semantic memory
        let combined_content: Vec<String> = cluster.iter().map(|m| m.content.clone()).collect();
        let content = format!("[Consolidated from {} memories] {}", cluster.len(), combined_content.join(" | "));
        let avg_importance = cluster.iter().map(|m| m.importance).sum::<f32>() / cluster.len() as f32;
        let all_tags: Vec<String> = cluster
            .iter()
            .flat_map(|m| m.tags.iter().cloned())
            .collect::<std::collections::HashSet<String>>()
            .into_iter()
            .collect();

        let now = chrono::Utc::now().to_rfc3339();
        let new_id = Uuid::now_v7();
        let content_hash = crate::hash::compute_content_hash(&content, agent_id, &now);

        let embedding = engine.embedding.embed(&content).await?;

        let prev_hash_raw = engine
            .storage
            .get_latest_memory_hash(agent_id, None)
            .await
            .ok()
            .flatten();
        let prev_hash = Some(crate::hash::compute_chain_hash(&content_hash, prev_hash_raw.as_deref()));

        let new_record = MemoryRecord {
            id: new_id,
            agent_id: agent_id.to_string(),
            content,
            memory_type: MemoryType::Semantic,
            scope: cluster[0].scope,
            importance: avg_importance,
            tags: all_tags,
            metadata: serde_json::json!({"consolidated_from": cluster.iter().map(|m| m.id.to_string()).collect::<Vec<_>>()}),
            embedding: Some(embedding.clone()),
            content_hash: content_hash.clone(),
            prev_hash,
            source_type: SourceType::Consolidation,
            source_id: None,
            consolidation_state: ConsolidationState::Active,
            access_count: 0,
            org_id: cluster[0].org_id.clone(),
            thread_id: None,
            created_at: now.clone(),
            updated_at: now,
            last_accessed_at: None,
            expires_at: None,
            deleted_at: None,
            decay_rate: None,
            created_by: Some("consolidation_engine".to_string()),
            version: 1,
            prev_version_id: None,
            quarantined: false,
            quarantine_reason: None,
            decay_function: None,
        };

        engine.storage.insert_memory(&new_record).await?;
        engine.index.add(new_id, &embedding)?;
        if let Some(ref ft) = engine.full_text {
            ft.add(new_id, &new_record.content)?;
            ft.commit()?;
        }
        new_memories_created += 1;

        // Create relations and mark originals as consolidated
        for original in cluster {
            let relation = Relation {
                id: Uuid::now_v7(),
                source_id: new_id,
                target_id: original.id,
                relation_type: "consolidated_from".to_string(),
                weight: 1.0,
                metadata: serde_json::Value::Object(serde_json::Map::new()),
                created_at: new_record.created_at.clone(),
            };
            if let Err(e) = engine.storage.insert_relation(&relation).await {
                tracing::error!(relation_id = %relation.id, error = %e, "failed to insert consolidation relation");
            }

            let mut updated = (*original).clone();
            updated.consolidation_state = ConsolidationState::Consolidated;
            updated.updated_at = chrono::Utc::now().to_rfc3339();
            if let Err(e) = engine.storage.update_memory(&updated).await {
                tracing::error!(memory_id = %updated.id, error = %e, "failed to update consolidation state");
            }
            originals_consolidated += 1;
        }
    }

    Ok(ConsolidationResult {
        clusters_found,
        new_memories_created,
        originals_consolidated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::memory::*;

    #[test]
    fn test_effective_importance_decay() {
        // Fresh memory with high importance
        let now = chrono::Utc::now().to_rfc3339();
        let record = MemoryRecord {
            id: Uuid::now_v7(),
            agent_id: "agent-1".to_string(),
            content: "test".to_string(),
            memory_type: MemoryType::Episodic,
            scope: Scope::Private,
            importance: 0.8,
            tags: vec![],
            metadata: serde_json::json!({}),
            embedding: None,
            content_hash: vec![],
            prev_hash: None,
            source_type: SourceType::Agent,
            source_id: None,
            consolidation_state: ConsolidationState::Raw,
            access_count: 0,
            org_id: None,
            thread_id: None,
            created_at: now,
            updated_at: "2025-01-01T00:00:00Z".to_string(),
            last_accessed_at: None,
            expires_at: None,
            deleted_at: None,
            decay_rate: Some(0.01),
            created_by: None,
            version: 1,
            prev_version_id: None,
            quarantined: false,
            quarantine_reason: None,
            decay_function: None,
        };

        let eff = effective_importance(&record);
        // Fresh memory should be close to base importance
        assert!(eff > 0.7, "effective importance {eff} should be > 0.7 for fresh memory");

        // Old memory with high decay rate
        let old_date = (chrono::Utc::now() - chrono::Duration::hours(1000)).to_rfc3339();
        let old_record = MemoryRecord {
            created_at: old_date,
            decay_rate: Some(0.01),
            access_count: 0,
            ..record.clone()
        };
        let old_eff = effective_importance(&old_record);
        assert!(old_eff < eff, "old memory {old_eff} should have lower importance than fresh {eff}");

        // Access count boosts importance
        let accessed_record = MemoryRecord {
            access_count: 100,
            ..old_record.clone()
        };
        let accessed_eff = effective_importance(&accessed_record);
        assert!(accessed_eff > old_eff, "accessed memory {accessed_eff} should be higher than unaccessed {old_eff}");
    }
}
