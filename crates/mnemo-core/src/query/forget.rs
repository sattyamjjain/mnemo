use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::compute_content_hash;
use crate::model::acl::Permission;
use crate::model::event::{AgentEvent, EventType};
use crate::model::memory::MemoryType;
use crate::query::MnemoEngine;
use crate::storage::MemoryFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgetStrategy {
    SoftDelete,
    HardDelete,
    Decay,
    Consolidate,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgetCriteria {
    pub max_age_hours: Option<f64>,
    pub min_importance_below: Option<f32>,
    pub memory_type: Option<MemoryType>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgetRequest {
    pub memory_ids: Vec<Uuid>,
    pub agent_id: Option<String>,
    pub strategy: Option<ForgetStrategy>,
    pub criteria: Option<ForgetCriteria>,
}

impl ForgetRequest {
    pub fn new(memory_ids: Vec<Uuid>) -> Self {
        Self {
            memory_ids,
            agent_id: None,
            strategy: None,
            criteria: None,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgetResponse {
    pub forgotten: Vec<Uuid>,
    pub errors: Vec<ForgetError>,
}

impl ForgetResponse {
    pub fn new(forgotten: Vec<Uuid>, errors: Vec<ForgetError>) -> Self {
        Self { forgotten, errors }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgetError {
    pub id: Uuid,
    pub error: String,
}

pub async fn execute(engine: &MnemoEngine, request: ForgetRequest) -> Result<ForgetResponse> {
    let agent_id = request.agent_id.unwrap_or_else(|| engine.default_agent_id.clone());
    let strategy = request.strategy.unwrap_or(ForgetStrategy::SoftDelete);

    // If criteria is specified and memory_ids is empty, find matching memories
    let memory_ids = if request.memory_ids.is_empty() {
        if let Some(ref criteria) = request.criteria {
            let filter = MemoryFilter {
                agent_id: Some(agent_id.clone()),
                memory_type: criteria.memory_type,
                min_importance: None, // We'll filter below
                tags: criteria.tags.clone(),
                include_deleted: false,
                ..Default::default()
            };
            let memories = engine.storage.list_memories(&filter, 1000, 0).await?;
            let now = chrono::Utc::now();
            memories
                .into_iter()
                .filter(|m| {
                    if let Some(max_age) = criteria.max_age_hours
                        && let Ok(created) = chrono::DateTime::parse_from_rfc3339(&m.created_at)
                    {
                        let age_hours = (now - created.with_timezone(&chrono::Utc)).num_seconds() as f64 / 3600.0;
                        if age_hours < max_age {
                            return false;
                        }
                    }
                    if let Some(min_below) = criteria.min_importance_below
                        && m.importance >= min_below
                    {
                        return false;
                    }
                    true
                })
                .map(|m| m.id)
                .collect()
        } else {
            return Err(Error::Validation("memory_ids or criteria must be provided".to_string()));
        }
    } else {
        request.memory_ids.clone()
    };

    if memory_ids.is_empty() {
        return Ok(ForgetResponse {
            forgotten: vec![],
            errors: vec![],
        });
    }

    let mut forgotten = Vec::new();
    let mut errors = Vec::new();

    for id in &memory_ids {
        // Check permission
        match engine.storage.check_permission(*id, &agent_id, Permission::Write).await {
            Ok(true) => {}
            Ok(false) => {
                errors.push(ForgetError {
                    id: *id,
                    error: "permission denied".to_string(),
                });
                continue;
            }
            Err(e) => {
                errors.push(ForgetError {
                    id: *id,
                    error: e.to_string(),
                });
                continue;
            }
        }

        // Execute strategy
        match strategy {
            ForgetStrategy::SoftDelete => {
                match engine.storage.soft_delete_memory(*id).await {
                    Ok(()) => {
                        if let Err(e) = engine.index.remove(*id) {
                            tracing::error!(memory_id = %id, error = %e, "failed to remove from vector index during soft delete");
                        }
                        if let Some(ref ft) = engine.full_text {
                            if let Err(e) = ft.remove(*id) {
                                tracing::error!(memory_id = %id, error = %e, "failed to remove from full-text index");
                            }
                            if let Err(e) = ft.commit() {
                                tracing::error!(memory_id = %id, error = %e, "failed to commit full-text index");
                            }
                        }
                        forgotten.push(*id);
                    }
                    Err(e) => {
                        errors.push(ForgetError { id: *id, error: e.to_string() });
                    }
                }
            }
            ForgetStrategy::HardDelete => {
                match engine.storage.hard_delete_memory(*id).await {
                    Ok(()) => {
                        if let Err(e) = engine.index.remove(*id) {
                            tracing::error!(memory_id = %id, error = %e, "failed to remove from vector index during hard delete");
                        }
                        if let Some(ref ft) = engine.full_text {
                            if let Err(e) = ft.remove(*id) {
                                tracing::error!(memory_id = %id, error = %e, "failed to remove from full-text index");
                            }
                            if let Err(e) = ft.commit() {
                                tracing::error!(memory_id = %id, error = %e, "failed to commit full-text index");
                            }
                        }
                        forgotten.push(*id);
                    }
                    Err(e) => {
                        errors.push(ForgetError { id: *id, error: e.to_string() });
                    }
                }
            }
            ForgetStrategy::Decay => {
                match engine.storage.get_memory(*id).await {
                    Ok(Some(mut record)) => {
                        let decay_rate = record.decay_rate.unwrap_or(0.1);
                        record.importance = (record.importance - decay_rate).max(0.0);
                        record.updated_at = chrono::Utc::now().to_rfc3339();
                        match engine.storage.update_memory(&record).await {
                            Ok(()) => forgotten.push(*id),
                            Err(e) => errors.push(ForgetError { id: *id, error: e.to_string() }),
                        }
                    }
                    Ok(None) => errors.push(ForgetError { id: *id, error: "not found".to_string() }),
                    Err(e) => errors.push(ForgetError { id: *id, error: e.to_string() }),
                }
            }
            ForgetStrategy::Archive => {
                match engine.storage.get_memory(*id).await {
                    Ok(Some(mut record)) => {
                        record.consolidation_state = crate::model::memory::ConsolidationState::Archived;
                        record.updated_at = chrono::Utc::now().to_rfc3339();
                        match engine.storage.update_memory(&record).await {
                            Ok(()) => {
                                // Archive to cold storage if configured
                                if let Some(ref cs) = engine.cold_storage
                                    && let Err(e) = cs.archive(&record).await
                                {
                                    tracing::warn!("cold storage archive failed for {}: {e}", id);
                                }
                                forgotten.push(*id);
                            }
                            Err(e) => errors.push(ForgetError { id: *id, error: e.to_string() }),
                        }
                    }
                    Ok(None) => errors.push(ForgetError { id: *id, error: "not found".to_string() }),
                    Err(e) => errors.push(ForgetError { id: *id, error: e.to_string() }),
                }
            }
            ForgetStrategy::Consolidate => {
                match engine.storage.get_memory(*id).await {
                    Ok(Some(mut record)) => {
                        record.consolidation_state = crate::model::memory::ConsolidationState::Consolidated;
                        record.updated_at = chrono::Utc::now().to_rfc3339();
                        match engine.storage.update_memory(&record).await {
                            Ok(()) => forgotten.push(*id),
                            Err(e) => errors.push(ForgetError { id: *id, error: e.to_string() }),
                        }
                    }
                    Ok(None) => errors.push(ForgetError { id: *id, error: "not found".to_string() }),
                    Err(e) => errors.push(ForgetError { id: *id, error: e.to_string() }),
                }
            }
        }
    }

    // Emit MemoryDelete event for each forgotten memory with hash chaining (fire-and-forget)
    let now = chrono::Utc::now().to_rfc3339();
    for id in &forgotten {
        let event_content_hash = compute_content_hash(&id.to_string(), &agent_id, &now);
        let prev_event_hash = match engine.storage.get_latest_event_hash(&agent_id, None).await {
            Ok(hash) => hash,
            Err(e) => {
                tracing::warn!(error = %e, "failed to get latest event hash, starting new chain segment");
                None
            }
        };
        let event_prev_hash = Some(crate::hash::compute_chain_hash(&event_content_hash, prev_event_hash.as_deref()));
        let event = AgentEvent {
            id: Uuid::now_v7(),
            agent_id: agent_id.clone(),
            thread_id: None,
            run_id: None,
            parent_event_id: None,
            event_type: EventType::MemoryDelete,
            payload: serde_json::json!({"memory_id": id.to_string()}),
            trace_id: None,
            span_id: None,
            model: None,
            tokens_input: None,
            tokens_output: None,
            latency_ms: None,
            cost_usd: None,
            timestamp: now.clone(),
            logical_clock: 0,
            content_hash: event_content_hash,
            prev_hash: event_prev_hash,
            embedding: None,
        };
        if let Err(e) = engine.storage.insert_event(&event).await {
            tracing::error!(event_id = %event.id, error = %e, "failed to insert audit event");
        }

        // Invalidate cache on forget
        if let Some(ref cache) = engine.cache {
            cache.invalidate(*id);
        }
    }

    Ok(ForgetResponse { forgotten, errors })
}
