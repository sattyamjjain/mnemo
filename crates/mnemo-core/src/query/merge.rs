use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::checkpoint::Checkpoint;
use crate::model::event::EventType;
use crate::query::MnemoEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    FullMerge,
    CherryPick,
    Squash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRequest {
    pub thread_id: String,
    pub agent_id: Option<String>,
    pub source_branch: String,
    pub target_branch: Option<String>,
    pub strategy: Option<MergeStrategy>,
    pub cherry_pick_ids: Option<Vec<Uuid>>,
}

impl MergeRequest {
    pub fn new(thread_id: String, source_branch: String) -> Self {
        Self {
            thread_id,
            agent_id: None,
            source_branch,
            target_branch: None,
            strategy: None,
            cherry_pick_ids: None,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResponse {
    pub checkpoint_id: Uuid,
    pub target_branch: String,
    pub merged_memory_count: usize,
}

impl MergeResponse {
    pub fn new(checkpoint_id: Uuid, target_branch: String, merged_memory_count: usize) -> Self {
        Self {
            checkpoint_id,
            target_branch,
            merged_memory_count,
        }
    }
}

pub async fn execute(engine: &MnemoEngine, request: MergeRequest) -> Result<MergeResponse> {
    let agent_id = request.agent_id.unwrap_or_else(|| engine.default_agent_id.clone());
    let target_branch = request.target_branch.unwrap_or_else(|| "main".to_string());
    let strategy = request.strategy.unwrap_or(MergeStrategy::FullMerge);
    let now = chrono::Utc::now().to_rfc3339();

    // Get latest checkpoint on source branch
    let source_cp = engine
        .storage
        .get_latest_checkpoint(&request.thread_id, &request.source_branch)
        .await?
        .ok_or_else(|| {
            Error::NotFound(format!(
                "no checkpoint on branch '{}' for thread '{}'",
                request.source_branch, request.thread_id
            ))
        })?;

    // Get latest checkpoint on target branch (may not exist yet)
    let target_cp = engine
        .storage
        .get_latest_checkpoint(&request.thread_id, &target_branch)
        .await?;

    let target_parent_id = target_cp.as_ref().map(|cp| cp.id);

    // Determine merged memory_refs based on strategy
    let merged_refs: Vec<Uuid> = match strategy {
        MergeStrategy::CherryPick => {
            let cherry = request.cherry_pick_ids.unwrap_or_default();
            let mut existing = target_cp
                .as_ref()
                .map(|cp| cp.memory_refs.clone())
                .unwrap_or_default();
            for id in &cherry {
                if !existing.contains(id) {
                    existing.push(*id);
                }
            }
            existing
        }
        MergeStrategy::FullMerge | MergeStrategy::Squash => {
            let mut merged = target_cp
                .as_ref()
                .map(|cp| cp.memory_refs.clone())
                .unwrap_or_default();
            for id in &source_cp.memory_refs {
                if !merged.contains(id) {
                    merged.push(*id);
                }
            }
            merged
        }
    };

    let merged_count = merged_refs.len();

    // Merge state snapshots (target takes precedence, source fields added)
    let merged_snapshot = if let Some(ref tcp) = target_cp {
        let mut base = tcp.state_snapshot.clone();
        if let (Some(base_obj), Some(source_obj)) = (base.as_object_mut(), source_cp.state_snapshot.as_object()) {
            for (k, v) in source_obj {
                if !base_obj.contains_key(k) {
                    base_obj.insert(k.clone(), v.clone());
                }
            }
        }
        base
    } else {
        source_cp.state_snapshot.clone()
    };

    let id = Uuid::now_v7();
    let new_cp = Checkpoint {
        id,
        thread_id: request.thread_id.clone(),
        agent_id: agent_id.clone(),
        parent_id: target_parent_id,
        branch_name: target_branch.clone(),
        state_snapshot: merged_snapshot,
        state_diff: Some(serde_json::json!({
            "merge_source": request.source_branch,
            "strategy": format!("{strategy:?}"),
        })),
        memory_refs: merged_refs,
        event_cursor: source_cp.event_cursor,
        label: Some(format!("merge from {}", request.source_branch)),
        created_at: now.clone(),
        metadata: serde_json::json!({
            "source_branch": request.source_branch,
            "source_checkpoint": source_cp.id.to_string(),
        }),
    };

    engine.storage.insert_checkpoint(&new_cp).await?;

    // Emit Merge event
    let event = super::event_builder::build_event(
        engine,
        &agent_id,
        EventType::Merge,
        serde_json::json!({
            "checkpoint_id": id.to_string(),
            "source_branch": request.source_branch,
            "target_branch": target_branch,
            "strategy": format!("{strategy:?}"),
        }),
        &id.to_string(),
        Some(request.thread_id),
    ).await;
    if let Err(e) = engine.storage.insert_event(&event).await {
        tracing::error!(event_id = %event.id, error = %e, "failed to insert audit event");
    }

    Ok(MergeResponse {
        checkpoint_id: id,
        target_branch,
        merged_memory_count: merged_count,
    })
}
