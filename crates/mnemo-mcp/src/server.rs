use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
    ErrorData as McpError, ServerHandler,
};

use mnemo_core::model::memory::{MemoryType, Scope, SourceType};
use mnemo_core::query::branch::BranchRequest;
use mnemo_core::query::checkpoint::CheckpointRequest;
use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy};
use mnemo_core::query::merge::{MergeRequest, MergeStrategy};
use mnemo_core::query::recall::{RecallRequest, TemporalRange};
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::query::replay::ReplayRequest;
use mnemo_core::query::share::ShareRequest;
use mnemo_core::query::MnemoEngine;

use crate::tools::branch::BranchInput;
use crate::tools::checkpoint::CheckpointInput;
use crate::tools::forget::ForgetInput;
use crate::tools::merge::MergeInput;
use crate::tools::recall::RecallInput;
use crate::tools::remember::RememberInput;
use crate::tools::replay::ReplayInput;
use crate::tools::delegate::DelegateInput;
use crate::tools::share::ShareInput;
use crate::tools::verify::VerifyInput;

#[derive(Clone)]
pub struct MnemoServer {
    engine: Arc<MnemoEngine>,
    tool_router: ToolRouter<Self>,
    activity_tracker: Option<Arc<AtomicU64>>,
}

impl MnemoServer {
    fn touch_activity(&self) {
        if let Some(ref t) = self.activity_tracker {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            t.store(now, Ordering::Relaxed);
        }
    }
}

#[tool_router]
impl MnemoServer {
    pub fn new(engine: Arc<MnemoEngine>) -> Self {
        Self {
            engine,
            tool_router: Self::tool_router(),
            activity_tracker: None,
        }
    }

    pub fn with_activity_tracker(mut self, tracker: Arc<AtomicU64>) -> Self {
        self.activity_tracker = Some(tracker);
        self
    }

    #[tool(
        name = "mnemo.remember",
        description = "Store a new memory. Use this to save facts, preferences, instructions, experiences, or any information that should be remembered for later. Memories are searchable by semantic similarity and keyword search."
    )]
    async fn remember(
        &self,
        Parameters(input): Parameters<RememberInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let memory_type = input
            .memory_type
            .and_then(|s| s.parse::<MemoryType>().ok());
        let scope = input.scope.and_then(|s| s.parse::<Scope>().ok());

        let source_type = input.source_type.as_deref().and_then(parse_source_type);

        let request = RememberRequest {
            content: input.content,
            agent_id: None,
            memory_type,
            scope,
            importance: input.importance,
            tags: input.tags,
            metadata: input.metadata,
            source_type,
            source_id: input.source_id,
            org_id: input.org_id,
            thread_id: input.thread_id,
            ttl_seconds: input.ttl_seconds,
            related_to: input.related_to,
            decay_rate: input.decay_rate,
            created_by: input.created_by,
        };

        match self.engine.remember(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "id": response.id.to_string(),
                    "content_hash": response.content_hash,
                    "status": "remembered"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.recall",
        description = "Search and retrieve memories. Supports semantic search (vector similarity), lexical search (keyword BM25), and hybrid search (combining both with recency). Returns the most relevant memories ranked by score."
    )]
    async fn recall(
        &self,
        Parameters(input): Parameters<RecallInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let memory_type = input
            .memory_type
            .and_then(|s| s.parse::<MemoryType>().ok());

        let memory_types = input.memory_types.map(|types| {
            types.iter().filter_map(|s| s.parse::<MemoryType>().ok()).collect()
        });

        let scope = input.scope.and_then(|s| s.parse::<Scope>().ok());

        let temporal_range = input.temporal_range.map(|tr| TemporalRange {
            after: tr.after,
            before: tr.before,
        });

        let request = RecallRequest {
            query: input.query,
            agent_id: None,
            limit: input.limit,
            memory_type,
            memory_types,
            scope,
            min_importance: input.min_importance,
            tags: input.tags,
            org_id: input.org_id,
            strategy: input.strategy,
            temporal_range,
            recency_half_life_hours: input.recency_half_life_hours,
            hybrid_weights: input.hybrid_weights,
            rrf_k: input.rrf_k,
            as_of: input.as_of,
        };

        match self.engine.recall(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "memories": response.memories,
                    "total": response.total
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.forget",
        description = "Delete one or more memories by ID. Supports soft delete (recoverable) or hard delete (permanent). Use this to remove outdated, incorrect, or no longer needed information."
    )]
    async fn forget(
        &self,
        Parameters(input): Parameters<ForgetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let memory_ids: Result<Vec<uuid::Uuid>, _> = input
            .memory_ids
            .iter()
            .map(|s| uuid::Uuid::parse_str(s))
            .collect();

        let memory_ids = match memory_ids {
            Ok(ids) => ids,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "invalid UUID: {e}"
                ))]));
            }
        };

        let strategy = input.strategy.map(|s| match s.as_str() {
            "hard_delete" => ForgetStrategy::HardDelete,
            "decay" => ForgetStrategy::Decay,
            "consolidate" => ForgetStrategy::Consolidate,
            "archive" => ForgetStrategy::Archive,
            _ => ForgetStrategy::SoftDelete,
        });

        let criteria = input.criteria.map(|c| {
            mnemo_core::query::forget::ForgetCriteria {
                max_age_hours: c.max_age_hours,
                min_importance_below: c.min_importance_below,
                memory_type: c.memory_type.and_then(|s| s.parse().ok()),
                tags: c.tags,
            }
        });

        let request = ForgetRequest {
            memory_ids,
            agent_id: None,
            strategy,
            criteria,
        };

        match self.engine.forget(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "forgotten": response.forgotten.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                    "errors": response.errors,
                    "status": "forgotten"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.share",
        description = "Share one or more memories with another agent by granting them access permissions. Supports batch sharing via memory_ids. The memory scope will be updated to 'shared' automatically."
    )]
    async fn share(
        &self,
        Parameters(input): Parameters<ShareInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();

        // Support batch: memory_ids takes precedence over memory_id
        let id_strings = input.memory_ids.unwrap_or_else(|| vec![input.memory_id.clone()]);

        let permission = input.permission.and_then(|s| s.parse().ok());

        let mut all_acl_ids: Vec<String> = Vec::new();
        let mut all_shared_with: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for id_str in &id_strings {
            let memory_id = match uuid::Uuid::parse_str(id_str) {
                Ok(id) => id,
                Err(e) => {
                    errors.push(format!("invalid UUID '{id_str}': {e}"));
                    continue;
                }
            };

            let request = ShareRequest {
                memory_id,
                agent_id: None,
                target_agent_id: input.target_agent_id.clone(),
                target_agent_ids: input.target_agent_ids.clone(),
                permission,
                expires_in_hours: input.expires_in_hours,
            };

            match self.engine.share(request).await {
                Ok(response) => {
                    for acl_id in &response.acl_ids {
                        all_acl_ids.push(acl_id.to_string());
                    }
                    if all_shared_with.is_empty() {
                        all_shared_with = response.shared_with_all;
                    }
                }
                Err(e) => {
                    errors.push(format!("share {id_str}: {e}"));
                }
            }
        }

        let result = serde_json::json!({
            "acl_ids": all_acl_ids,
            "memory_ids": id_strings,
            "shared_with": all_shared_with,
            "errors": errors,
            "status": "shared"
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap(),
        )]))
    }

    #[tool(
        name = "mnemo.checkpoint",
        description = "Create a checkpoint to snapshot the current agent state. Checkpoints capture the state, active memories, and event cursor at a point in time, enabling git-like state management."
    )]
    async fn checkpoint(
        &self,
        Parameters(input): Parameters<CheckpointInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let request = CheckpointRequest {
            thread_id: input.thread_id,
            agent_id: None,
            branch_name: input.branch_name,
            state_snapshot: input.state_snapshot,
            label: input.label,
            metadata: input.metadata,
        };

        match self.engine.checkpoint(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "checkpoint_id": response.id.to_string(),
                    "parent_id": response.parent_id.map(|id| id.to_string()),
                    "branch_name": response.branch_name,
                    "status": "checkpointed"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.branch",
        description = "Fork the current state into a new branch for exploration. Creates a new branch from an existing checkpoint, copying the state snapshot and memory references."
    )]
    async fn branch(
        &self,
        Parameters(input): Parameters<BranchInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let source_checkpoint_id = input.source_checkpoint_id
            .and_then(|s| uuid::Uuid::parse_str(&s).ok());

        let request = BranchRequest {
            thread_id: input.thread_id,
            agent_id: None,
            new_branch_name: input.new_branch_name,
            source_checkpoint_id,
            source_branch: input.source_branch,
        };

        match self.engine.branch(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "checkpoint_id": response.checkpoint_id.to_string(),
                    "branch_name": response.branch_name,
                    "source_checkpoint_id": response.source_checkpoint_id.to_string(),
                    "status": "branched"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.merge",
        description = "Merge a branch back into another branch. Supports full merge (all memories), cherry-pick (specific memories), and squash strategies."
    )]
    async fn merge(
        &self,
        Parameters(input): Parameters<MergeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let strategy = input.strategy.as_deref().map(|s| match s {
            "cherry_pick" => MergeStrategy::CherryPick,
            "squash" => MergeStrategy::Squash,
            _ => MergeStrategy::FullMerge,
        });

        let cherry_pick_ids = input.cherry_pick_ids.map(|ids| {
            ids.iter()
                .filter_map(|s| uuid::Uuid::parse_str(s).ok())
                .collect()
        });

        let request = MergeRequest {
            thread_id: input.thread_id,
            agent_id: None,
            source_branch: input.source_branch,
            target_branch: input.target_branch,
            strategy,
            cherry_pick_ids,
        };

        match self.engine.merge(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "checkpoint_id": response.checkpoint_id.to_string(),
                    "target_branch": response.target_branch,
                    "merged_memory_count": response.merged_memory_count,
                    "status": "merged"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.replay",
        description = "Reconstruct the agent context at a specific checkpoint. Returns the checkpoint state, referenced memories, and events up to that point."
    )]
    async fn replay(
        &self,
        Parameters(input): Parameters<ReplayInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let checkpoint_id = input.checkpoint_id
            .and_then(|s| uuid::Uuid::parse_str(&s).ok());

        let request = ReplayRequest {
            thread_id: input.thread_id,
            agent_id: None,
            checkpoint_id,
            branch_name: input.branch_name,
        };

        match self.engine.replay(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "checkpoint": {
                        "id": response.checkpoint.id.to_string(),
                        "branch_name": response.checkpoint.branch_name,
                        "state_snapshot": response.checkpoint.state_snapshot,
                        "label": response.checkpoint.label,
                        "created_at": response.checkpoint.created_at,
                    },
                    "memory_count": response.memories.len(),
                    "event_count": response.events.len(),
                    "memories": response.memories.iter().map(|m| {
                        serde_json::json!({
                            "id": m.id.to_string(),
                            "content": m.content,
                            "memory_type": m.memory_type.to_string(),
                            "created_at": m.created_at,
                        })
                    }).collect::<Vec<_>>(),
                    "status": "replayed"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.delegate",
        description = "Delegate permissions to another agent. Allows granting scoped, time-bounded access to your memories with optional re-delegation depth limits."
    )]
    async fn delegate(
        &self,
        Parameters(input): Parameters<DelegateInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        use mnemo_core::model::acl::Permission;
        use mnemo_core::model::delegation::{Delegation, DelegationScope};

        let permission = match input.permission.parse::<Permission>() {
            Ok(p) => p,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        };

        let scope = if let Some(ref ids) = input.memory_ids {
            let parsed: Result<Vec<uuid::Uuid>, _> = ids.iter().map(|s| uuid::Uuid::parse_str(s)).collect();
            match parsed {
                Ok(uuids) => DelegationScope::ByMemoryId(uuids),
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!("invalid UUID: {e}"))])),
            }
        } else if let Some(ref tags) = input.tags {
            DelegationScope::ByTag(tags.clone())
        } else {
            DelegationScope::AllMemories
        };

        let now = chrono::Utc::now();
        let expires_at = input.expires_in_hours.map(|h| {
            (now + chrono::Duration::seconds((h * 3600.0) as i64)).to_rfc3339()
        });

        let delegation = Delegation {
            id: uuid::Uuid::now_v7(),
            delegator_id: self.engine.default_agent_id.clone(),
            delegate_id: input.delegate_id.clone(),
            permission,
            scope,
            max_depth: input.max_depth.unwrap_or(0),
            current_depth: 0,
            parent_delegation_id: None,
            created_at: now.to_rfc3339(),
            expires_at,
            revoked_at: None,
        };

        match self.engine.storage.insert_delegation(&delegation).await {
            Ok(()) => {
                let result = serde_json::json!({
                    "delegation_id": delegation.id.to_string(),
                    "delegator": delegation.delegator_id,
                    "delegate": delegation.delegate_id,
                    "permission": delegation.permission.to_string(),
                    "status": "delegated"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.verify",
        description = "Verify the hash chain integrity of stored memories. Detects tampered or corrupted records by validating content hashes and chain linkage."
    )]
    async fn verify(
        &self,
        Parameters(input): Parameters<VerifyInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        match self.engine.verify_integrity(input.agent_id, input.thread_id.as_deref()).await {
            Ok(result) => {
                let response = serde_json::json!({
                    "valid": result.valid,
                    "total_records": result.total_records,
                    "verified_records": result.verified_records,
                    "first_broken_at": result.first_broken_at.map(|id| id.to_string()),
                    "error_message": result.error_message,
                    "status": if result.valid { "verified" } else { "integrity_violation" }
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

fn parse_source_type(s: &str) -> Option<SourceType> {
    match s {
        "agent" => Some(SourceType::Agent),
        "human" => Some(SourceType::Human),
        "system" => Some(SourceType::System),
        "user_input" => Some(SourceType::UserInput),
        "tool_output" => Some(SourceType::ToolOutput),
        "model_response" => Some(SourceType::ModelResponse),
        "retrieval" => Some(SourceType::Retrieval),
        "consolidation" => Some(SourceType::Consolidation),
        "import" => Some(SourceType::Import),
        _ => None,
    }
}

#[tool_handler]
impl ServerHandler for MnemoServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Mnemo is an MCP-native memory database for AI agents. \
                 Use mnemo.remember to store memories, mnemo.recall to search them, \
                 mnemo.forget to delete them, mnemo.share to share with other agents, \
                 mnemo.checkpoint to snapshot state, mnemo.branch to fork for exploration, \
                 mnemo.merge to combine branches, mnemo.replay to reconstruct context, \
                 mnemo.verify to check hash chain integrity, \
                 and mnemo.delegate to grant scoped permissions to other agents."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "mnemo".into(),
                title: None,
                version: env!("CARGO_PKG_VERSION").into(),
                icons: None,
                website_url: None,
            },
            ..Default::default()
        }
    }
}
