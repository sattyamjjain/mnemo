use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::checkpoint::Checkpoint;
use crate::model::event::EventType;
use crate::query::MnemoEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchRequest {
    pub thread_id: String,
    pub agent_id: Option<String>,
    pub new_branch_name: String,
    pub source_checkpoint_id: Option<Uuid>,
    pub source_branch: Option<String>,
}

impl BranchRequest {
    pub fn new(thread_id: String, new_branch_name: String) -> Self {
        Self {
            thread_id,
            agent_id: None,
            new_branch_name,
            source_checkpoint_id: None,
            source_branch: None,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchResponse {
    pub checkpoint_id: Uuid,
    pub branch_name: String,
    pub source_checkpoint_id: Uuid,
}

impl BranchResponse {
    pub fn new(checkpoint_id: Uuid, branch_name: String, source_checkpoint_id: Uuid) -> Self {
        Self {
            checkpoint_id,
            branch_name,
            source_checkpoint_id,
        }
    }
}

pub async fn execute(engine: &MnemoEngine, request: BranchRequest) -> Result<BranchResponse> {
    let agent_id = request.agent_id.unwrap_or_else(|| engine.default_agent_id.clone());
    let now = chrono::Utc::now().to_rfc3339();

    // Find source checkpoint
    let source_cp = if let Some(cp_id) = request.source_checkpoint_id {
        engine
            .storage
            .get_checkpoint(cp_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("checkpoint {cp_id} not found")))?
    } else {
        let source_branch = request.source_branch.as_deref().unwrap_or("main");
        engine
            .storage
            .get_latest_checkpoint(&request.thread_id, source_branch)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "no checkpoint found on branch '{source_branch}' for thread '{}'",
                    request.thread_id
                ))
            })?
    };

    // Create new checkpoint on the new branch with parent = source
    let id = Uuid::now_v7();
    let new_cp = Checkpoint {
        id,
        thread_id: request.thread_id.clone(),
        agent_id: agent_id.clone(),
        parent_id: Some(source_cp.id),
        branch_name: request.new_branch_name.clone(),
        state_snapshot: source_cp.state_snapshot.clone(),
        state_diff: None,
        memory_refs: source_cp.memory_refs.clone(),
        event_cursor: source_cp.event_cursor,
        label: Some(format!("branch from {}", source_cp.id)),
        created_at: now.clone(),
        metadata: serde_json::json!({"branched_from": source_cp.id.to_string()}),
    };

    engine.storage.insert_checkpoint(&new_cp).await?;

    // Emit Branch event
    let event = super::event_builder::build_event(
        engine,
        &agent_id,
        EventType::Branch,
        serde_json::json!({
            "checkpoint_id": id.to_string(),
            "new_branch": request.new_branch_name,
            "source_checkpoint": source_cp.id.to_string(),
        }),
        &id.to_string(),
        Some(request.thread_id),
    ).await;
    if let Err(e) = engine.storage.insert_event(&event).await {
        tracing::error!(event_id = %event.id, error = %e, "failed to insert audit event");
    }

    Ok(BranchResponse {
        checkpoint_id: id,
        branch_name: request.new_branch_name,
        source_checkpoint_id: source_cp.id,
    })
}
