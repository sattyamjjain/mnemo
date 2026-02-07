use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::model::memory::{ConsolidationState, MemoryRecord, MemoryType, SourceType};
use crate::query::MnemoEngine;
use crate::storage::MemoryFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictDetectionResult {
    pub conflicts: Vec<ConflictPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictPair {
    pub memory_a: Uuid,
    pub memory_b: Uuid,
    pub similarity: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStrategy {
    KeepNewest,
    KeepHighestImportance,
    MergeIntoSemantic,
    Manual,
    EvidenceWeighted,
}

/// Scoring components for evidence-weighted conflict resolution.
#[derive(Debug, Clone, Serialize)]
pub struct ConflictEvidence {
    pub source_reliability: f32,
    pub recency_score: f32,
    pub usage_score: f32,
    pub importance_score: f32,
    pub similarity_bonus: f32,
    pub composite_weight: f32,
}

/// Returns a reliability score for each source type.
/// Higher values = more trustworthy.
pub fn source_reliability(st: SourceType) -> f32 {
    match st {
        SourceType::ToolOutput => 0.9,
        SourceType::Human | SourceType::UserInput => 0.8,
        SourceType::System => 0.75,
        SourceType::ModelResponse => 0.7,
        SourceType::Agent => 0.6,
        SourceType::Consolidation => 0.5,
        SourceType::Retrieval => 0.4,
        SourceType::Import => 0.3,
    }
}

fn compute_evidence(record: &MemoryRecord, max_access: u64, similarity: f32) -> ConflictEvidence {
    let src_rel = source_reliability(record.source_type);
    let recency = crate::query::retrieval::recency_score(&record.created_at, 168.0);
    let usage = if max_access > 0 {
        record.access_count as f32 / max_access as f32
    } else {
        0.0
    };
    let importance = record.importance;
    let sim_bonus = similarity;

    let composite = src_rel * 0.3 + recency * 0.2 + usage * 0.2 + importance * 0.2 + sim_bonus * 0.1;

    ConflictEvidence {
        source_reliability: src_rel,
        recency_score: recency,
        usage_score: usage,
        importance_score: importance,
        similarity_bonus: sim_bonus,
        composite_weight: composite,
    }
}

/// Detect potential conflicts (near-duplicate memories) for an agent.
/// Uses the vector index to find memories with cosine similarity above threshold.
pub async fn detect_conflicts(
    engine: &MnemoEngine,
    agent_id: &str,
    threshold: f32,
) -> Result<ConflictDetectionResult> {
    let filter = MemoryFilter {
        agent_id: Some(agent_id.to_string()),
        include_deleted: false,
        ..Default::default()
    };
    let memories = engine.storage.list_memories(&filter, 1000, 0).await?;

    let mut conflicts = Vec::new();
    let mut checked: std::collections::HashSet<(Uuid, Uuid)> = std::collections::HashSet::new();

    for record in &memories {
        if record.quarantined {
            continue;
        }
        let embedding = match &record.embedding {
            Some(e) => e,
            None => continue,
        };

        // Search for similar memories using the vector index
        let results = engine.index.search(embedding, 20)?;

        for (candidate_id, distance) in results {
            if candidate_id == record.id {
                continue;
            }
            let similarity = 1.0 - distance;
            if similarity < threshold {
                continue;
            }

            // Avoid duplicate pairs
            let pair = if record.id < candidate_id {
                (record.id, candidate_id)
            } else {
                (candidate_id, record.id)
            };
            if !checked.insert(pair) {
                continue;
            }

            // Verify the candidate belongs to the same agent
            if let Some(candidate) = engine.storage.get_memory(candidate_id).await? {
                if candidate.agent_id != agent_id || candidate.is_deleted() || candidate.quarantined {
                    continue;
                }
                if candidate.content != record.content {
                    conflicts.push(ConflictPair {
                        memory_a: record.id,
                        memory_b: candidate_id,
                        similarity,
                        reason: format!(
                            "High semantic similarity ({:.3}) between different content",
                            similarity
                        ),
                    });
                }
            }
        }
    }

    Ok(ConflictDetectionResult { conflicts })
}

/// Resolve a detected conflict using the specified strategy.
pub async fn resolve_conflict(
    engine: &MnemoEngine,
    conflict: &ConflictPair,
    strategy: ResolutionStrategy,
) -> Result<()> {
    let mem_a = engine.storage.get_memory(conflict.memory_a).await?
        .ok_or_else(|| crate::error::Error::NotFound(format!("memory {} not found", conflict.memory_a)))?;
    let mem_b = engine.storage.get_memory(conflict.memory_b).await?
        .ok_or_else(|| crate::error::Error::NotFound(format!("memory {} not found", conflict.memory_b)))?;

    match strategy {
        ResolutionStrategy::KeepNewest => {
            // Soft-delete the older memory
            if mem_a.created_at >= mem_b.created_at {
                engine.storage.soft_delete_memory(mem_b.id).await?;
            } else {
                engine.storage.soft_delete_memory(mem_a.id).await?;
            }
        }
        ResolutionStrategy::KeepHighestImportance => {
            if mem_a.importance >= mem_b.importance {
                engine.storage.soft_delete_memory(mem_b.id).await?;
            } else {
                engine.storage.soft_delete_memory(mem_a.id).await?;
            }
        }
        ResolutionStrategy::MergeIntoSemantic => {
            // Create new semantic memory combining both, soft-delete originals
            let combined_content = format!("{} | {}", mem_a.content, mem_b.content);
            let avg_importance = (mem_a.importance + mem_b.importance) / 2.0;
            let mut all_tags: Vec<String> = mem_a.tags.clone();
            for t in &mem_b.tags {
                if !all_tags.contains(t) {
                    all_tags.push(t.clone());
                }
            }

            let now = chrono::Utc::now().to_rfc3339();
            let embedding = engine.embedding.embed(&combined_content).await?;
            let content_hash = crate::hash::compute_content_hash(&combined_content, &mem_a.agent_id, &now);

            let new_record = MemoryRecord {
                id: Uuid::now_v7(),
                agent_id: mem_a.agent_id.clone(),
                content: combined_content,
                memory_type: MemoryType::Semantic,
                scope: mem_a.scope,
                importance: avg_importance,
                tags: all_tags,
                metadata: serde_json::json!({
                    "merged_from": [mem_a.id.to_string(), mem_b.id.to_string()]
                }),
                embedding: Some(embedding.clone()),
                content_hash,
                prev_hash: None,
                source_type: SourceType::Consolidation,
                source_id: None,
                consolidation_state: ConsolidationState::Active,
                access_count: 0,
                org_id: mem_a.org_id.clone(),
                thread_id: None,
                created_at: now.clone(),
                updated_at: now,
                last_accessed_at: None,
                expires_at: None,
                deleted_at: None,
                decay_rate: None,
                created_by: Some("conflict_resolution".to_string()),
                version: 1,
                prev_version_id: None,
                quarantined: false,
                quarantine_reason: None,
                decay_function: None,
            };

            engine.storage.insert_memory(&new_record).await?;
            engine.index.add(new_record.id, &embedding)?;
            if let Some(ref ft) = engine.full_text {
                ft.add(new_record.id, &new_record.content)?;
                ft.commit()?;
            }

            engine.storage.soft_delete_memory(mem_a.id).await?;
            engine.storage.soft_delete_memory(mem_b.id).await?;
        }
        ResolutionStrategy::Manual => {
            // No-op: just flag for manual review
        }
        ResolutionStrategy::EvidenceWeighted => {
            let max_access = mem_a.access_count.max(mem_b.access_count);
            let evidence_a = compute_evidence(&mem_a, max_access, conflict.similarity);
            let evidence_b = compute_evidence(&mem_b, max_access, conflict.similarity);

            let (winner, loser, winner_evidence, loser_evidence) =
                if evidence_a.composite_weight >= evidence_b.composite_weight {
                    (&mem_a, &mem_b, &evidence_a, &evidence_b)
                } else {
                    (&mem_b, &mem_a, &evidence_b, &evidence_a)
                };

            // Soft-delete the loser
            engine.storage.soft_delete_memory(loser.id).await?;

            // Store resolution metadata in winner's metadata
            let mut winner_record = winner.clone();
            let mut meta = winner_record.metadata.as_object().cloned().unwrap_or_default();
            meta.insert(
                "conflict_resolution".to_string(),
                serde_json::json!({
                    "strategy": "evidence_weighted",
                    "defeated_id": loser.id.to_string(),
                    "winner_score": winner_evidence.composite_weight,
                    "loser_score": loser_evidence.composite_weight,
                    "winner_evidence": {
                        "source_reliability": winner_evidence.source_reliability,
                        "recency_score": winner_evidence.recency_score,
                        "usage_score": winner_evidence.usage_score,
                        "importance_score": winner_evidence.importance_score,
                    },
                }),
            );
            winner_record.metadata = serde_json::Value::Object(meta);
            engine.storage.update_memory(&winner_record).await?;
        }
    }

    Ok(())
}
