use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::{verify_chain, ChainVerificationResult};
use crate::model::checkpoint::Checkpoint;
use crate::model::event::AgentEvent;
use crate::model::memory::MemoryRecord;
use crate::query::MnemoEngine;
use crate::storage::MemoryFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayRequest {
    pub thread_id: String,
    pub agent_id: Option<String>,
    pub checkpoint_id: Option<Uuid>,
    pub branch_name: Option<String>,
    /// Synthesize a virtual checkpoint from the memories and events that
    /// existed at this RFC3339 timestamp. When set, `checkpoint_id` and
    /// `branch_name` are ignored.
    pub as_of: Option<String>,
}

impl ReplayRequest {
    pub fn new(thread_id: String) -> Self {
        Self {
            thread_id,
            agent_id: None,
            checkpoint_id: None,
            branch_name: None,
            as_of: None,
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
    // Time-travel path: synthesize a virtual checkpoint at `as_of`.
    if let Some(ref as_of) = request.as_of {
        return replay_as_of(engine, &request, as_of).await;
    }

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

/// Build a synthetic `Checkpoint` that describes agent state as it existed at
/// `as_of_str` — every memory created at or before that instant, excluding
/// memories already deleted. Events are filtered by timestamp identically so
/// the returned `ReplayResponse` looks like a real checkpoint from that time.
async fn replay_as_of(
    engine: &MnemoEngine,
    request: &ReplayRequest,
    as_of_str: &str,
) -> Result<ReplayResponse> {
    let as_of = chrono::DateTime::parse_from_rfc3339(as_of_str)
        .map_err(|e| Error::Validation(format!("invalid as_of timestamp '{as_of_str}': {e}")))?
        .with_timezone(&chrono::Utc);

    let agent_id = request
        .agent_id
        .clone()
        .unwrap_or_else(|| engine.default_agent_id.clone());
    super::validate_agent_id(&agent_id)?;

    // Pull all memories for the agent (including soft-deleted ones, so we can
    // decide per-record whether they existed at `as_of`).
    let filter = MemoryFilter {
        agent_id: Some(agent_id.clone()),
        thread_id: Some(request.thread_id.clone()),
        include_deleted: true,
        ..Default::default()
    };
    let candidates = engine
        .storage
        .list_memories(&filter, super::MAX_BATCH_QUERY_LIMIT, 0)
        .await?;

    let mut memories: Vec<MemoryRecord> = Vec::new();
    for record in candidates {
        let Ok(created) = chrono::DateTime::parse_from_rfc3339(&record.created_at) else {
            continue;
        };
        if created.with_timezone(&chrono::Utc) > as_of {
            continue;
        }
        if let Some(ref deleted_at) = record.deleted_at
            && let Ok(del) = chrono::DateTime::parse_from_rfc3339(deleted_at)
            && del.with_timezone(&chrono::Utc) <= as_of
        {
            continue;
        }
        memories.push(record);
    }

    let chain_verification = Some(verify_chain(&memories));

    let all_events = engine
        .storage
        .get_events_by_thread(&request.thread_id, super::MAX_BATCH_QUERY_LIMIT)
        .await?;
    let events: Vec<AgentEvent> = all_events
        .into_iter()
        .filter(|e| {
            chrono::DateTime::parse_from_rfc3339(&e.timestamp)
                .map(|ts| ts.with_timezone(&chrono::Utc) <= as_of)
                .unwrap_or(false)
        })
        .collect();

    let memory_refs: Vec<Uuid> = memories.iter().map(|m| m.id).collect();

    let virtual_checkpoint = Checkpoint {
        id: Uuid::nil(),
        thread_id: request.thread_id.clone(),
        agent_id,
        parent_id: None,
        branch_name: request
            .branch_name
            .clone()
            .unwrap_or_else(|| "main".to_string()),
        state_snapshot: serde_json::json!({
            "as_of": as_of_str,
            "virtual": true,
        }),
        state_diff: None,
        memory_refs,
        event_cursor: events.last().map(|e| e.id),
        label: Some(format!("virtual@{as_of_str}")),
        created_at: as_of_str.to_string(),
        metadata: serde_json::json!({"synthesized": true}),
    };

    Ok(ReplayResponse {
        checkpoint: virtual_checkpoint,
        memories,
        events,
        chain_verification,
    })
}
