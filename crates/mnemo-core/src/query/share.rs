use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::compute_content_hash;
use crate::model::acl::{Acl, Permission, PrincipalType};
use crate::model::event::{AgentEvent, EventType};
use crate::model::memory::Scope;
use crate::query::MnemoEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRequest {
    pub memory_id: Uuid,
    pub agent_id: Option<String>,
    pub target_agent_id: String,
    pub target_agent_ids: Option<Vec<String>>,
    pub permission: Option<Permission>,
    pub expires_in_hours: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareResponse {
    pub acl_id: Uuid,
    pub acl_ids: Vec<Uuid>,
    pub memory_id: Uuid,
    pub shared_with: String,
    pub shared_with_all: Vec<String>,
    pub permission: Permission,
}

pub async fn execute(engine: &MnemoEngine, request: ShareRequest) -> Result<ShareResponse> {
    let agent_id = request.agent_id.unwrap_or_else(|| engine.default_agent_id.clone());
    let permission = request.permission.unwrap_or(Permission::Read);

    // Verify the requester owns or has admin access to the memory
    let has_access = engine
        .storage
        .check_permission(request.memory_id, &agent_id, Permission::Admin)
        .await?;

    if !has_access {
        return Err(Error::PermissionDenied(format!(
            "agent {agent_id} cannot share memory {}",
            request.memory_id
        )));
    }

    // Build list of targets: multi-target takes precedence over single target
    let targets = if let Some(ref ids) = request.target_agent_ids {
        ids.clone()
    } else {
        vec![request.target_agent_id.clone()]
    };

    // Compute expiration from expires_in_hours
    let expires_at = request.expires_in_hours.map(|h| {
        let exp = chrono::Utc::now() + chrono::Duration::seconds((h * 3600.0) as i64);
        exp.to_rfc3339()
    });

    let now = chrono::Utc::now().to_rfc3339();
    let mut acl_ids = Vec::new();

    for target in &targets {
        let acl_id = Uuid::now_v7();
        let acl = Acl {
            id: acl_id,
            memory_id: request.memory_id,
            principal_type: PrincipalType::Agent,
            principal_id: target.clone(),
            permission,
            granted_by: agent_id.clone(),
            created_at: now.clone(),
            expires_at: expires_at.clone(),
        };
        engine.storage.insert_acl(&acl).await?;
        acl_ids.push(acl_id);
    }

    // Optionally update scope to Shared if it was Private
    if let Some(mut record) = engine.storage.get_memory(request.memory_id).await? {
        if record.scope == Scope::Private {
            record.scope = Scope::Shared;
            record.updated_at = now.clone();
            engine.storage.update_memory(&record).await?;
        }
    }

    // Emit MemoryShare event (fire-and-forget)
    let mut event = AgentEvent {
        id: Uuid::now_v7(),
        agent_id: agent_id.clone(),
        thread_id: None,
        run_id: None,
        parent_event_id: None,
        event_type: EventType::MemoryShare,
        payload: serde_json::json!({
            "memory_id": request.memory_id.to_string(),
            "shared_with": targets,
            "permission": permission.to_string(),
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
        content_hash: compute_content_hash(&request.memory_id.to_string(), &agent_id, &now),
        prev_hash: None,
        embedding: None,
    };
    // Optionally embed the event payload
    if engine.embed_events {
        if let Ok(emb) = engine.embedding.embed(&event.payload.to_string()).await {
            event.embedding = Some(emb);
        }
    }
    let _ = engine.storage.insert_event(&event).await;

    let first_acl_id = acl_ids[0];
    let first_target = targets[0].clone();

    Ok(ShareResponse {
        acl_id: first_acl_id,
        acl_ids,
        memory_id: request.memory_id,
        shared_with: first_target,
        shared_with_all: targets,
        permission,
    })
}
