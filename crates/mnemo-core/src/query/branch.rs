use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::compute_content_hash;
use crate::model::checkpoint::Checkpoint;
use crate::model::event::{AgentEvent, EventType};
use crate::query::MnemoEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchRequest {
    pub thread_id: String,
    pub agent_id: Option<String>,
    pub new_branch_name: String,
    pub source_checkpoint_id: Option<Uuid>,
    pub source_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchResponse {
    pub checkpoint_id: Uuid,
    pub branch_name: String,
    pub source_checkpoint_id: Uuid,
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
    let event = AgentEvent {
        id: Uuid::now_v7(),
        agent_id,
        thread_id: Some(request.thread_id),
        run_id: None,
        parent_event_id: None,
        event_type: EventType::Branch,
        payload: serde_json::json!({
            "checkpoint_id": id.to_string(),
            "new_branch": request.new_branch_name,
            "source_checkpoint": source_cp.id.to_string(),
        }),
        trace_id: None,
        span_id: None,
        model: None,
        tokens_input: None,
        tokens_output: None,
        latency_ms: None,
        cost_usd: None,
        timestamp: now.clone(),
        logical_clock: 0,
        content_hash: compute_content_hash(&id.to_string(), &new_cp.agent_id, &now),
        prev_hash: None,
        embedding: None,
    };
    let _ = engine.storage.insert_event(&event).await;

    Ok(BranchResponse {
        checkpoint_id: id,
        branch_name: request.new_branch_name,
        source_checkpoint_id: source_cp.id,
    })
}
