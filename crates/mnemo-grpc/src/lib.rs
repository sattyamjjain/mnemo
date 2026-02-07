//! gRPC API server for Mnemo.
//!
//! This crate exposes Mnemo's core memory operations (remember, recall, forget)
//! over gRPC using [`tonic`]. The protobuf service definition lives in
//! `proto/mnemo.proto` and code is generated at build time via `tonic-build`.
//!
//! # Usage
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use mnemo_grpc::router;
//!
//! let engine: Arc<mnemo_core::query::MnemoEngine> = /* ... */;
//! let grpc_router = router(engine);
//! ```

use std::sync::Arc;

use tonic::{Request, Response, Status};
use uuid::Uuid;

use mnemo_core::model::acl::Permission;
use mnemo_core::model::delegation::{Delegation, DelegationScope};
use mnemo_core::model::memory::{MemoryType, Scope, SourceType};
use mnemo_core::query::branch::BranchRequest as CoreBranchRequest;
use mnemo_core::query::checkpoint::CheckpointRequest as CoreCheckpointRequest;
use mnemo_core::query::forget::{
    ForgetRequest as CoreForgetRequest, ForgetStrategy,
};
use mnemo_core::query::merge::{MergeRequest as CoreMergeRequest, MergeStrategy};
use mnemo_core::query::recall::RecallRequest as CoreRecallRequest;
use mnemo_core::query::remember::RememberRequest as CoreRememberRequest;
use mnemo_core::query::replay::ReplayRequest as CoreReplayRequest;
use mnemo_core::query::share::ShareRequest as CoreShareRequest;
use mnemo_core::query::MnemoEngine;

// ---------------------------------------------------------------------------
// Generated protobuf code
// ---------------------------------------------------------------------------

pub mod proto {
    tonic::include_proto!("mnemo.v1");
}

use proto::mnemo_service_server::{MnemoService, MnemoServiceServer};
use proto::{
    BranchRequest as ProtoBranchRequest, BranchResponse as ProtoBranchResponse,
    CheckpointRequest as ProtoCheckpointRequest, CheckpointResponse as ProtoCheckpointResponse,
    DelegateRequest as ProtoDelegateRequest, DelegateResponse as ProtoDelegateResponse,
    ForgetError as ProtoForgetError, ForgetRequest as ProtoForgetRequest,
    ForgetResponse as ProtoForgetResponse, HealthRequest, HealthResponse,
    MergeRequest as ProtoMergeRequest, MergeResponse as ProtoMergeResponse,
    RecallRequest as ProtoRecallRequest, RecallResponse as ProtoRecallResponse,
    RememberRequest as ProtoRememberRequest,
    RememberResponse as ProtoRememberResponse,
    ReplayMemory as ProtoReplayMemory,
    ReplayRequest as ProtoReplayRequest, ReplayResponse as ProtoReplayResponse,
    ScoredMemory as ProtoScoredMemory,
    ShareRequest as ProtoShareRequest, ShareResponse as ProtoShareResponse,
    VerifyRequest as ProtoVerifyRequest, VerifyResponse as ProtoVerifyResponse,
};

// ---------------------------------------------------------------------------
// Server implementation
// ---------------------------------------------------------------------------

/// gRPC server backed by a shared [`MnemoEngine`].
#[derive(Clone)]
pub struct MnemoGrpcServer {
    engine: Arc<MnemoEngine>,
}

impl MnemoGrpcServer {
    /// Create a new server wrapping the given engine.
    pub fn new(engine: Arc<MnemoEngine>) -> Self {
        Self { engine }
    }
}

#[tonic::async_trait]
impl MnemoService for MnemoGrpcServer {
    // -- Remember ----------------------------------------------------------

    async fn remember(
        &self,
        request: Request<ProtoRememberRequest>,
    ) -> Result<Response<ProtoRememberResponse>, Status> {
        let req = request.into_inner();

        let memory_type = match req.memory_type {
            Some(ref s) => match s.parse::<MemoryType>() {
                Ok(mt) => Some(mt),
                Err(_) => return Err(Status::invalid_argument(format!(
                    "invalid memory_type '{}': expected one of: episodic, semantic, procedural, working", s
                ))),
            },
            None => None,
        };

        let scope = match req.scope {
            Some(ref s) => match s.parse::<Scope>() {
                Ok(sc) => Some(sc),
                Err(_) => return Err(Status::invalid_argument(format!(
                    "invalid scope '{}': expected one of: private, shared, public, global", s
                ))),
            },
            None => None,
        };

        let source_type = match req.source_type {
            Some(ref s) => match s.parse::<SourceType>() {
                Ok(st) => Some(st),
                Err(_) => return Err(Status::invalid_argument(format!(
                    "invalid source_type '{}': expected one of: agent, human, system, user_input, tool_output, model_response, retrieval, consolidation, import", s
                ))),
            },
            None => None,
        };

        let metadata: Option<serde_json::Value> = match req.metadata {
            Some(ref s) => match serde_json::from_str(s) {
                Ok(v) => Some(v),
                Err(e) => return Err(Status::invalid_argument(format!(
                    "invalid metadata JSON: {}", e
                ))),
            },
            None => None,
        };

        let tags = if req.tags.is_empty() {
            None
        } else {
            Some(req.tags)
        };

        let related_to = if req.related_to.is_empty() {
            None
        } else {
            Some(req.related_to)
        };

        let core_req = CoreRememberRequest {
            content: req.content,
            agent_id: req.agent_id,
            memory_type,
            scope,
            importance: req.importance,
            tags,
            metadata,
            source_type,
            source_id: req.source_id,
            org_id: req.org_id,
            thread_id: req.thread_id,
            ttl_seconds: req.ttl_seconds,
            related_to,
            decay_rate: req.decay_rate,
            created_by: req.created_by,
        };

        let result = self
            .engine
            .remember(core_req)
            .await
            .map_err(core_error_to_status)?;

        Ok(Response::new(ProtoRememberResponse {
            id: result.id.to_string(),
            content_hash: result.content_hash,
        }))
    }

    // -- Recall ------------------------------------------------------------

    async fn recall(
        &self,
        request: Request<ProtoRecallRequest>,
    ) -> Result<Response<ProtoRecallResponse>, Status> {
        let req = request.into_inner();

        let memory_type = match req.memory_type {
            Some(ref s) => match s.parse::<MemoryType>() {
                Ok(mt) => Some(mt),
                Err(_) => return Err(Status::invalid_argument(format!(
                    "invalid memory_type '{}': expected one of: episodic, semantic, procedural, working", s
                ))),
            },
            None => None,
        };

        let scope = match req.scope {
            Some(ref s) => match s.parse::<Scope>() {
                Ok(sc) => Some(sc),
                Err(_) => return Err(Status::invalid_argument(format!(
                    "invalid scope '{}': expected one of: private, shared, public, global", s
                ))),
            },
            None => None,
        };

        let tags = if req.tags.is_empty() {
            None
        } else {
            Some(req.tags)
        };

        let hybrid_weights = if req.hybrid_weights.is_empty() {
            None
        } else {
            Some(req.hybrid_weights)
        };

        let core_req = CoreRecallRequest {
            query: req.query,
            agent_id: req.agent_id,
            limit: req.limit.map(|l| l as usize),
            memory_type,
            memory_types: None,
            scope,
            min_importance: req.min_importance,
            tags,
            org_id: req.org_id,
            strategy: req.strategy,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights,
            rrf_k: req.rrf_k,
            as_of: req.as_of,
        };

        let result = self
            .engine
            .recall(core_req)
            .await
            .map_err(core_error_to_status)?;

        let memories: Vec<ProtoScoredMemory> = result
            .memories
            .into_iter()
            .map(|m| ProtoScoredMemory {
                id: m.id.to_string(),
                content: m.content,
                memory_type: format!("{:?}", m.memory_type),
                importance: m.importance,
                score: m.score,
                created_at: m.created_at,
                agent_id: m.agent_id,
                scope: format!("{:?}", m.scope),
                tags: m.tags,
                metadata: m.metadata.to_string(),
                access_count: m.access_count,
                updated_at: m.updated_at,
            })
            .collect();

        let total = result.total as u32;

        Ok(Response::new(ProtoRecallResponse { memories, total }))
    }

    // -- Forget ------------------------------------------------------------

    async fn forget(
        &self,
        request: Request<ProtoForgetRequest>,
    ) -> Result<Response<ProtoForgetResponse>, Status> {
        let req = request.into_inner();

        let memory_ids: Vec<Uuid> = req
            .memory_ids
            .iter()
            .map(|s| {
                Uuid::parse_str(s)
                    .map_err(|e| Status::invalid_argument(format!("invalid UUID '{s}': {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let strategy = match req.strategy {
            Some(ref s) => {
                let st = match s.as_str() {
                    "soft_delete" => ForgetStrategy::SoftDelete,
                    "hard_delete" => ForgetStrategy::HardDelete,
                    "decay" => ForgetStrategy::Decay,
                    "consolidate" => ForgetStrategy::Consolidate,
                    "archive" => ForgetStrategy::Archive,
                    _ => return Err(Status::invalid_argument(format!(
                        "invalid forget strategy '{}': expected one of: soft_delete, hard_delete, decay, consolidate, archive", s
                    ))),
                };
                Some(st)
            }
            None => None,
        };

        let core_req = CoreForgetRequest {
            memory_ids,
            agent_id: req.agent_id,
            strategy,
            criteria: None,
        };

        let result = self
            .engine
            .forget(core_req)
            .await
            .map_err(core_error_to_status)?;

        let forgotten: Vec<String> = result.forgotten.iter().map(|id| id.to_string()).collect();

        let errors: Vec<ProtoForgetError> = result
            .errors
            .into_iter()
            .map(|e| ProtoForgetError {
                id: e.id.to_string(),
                error: e.error,
            })
            .collect();

        Ok(Response::new(ProtoForgetResponse { forgotten, errors }))
    }

    // -- Health ------------------------------------------------------------

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    }

    // -- Share -------------------------------------------------------------

    async fn share(
        &self,
        request: Request<ProtoShareRequest>,
    ) -> Result<Response<ProtoShareResponse>, Status> {
        let req = request.into_inner();
        let memory_id = Uuid::parse_str(&req.memory_id)
            .map_err(|e| Status::invalid_argument(format!("invalid UUID: {e}")))?;
        let permission = match req.permission {
            Some(ref s) => match s.parse::<Permission>() {
                Ok(p) => Some(p),
                Err(_) => return Err(Status::invalid_argument(format!(
                    "invalid permission '{}': expected one of: read, write, delete, share, delegate, admin", s
                ))),
            },
            None => None,
        };
        let target_agent_ids = if req.target_agent_ids.is_empty() {
            None
        } else {
            Some(req.target_agent_ids)
        };

        let core_req = CoreShareRequest {
            memory_id,
            agent_id: req.agent_id,
            target_agent_id: req.target_agent_id,
            target_agent_ids,
            permission,
            expires_in_hours: req.expires_in_hours,
        };
        let result = self
            .engine
            .share(core_req)
            .await
            .map_err(core_error_to_status)?;

        Ok(Response::new(ProtoShareResponse {
            acl_id: result.acl_id.to_string(),
            acl_ids: result.acl_ids.iter().map(|id| id.to_string()).collect(),
            memory_id: result.memory_id.to_string(),
            shared_with: result.shared_with,
            shared_with_all: result.shared_with_all,
            permission: result.permission.to_string(),
        }))
    }

    // -- Checkpoint --------------------------------------------------------

    async fn checkpoint(
        &self,
        request: Request<ProtoCheckpointRequest>,
    ) -> Result<Response<ProtoCheckpointResponse>, Status> {
        let req = request.into_inner();
        let state_snapshot: serde_json::Value = serde_json::from_str(&req.state_snapshot)
            .map_err(|e| {
                Status::invalid_argument(format!("invalid JSON state_snapshot: {e}"))
            })?;
        let metadata: Option<serde_json::Value> = match req.metadata {
            Some(ref s) => match serde_json::from_str(s) {
                Ok(v) => Some(v),
                Err(e) => return Err(Status::invalid_argument(format!(
                    "invalid metadata JSON: {}", e
                ))),
            },
            None => None,
        };

        let core_req = CoreCheckpointRequest {
            thread_id: req.thread_id,
            agent_id: req.agent_id,
            branch_name: req.branch_name,
            state_snapshot,
            label: req.label,
            metadata,
        };
        let result = self
            .engine
            .checkpoint(core_req)
            .await
            .map_err(core_error_to_status)?;

        Ok(Response::new(ProtoCheckpointResponse {
            checkpoint_id: result.id.to_string(),
            parent_id: result.parent_id.map(|id| id.to_string()),
            branch_name: result.branch_name,
        }))
    }

    // -- Branch ------------------------------------------------------------

    async fn branch(
        &self,
        request: Request<ProtoBranchRequest>,
    ) -> Result<Response<ProtoBranchResponse>, Status> {
        let req = request.into_inner();
        let source_checkpoint_id = match req.source_checkpoint_id {
            Some(ref s) => match Uuid::parse_str(s) {
                Ok(id) => Some(id),
                Err(e) => return Err(Status::invalid_argument(format!(
                    "invalid source_checkpoint_id '{}': {}", s, e
                ))),
            },
            None => None,
        };

        let core_req = CoreBranchRequest {
            thread_id: req.thread_id,
            agent_id: req.agent_id,
            new_branch_name: req.new_branch_name,
            source_checkpoint_id,
            source_branch: req.source_branch,
        };
        let result = self
            .engine
            .branch(core_req)
            .await
            .map_err(core_error_to_status)?;

        Ok(Response::new(ProtoBranchResponse {
            checkpoint_id: result.checkpoint_id.to_string(),
            branch_name: result.branch_name,
            source_checkpoint_id: result.source_checkpoint_id.to_string(),
        }))
    }

    // -- Merge -------------------------------------------------------------

    async fn merge(
        &self,
        request: Request<ProtoMergeRequest>,
    ) -> Result<Response<ProtoMergeResponse>, Status> {
        let req = request.into_inner();
        let strategy = match req.strategy {
            Some(ref s) => {
                let st = match s.as_str() {
                    "full_merge" => MergeStrategy::FullMerge,
                    "cherry_pick" => MergeStrategy::CherryPick,
                    "squash" => MergeStrategy::Squash,
                    _ => return Err(Status::invalid_argument(format!(
                        "invalid merge strategy '{}': expected one of: full_merge, cherry_pick, squash", s
                    ))),
                };
                Some(st)
            }
            None => None,
        };
        let cherry_pick_ids = if req.cherry_pick_ids.is_empty() {
            None
        } else {
            let ids: Result<Vec<Uuid>, _> = req
                .cherry_pick_ids
                .iter()
                .map(|s| Uuid::parse_str(s))
                .collect();
            Some(
                ids.map_err(|e| {
                    Status::invalid_argument(format!("invalid UUID: {e}"))
                })?,
            )
        };

        let core_req = CoreMergeRequest {
            thread_id: req.thread_id,
            agent_id: req.agent_id,
            source_branch: req.source_branch,
            target_branch: req.target_branch,
            strategy,
            cherry_pick_ids,
        };
        let result = self
            .engine
            .merge(core_req)
            .await
            .map_err(core_error_to_status)?;

        Ok(Response::new(ProtoMergeResponse {
            checkpoint_id: result.checkpoint_id.to_string(),
            target_branch: result.target_branch,
            merged_memory_count: result.merged_memory_count as u32,
        }))
    }

    // -- Replay ------------------------------------------------------------

    async fn replay(
        &self,
        request: Request<ProtoReplayRequest>,
    ) -> Result<Response<ProtoReplayResponse>, Status> {
        let req = request.into_inner();
        let checkpoint_id = match req.checkpoint_id {
            Some(ref s) => match Uuid::parse_str(s) {
                Ok(id) => Some(id),
                Err(e) => return Err(Status::invalid_argument(format!(
                    "invalid checkpoint_id '{}': {}", s, e
                ))),
            },
            None => None,
        };

        let core_req = CoreReplayRequest {
            thread_id: req.thread_id,
            agent_id: req.agent_id,
            checkpoint_id,
            branch_name: req.branch_name,
        };
        let result = self
            .engine
            .replay(core_req)
            .await
            .map_err(core_error_to_status)?;

        let checkpoint_json = serde_json::to_string(&result.checkpoint)
            .unwrap_or_else(|_| "{}".to_string());
        let memories: Vec<ProtoReplayMemory> = result
            .memories
            .iter()
            .map(|m| ProtoReplayMemory {
                id: m.id.to_string(),
                content: m.content.clone(),
                memory_type: format!("{:?}", m.memory_type),
                created_at: m.created_at.clone(),
            })
            .collect();

        let (chain_valid, chain_total, chain_verified) =
            if let Some(ref cv) = result.chain_verification {
                (
                    Some(cv.valid),
                    Some(cv.total_records as u32),
                    Some(cv.verified_records as u32),
                )
            } else {
                (None, None, None)
            };

        Ok(Response::new(ProtoReplayResponse {
            checkpoint_json,
            memories,
            event_count: result.events.len() as u32,
            chain_valid,
            chain_total,
            chain_verified,
        }))
    }

    // -- Delegate ----------------------------------------------------------

    async fn delegate(
        &self,
        request: Request<ProtoDelegateRequest>,
    ) -> Result<Response<ProtoDelegateResponse>, Status> {
        let req = request.into_inner();
        let permission: Permission = req
            .permission
            .parse()
            .map_err(|e: mnemo_core::error::Error| {
                Status::invalid_argument(e.to_string())
            })?;

        let scope = if !req.memory_ids.is_empty() {
            let ids: Vec<Uuid> = req
                .memory_ids
                .iter()
                .map(|s| {
                    Uuid::parse_str(s).map_err(|e| {
                        Status::invalid_argument(format!("invalid UUID: {e}"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            DelegationScope::ByMemoryId(ids)
        } else if !req.tags.is_empty() {
            DelegationScope::ByTag(req.tags)
        } else {
            DelegationScope::AllMemories
        };

        let now = chrono::Utc::now();
        let expires_at = req.expires_in_hours.map(|h| {
            (now + chrono::Duration::seconds((h * 3600.0) as i64)).to_rfc3339()
        });

        let delegation = Delegation {
            id: Uuid::now_v7(),
            delegator_id: req.delegator_id,
            delegate_id: req.delegate_id,
            permission,
            scope,
            max_depth: req.max_depth.unwrap_or(0),
            current_depth: 0,
            parent_delegation_id: None,
            created_at: now.to_rfc3339(),
            expires_at,
            revoked_at: None,
        };

        self.engine
            .storage
            .insert_delegation(&delegation)
            .await
            .map_err(core_error_to_status)?;

        Ok(Response::new(ProtoDelegateResponse {
            delegation_id: delegation.id.to_string(),
        }))
    }

    // -- Verify ------------------------------------------------------------

    async fn verify(
        &self,
        request: Request<ProtoVerifyRequest>,
    ) -> Result<Response<ProtoVerifyResponse>, Status> {
        let req = request.into_inner();
        let result = self
            .engine
            .verify_integrity(req.agent_id, req.thread_id.as_deref())
            .await
            .map_err(core_error_to_status)?;

        Ok(Response::new(ProtoVerifyResponse {
            valid: result.valid,
            total_records: result.total_records as u32,
            verified_records: result.verified_records as u32,
            first_broken_at: result.first_broken_at.map(|id| id.to_string()),
            error_message: result.error_message,
        }))
    }
}

// ---------------------------------------------------------------------------
// Router constructor
// ---------------------------------------------------------------------------

/// Build a [`tonic::transport::server::Router`] serving the Mnemo gRPC API.
///
/// The returned router can be composed with other tonic services or served
/// directly via `tonic::transport::Server`.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
/// use mnemo_grpc::router;
///
/// let engine: Arc<mnemo_core::query::MnemoEngine> = /* ... */;
/// let grpc_router = router(engine);
/// tonic::transport::Server::builder()
///     .add_routes(grpc_router.into_service())
///     .serve("[::1]:50051".parse().unwrap())
///     .await
///     .unwrap();
/// ```
pub fn router(engine: Arc<MnemoEngine>) -> tonic::transport::server::Router {
    let svc = MnemoGrpcServer::new(engine);
    tonic::transport::Server::builder().add_service(MnemoServiceServer::new(svc))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a `mnemo_core::error::Error` to a tonic `Status`.
fn core_error_to_status(err: mnemo_core::error::Error) -> Status {
    use mnemo_core::error::Error;

    match err {
        Error::Validation(msg) => Status::invalid_argument(msg),
        Error::PermissionDenied(msg) => Status::permission_denied(msg),
        Error::NotFound(msg) => Status::not_found(msg),
        other => Status::internal(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_error_maps_correctly() {
        let validation =
            core_error_to_status(mnemo_core::error::Error::Validation("bad input".into()));
        assert_eq!(validation.code(), tonic::Code::InvalidArgument);

        let perm = core_error_to_status(mnemo_core::error::Error::PermissionDenied(
            "forbidden".into(),
        ));
        assert_eq!(perm.code(), tonic::Code::PermissionDenied);

        let not_found =
            core_error_to_status(mnemo_core::error::Error::NotFound("missing".into()));
        assert_eq!(not_found.code(), tonic::Code::NotFound);
    }
}
