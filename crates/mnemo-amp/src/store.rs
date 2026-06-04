//! `MemoryStore`-conformant surface over a [`MnemoEngine`].
//!
//! Maps each of the 5 AMP ops onto **real** engine calls. Two of them
//! have no 1:1 engine primitive and are implemented as thin
//! compositions, deliberately:
//!
//! - **`merge`** — mnemo's `MnemoEngine::merge` is a *branch-timeline*
//!   merge (checkpoint thread/branch), not a memory-record merge. AMP
//!   `merge` folds N memory records into one, so this adapter composes
//!   `storage.get_memory` → `engine.remember` (the consolidated
//!   record, `SourceType::Consolidation`) → `engine.forget`
//!   (`ForgetStrategy::Consolidate` on the originals). No fictitious
//!   engine method is assumed.
//! - **`expire`** — there is no `engine.expire`; the lifecycle path is
//!   `expires_at` + `run_ttl_sweep`. AMP `expire` sets `expires_at` on
//!   the targets (immediately in the past when `ttl_seconds` is unset
//!   or `0`, else `now + ttl`) via `storage.update_memory`, then runs
//!   `run_ttl_sweep` for the immediate case so the records are
//!   hard-deleted and a `MemoryExpired` audit event is emitted.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use mnemo_core::model::event::EventType;
use mnemo_core::model::memory::{MemoryType, SourceType};
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy};
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;

use crate::approval::{Approval, ApprovalHook, AutoApprove, WriteDiff};
use crate::error::AmpError;
use crate::wire::{AmpEnvelope, AmpHit, AmpMemoryType, AmpOp, AmpResult};

/// Default recall depth (the conformance suite's recall@5).
pub const DEFAULT_TOP_K: usize = 5;

/// The AMP `MemoryStore` surface: 5 ops, each returning an
/// [`AmpResult`]. Transport adapters (REST / MCP / a fan-out router)
/// call these.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn remember(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError>;
    async fn recall(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError>;
    async fn forget(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError>;
    async fn merge(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError>;
    async fn expire(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError>;

    /// Dispatch an envelope to the matching op.
    async fn dispatch(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError> {
        match env.op {
            AmpOp::Remember => self.remember(env).await,
            AmpOp::Recall => self.recall(env).await,
            AmpOp::Forget => self.forget(env).await,
            AmpOp::Merge => self.merge(env).await,
            AmpOp::Expire => self.expire(env).await,
        }
    }
}

/// AMP store backed by a shared [`MnemoEngine`], with an optional HITL
/// approval gate on long-term writes.
pub struct MnemoAmpStore {
    engine: Arc<MnemoEngine>,
    approval: Arc<dyn ApprovalHook>,
}

impl MnemoAmpStore {
    /// Build a store with the default [`AutoApprove`] gate (no human in
    /// the loop).
    pub fn new(engine: Arc<MnemoEngine>) -> Self {
        Self {
            engine,
            approval: Arc::new(AutoApprove),
        }
    }

    /// Attach a HITL diff-and-approve hook consulted before every
    /// long-term (`semantic` / `procedural`) write.
    pub fn with_approval_hook(mut self, hook: Arc<dyn ApprovalHook>) -> Self {
        self.approval = hook;
        self
    }

    fn agent<'a>(&'a self, env: &'a AmpEnvelope) -> &'a str {
        env.agent_id
            .as_deref()
            .unwrap_or(&self.engine.default_agent_id)
    }

    /// Run the approval gate for a long-term write and, on approval,
    /// emit a `Decision` audit event through the existing hash chain.
    /// Short-term tiers approve implicitly without an event.
    async fn gate_long_term_write(
        &self,
        agent_id: &str,
        diff: WriteDiff,
    ) -> Result<bool, AmpError> {
        if !diff.memory_type.is_long_term() {
            return Ok(true);
        }
        match self.approval.review(&diff) {
            Approval::Approve => {
                let rendered = diff.render();
                let event = mnemo_core::query::event_builder::build_event(
                    &self.engine,
                    agent_id,
                    EventType::Decision,
                    serde_json::json!({
                        "amp_approval": "approved",
                        "hook": self.approval.name(),
                        "memory_type": diff.memory_type.as_str(),
                        "diff": rendered,
                    }),
                    &rendered,
                    None,
                )
                .await;
                if let Err(e) = self.engine.storage.insert_event(&event).await {
                    tracing::warn!(error = %e, "failed to insert AMP approval audit event");
                }
                Ok(true)
            }
            Approval::Reject(_) => Ok(false),
        }
    }
}

fn to_core_type(t: AmpMemoryType) -> MemoryType {
    match t {
        AmpMemoryType::Episodic => MemoryType::Episodic,
        AmpMemoryType::Semantic => MemoryType::Semantic,
        AmpMemoryType::Procedural => MemoryType::Procedural,
        AmpMemoryType::Working => MemoryType::Working,
    }
}

fn from_core_type(t: MemoryType) -> AmpMemoryType {
    match t {
        MemoryType::Episodic => AmpMemoryType::Episodic,
        MemoryType::Semantic => AmpMemoryType::Semantic,
        MemoryType::Procedural => AmpMemoryType::Procedural,
        MemoryType::Working => AmpMemoryType::Working,
    }
}

fn parse_ids(env: &AmpEnvelope) -> Result<Vec<Uuid>, AmpError> {
    env.memory_ids
        .iter()
        .map(|s| {
            Uuid::parse_str(s).map_err(|_| AmpError::Validation(format!("invalid memory id '{s}'")))
        })
        .collect()
}

#[async_trait]
impl MemoryStore for MnemoAmpStore {
    async fn remember(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError> {
        let content = env
            .content
            .clone()
            .ok_or_else(|| AmpError::Validation("remember requires `content`".into()))?;
        let agent_id = self.agent(env).to_string();

        // HITL gate on long-term writes BEFORE the record commits.
        let diff = WriteDiff {
            agent_id: agent_id.clone(),
            memory_type: env.memory_type,
            before: None,
            after: content.clone(),
            tags: env.tags.clone(),
        };
        let long_term = env.memory_type.is_long_term();
        let approved = self.gate_long_term_write(&agent_id, diff).await?;
        if !approved {
            return Ok(AmpResult::rejected(
                AmpOp::Remember,
                "long-term write rejected by approval hook",
            ));
        }

        let mut req = RememberRequest::new(content);
        req.agent_id = Some(agent_id);
        req.memory_type = Some(to_core_type(env.memory_type));
        if !env.tags.is_empty() {
            req.tags = Some(env.tags.clone());
        }
        req.metadata = env.metadata.clone();
        if let Some(ttl) = env.ttl_seconds {
            req.ttl_seconds = Some(ttl);
        }

        let resp = self.engine.remember(req).await?;
        let mut out = AmpResult::ok(AmpOp::Remember);
        out.ids = vec![resp.id.to_string()];
        if long_term {
            out.approved = Some(true);
        }
        Ok(out)
    }

    async fn recall(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError> {
        let query = env
            .query
            .clone()
            .ok_or_else(|| AmpError::Validation("recall requires `query`".into()))?;
        let mut req = RecallRequest::new(query);
        req.agent_id = Some(self.agent(env).to_string());
        req.limit = Some(env.top_k.unwrap_or(DEFAULT_TOP_K));
        req.memory_type = Some(to_core_type(env.memory_type));
        if !env.tags.is_empty() {
            req.tags = Some(env.tags.clone());
        }
        req.strategy = Some("auto".to_string());

        let resp = self.engine.recall(req).await?;
        let mut out = AmpResult::ok(AmpOp::Recall);
        out.hits = resp
            .memories
            .into_iter()
            .map(|m| AmpHit {
                id: m.id.to_string(),
                content: m.content,
                memory_type: from_core_type(m.memory_type),
                score: m.score,
                tags: m.tags,
            })
            .collect();
        Ok(out)
    }

    async fn forget(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError> {
        let ids = parse_ids(env)?;
        if ids.is_empty() {
            return Err(AmpError::Validation("forget requires `memory_ids`".into()));
        }
        let mut req = ForgetRequest::new(ids);
        req.agent_id = Some(self.agent(env).to_string());
        req.strategy = Some(ForgetStrategy::SoftDelete);
        let resp = self.engine.forget(req).await?;
        let mut out = AmpResult::ok(AmpOp::Forget);
        out.ids = resp.forgotten.iter().map(|id| id.to_string()).collect();
        if !resp.errors.is_empty() {
            out.detail = format!("{} id(s) failed", resp.errors.len());
        }
        Ok(out)
    }

    async fn merge(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError> {
        // Thin composition over remember + forget — NOT engine.merge
        // (which is a branch-timeline merge). Fold the source records'
        // content into one consolidated record, then retire the
        // originals with the Consolidate strategy so the chain stays
        // auditable.
        let ids = parse_ids(env)?;
        if ids.len() < 2 {
            return Err(AmpError::Validation(
                "merge requires at least two `memory_ids`".into(),
            ));
        }
        let agent_id = self.agent(env).to_string();

        let mut contents = Vec::with_capacity(ids.len());
        let mut tag_set: Vec<String> = env.tags.clone();
        for id in &ids {
            let rec = self
                .engine
                .storage
                .get_memory(*id)
                .await?
                .ok_or_else(|| AmpError::NotFound(format!("memory {id} not found")))?;
            contents.push(rec.content);
            for t in rec.tags {
                if !tag_set.contains(&t) {
                    tag_set.push(t);
                }
            }
        }
        let merged_content = format!("[AMP merged from {}] {}", ids.len(), contents.join(" | "));

        // HITL gate on the long-term merged write.
        let diff = WriteDiff {
            agent_id: agent_id.clone(),
            memory_type: env.memory_type,
            before: Some(contents_preview(&contents)),
            after: merged_content.clone(),
            tags: tag_set.clone(),
        };
        let approved = self.gate_long_term_write(&agent_id, diff).await?;
        if !approved {
            return Ok(AmpResult::rejected(
                AmpOp::Merge,
                "long-term merge rejected by approval hook",
            ));
        }

        let mut req = RememberRequest::new(merged_content);
        req.agent_id = Some(agent_id.clone());
        req.memory_type = Some(to_core_type(env.memory_type));
        req.source_type = Some(SourceType::Consolidation);
        if !tag_set.is_empty() {
            req.tags = Some(tag_set);
        }
        req.metadata = Some(serde_json::json!({
            "amp_merged_from": ids.iter().map(|i| i.to_string()).collect::<Vec<_>>(),
        }));
        let merged = self.engine.remember(req).await?;

        // Retire the originals.
        let mut forget_req = ForgetRequest::new(ids);
        forget_req.agent_id = Some(agent_id);
        forget_req.strategy = Some(ForgetStrategy::Consolidate);
        let forgotten = self.engine.forget(forget_req).await?;

        let mut out = AmpResult::ok(AmpOp::Merge);
        out.ids = vec![merged.id.to_string()];
        if env.memory_type.is_long_term() {
            out.approved = Some(true);
        }
        if !forgotten.errors.is_empty() {
            out.detail = format!("{} original(s) failed to retire", forgotten.errors.len());
        }
        Ok(out)
    }

    async fn expire(&self, env: &AmpEnvelope) -> Result<AmpResult, AmpError> {
        // Thin composition over expires_at + run_ttl_sweep — there is
        // no engine.expire primitive. Immediate when ttl is unset/0.
        let ids = parse_ids(env)?;
        if ids.is_empty() {
            return Err(AmpError::Validation("expire requires `memory_ids`".into()));
        }
        let ttl = env.ttl_seconds.unwrap_or(0);
        let immediate = ttl == 0;
        let now = chrono::Utc::now();
        let expires_at = if immediate {
            now - chrono::Duration::seconds(1)
        } else {
            now + chrono::Duration::seconds(ttl as i64)
        };
        let expires_str = expires_at.to_rfc3339();

        let mut touched = Vec::new();
        for id in &ids {
            match self.engine.storage.get_memory(*id).await? {
                Some(mut rec) => {
                    rec.expires_at = Some(expires_str.clone());
                    rec.updated_at = now.to_rfc3339();
                    self.engine.storage.update_memory(&rec).await?;
                    touched.push(id.to_string());
                }
                None => {
                    return Err(AmpError::NotFound(format!("memory {id} not found")));
                }
            }
        }

        if immediate {
            // Hard-delete the now-expired records + emit MemoryExpired
            // audit events through the existing lifecycle path.
            self.engine.run_ttl_sweep().await?;
        }

        let mut out = AmpResult::ok(AmpOp::Expire);
        out.ids = touched;
        if !immediate {
            out.detail = format!("scheduled to expire at {expires_str}");
        }
        Ok(out)
    }
}

/// A bounded, deterministic preview of the source contents folded by a
/// merge, for the approval diff.
fn contents_preview(contents: &[String]) -> String {
    contents
        .iter()
        .map(|c| {
            if c.len() > 80 {
                format!("{}…", &c[..80])
            } else {
                c.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}
