use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::model::checkpoint::Checkpoint;
use crate::model::event::EventType;
use crate::query::MnemoEngine;
use crate::storage::MemoryFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRequest {
    pub thread_id: String,
    pub agent_id: Option<String>,
    pub branch_name: Option<String>,
    pub state_snapshot: serde_json::Value,
    pub label: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

impl CheckpointRequest {
    pub fn new(thread_id: String, state_snapshot: serde_json::Value) -> Self {
        Self {
            thread_id,
            agent_id: None,
            branch_name: None,
            state_snapshot,
            label: None,
            metadata: None,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointResponse {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub branch_name: String,
}

impl CheckpointResponse {
    pub fn new(id: Uuid, parent_id: Option<Uuid>, branch_name: String) -> Self {
        Self {
            id,
            parent_id,
            branch_name,
        }
    }
}

pub async fn execute(engine: &MnemoEngine, request: CheckpointRequest) -> Result<CheckpointResponse> {
    let agent_id = request.agent_id.unwrap_or_else(|| engine.default_agent_id.clone());
    let branch_name = request.branch_name.unwrap_or_else(|| "main".to_string());
    let now = chrono::Utc::now().to_rfc3339();

    // Get latest checkpoint on this branch as parent
    let parent = engine
        .storage
        .get_latest_checkpoint(&request.thread_id, &branch_name)
        .await?;

    let parent_id = parent.as_ref().map(|p| p.id);

    // Compute state_diff from parent
    let state_diff = parent.as_ref().map(|p| {
        serde_json::json!({
            "from": p.state_snapshot,
            "to": request.state_snapshot,
        })
    });

    // Collect memory_refs — active memories for this agent
    let filter = MemoryFilter {
        agent_id: Some(agent_id.clone()),
        ..Default::default()
    };
    let memories = engine.storage.list_memories(&filter, 1000, 0).await?;
    let memory_refs: Vec<Uuid> = memories.iter().map(|m| m.id).collect();

    // Get latest event as cursor
    let events = engine.storage.list_events(&agent_id, 1, 0).await?;
    let event_cursor = events.first().map(|e| e.id);

    let id = Uuid::now_v7();
    let cp = Checkpoint {
        id,
        thread_id: request.thread_id.clone(),
        agent_id: agent_id.clone(),
        parent_id,
        branch_name: branch_name.clone(),
        state_snapshot: request.state_snapshot,
        state_diff,
        memory_refs,
        event_cursor,
        label: request.label,
        created_at: now.clone(),
        metadata: request.metadata.unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
    };

    engine.storage.insert_checkpoint(&cp).await?;

    // Emit Checkpoint event
    let event = super::event_builder::build_event(
        engine,
        &agent_id,
        EventType::Checkpoint,
        serde_json::json!({"checkpoint_id": id.to_string(), "branch": branch_name}),
        &id.to_string(),
        Some(request.thread_id),
    ).await;
    if let Err(e) = engine.storage.insert_event(&event).await {
        tracing::error!(event_id = %event.id, error = %e, "failed to insert audit event");
    }

    Ok(CheckpointResponse {
        id,
        parent_id,
        branch_name,
    })
}
