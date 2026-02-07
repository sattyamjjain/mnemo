use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::{verify_chain, ChainVerificationResult};
use crate::model::checkpoint::Checkpoint;
use crate::model::event::AgentEvent;
use crate::model::memory::MemoryRecord;
use crate::query::MnemoEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayRequest {
    pub thread_id: String,
    pub agent_id: Option<String>,
    pub checkpoint_id: Option<Uuid>,
    pub branch_name: Option<String>,
}

impl ReplayRequest {
    pub fn new(thread_id: String) -> Self {
        Self {
            thread_id,
            agent_id: None,
            checkpoint_id: None,
            branch_name: None,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayResponse {
    pub checkpoint: Checkpoint,
    pub memories: Vec<MemoryRecord>,
    pub events: Vec<AgentEvent>,
    pub chain_verification: Option<ChainVerificationResult>,
}

impl ReplayResponse {
    pub fn new(
        checkpoint: Checkpoint,
        memories: Vec<MemoryRecord>,
        events: Vec<AgentEvent>,
        chain_verification: Option<ChainVerificationResult>,
    ) -> Self {
        Self {
            checkpoint,
            memories,
            events,
            chain_verification,
        }
    }
}

pub async fn execute(engine: &MnemoEngine, request: ReplayRequest) -> Result<ReplayResponse> {
    let branch = request.branch_name.as_deref().unwrap_or("main");

    // Get checkpoint (specified or latest)
    let checkpoint = if let Some(cp_id) = request.checkpoint_id {
        engine
            .storage
            .get_checkpoint(cp_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("checkpoint {cp_id} not found")))?
    } else {
        engine
            .storage
            .get_latest_checkpoint(&request.thread_id, branch)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "no checkpoint found on branch '{branch}' for thread '{}'",
                    request.thread_id
                ))
            })?
    };

    // Load memories referenced by checkpoint.memory_refs
    let mut memories = Vec::new();
    for mem_id in &checkpoint.memory_refs {
        if let Some(record) = engine.storage.get_memory(*mem_id).await? {
            memories.push(record);
        }
    }

    // Verify hash chain integrity on loaded memories
    let chain_verification = Some(verify_chain(&memories));

    // Load events up to checkpoint.event_cursor (or all thread events if no cursor)
    let events = engine
        .storage
        .get_events_by_thread(&checkpoint.thread_id, 1000)
        .await?;

    let events = if let Some(cursor_id) = checkpoint.event_cursor {
        // Return events up to and including the cursor
        let mut filtered = Vec::new();
        for event in events {
            filtered.push(event.clone());
            if event.id == cursor_id {
                break;
            }
        }
        filtered
    } else {
        events
    };

    Ok(ReplayResponse {
        checkpoint,
        memories,
        events,
        chain_verification,
    })
}
