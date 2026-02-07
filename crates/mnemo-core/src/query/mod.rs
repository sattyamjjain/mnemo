pub mod branch;
pub mod causality;
pub mod checkpoint;
pub mod conflict;
pub mod forget;
pub mod lifecycle;
pub mod merge;
pub mod poisoning;
pub mod recall;
pub mod remember;
pub mod replay;
pub mod retrieval;
pub mod share;

use std::sync::Arc;

use crate::cache::MemoryCache;
use crate::embedding::EmbeddingProvider;
use crate::encryption::ContentEncryption;
use crate::error::Result;
use crate::index::VectorIndex;
use crate::search::FullTextIndex;
use crate::storage::StorageBackend;
use crate::storage::cold::ColdStorage;

pub struct MnemoEngine {
    pub storage: Arc<dyn StorageBackend>,
    pub index: Arc<dyn VectorIndex>,
    pub embedding: Arc<dyn EmbeddingProvider>,
    pub full_text: Option<Arc<dyn FullTextIndex>>,
    pub default_agent_id: String,
    pub default_org_id: Option<String>,
    pub encryption: Option<Arc<ContentEncryption>>,
    pub cold_storage: Option<Arc<dyn ColdStorage>>,
    pub cache: Option<Arc<MemoryCache>>,
    pub embed_events: bool,
}

impl MnemoEngine {
    pub fn new(
        storage: Arc<dyn StorageBackend>,
        index: Arc<dyn VectorIndex>,
        embedding: Arc<dyn EmbeddingProvider>,
        default_agent_id: String,
        default_org_id: Option<String>,
    ) -> Self {
        Self {
            storage,
            index,
            embedding,
            full_text: None,
            default_agent_id,
            default_org_id,
            encryption: None,
            cold_storage: None,
            cache: None,
            embed_events: false,
        }
    }

    pub fn with_full_text(mut self, ft: Arc<dyn FullTextIndex>) -> Self {
        self.full_text = Some(ft);
        self
    }

    pub fn with_encryption(mut self, enc: Arc<ContentEncryption>) -> Self {
        self.encryption = Some(enc);
        self
    }

    pub fn with_cold_storage(mut self, cs: Arc<dyn ColdStorage>) -> Self {
        self.cold_storage = Some(cs);
        self
    }

    pub fn with_cache(mut self, c: Arc<MemoryCache>) -> Self {
        self.cache = Some(c);
        self
    }

    pub fn with_event_embeddings(mut self) -> Self {
        self.embed_events = true;
        self
    }

    pub async fn remember(&self, request: remember::RememberRequest) -> Result<remember::RememberResponse> {
        remember::execute(self, request).await
    }

    pub async fn recall(&self, request: recall::RecallRequest) -> Result<recall::RecallResponse> {
        recall::execute(self, request).await
    }

    pub async fn forget(&self, request: forget::ForgetRequest) -> Result<forget::ForgetResponse> {
        forget::execute(self, request).await
    }

    pub async fn share(&self, request: share::ShareRequest) -> Result<share::ShareResponse> {
        share::execute(self, request).await
    }

    pub async fn checkpoint(&self, request: checkpoint::CheckpointRequest) -> Result<checkpoint::CheckpointResponse> {
        checkpoint::execute(self, request).await
    }

    pub async fn branch(&self, request: branch::BranchRequest) -> Result<branch::BranchResponse> {
        branch::execute(self, request).await
    }

    pub async fn merge(&self, request: merge::MergeRequest) -> Result<merge::MergeResponse> {
        merge::execute(self, request).await
    }

    pub async fn replay(&self, request: replay::ReplayRequest) -> Result<replay::ReplayResponse> {
        replay::execute(self, request).await
    }

    pub async fn run_decay_pass(
        &self,
        agent_id: Option<String>,
        archive_threshold: f32,
        forget_threshold: f32,
    ) -> Result<lifecycle::DecayPassResult> {
        let agent_id = agent_id.unwrap_or_else(|| self.default_agent_id.clone());
        lifecycle::run_decay_pass(self, &agent_id, archive_threshold, forget_threshold).await
    }

    pub async fn run_consolidation(
        &self,
        agent_id: Option<String>,
        min_cluster_size: usize,
    ) -> Result<lifecycle::ConsolidationResult> {
        let agent_id = agent_id.unwrap_or_else(|| self.default_agent_id.clone());
        lifecycle::run_consolidation(self, &agent_id, min_cluster_size).await
    }

    pub async fn verify_integrity(
        &self,
        agent_id: Option<String>,
        thread_id: Option<&str>,
    ) -> Result<crate::hash::ChainVerificationResult> {
        let agent_id = agent_id.unwrap_or_else(|| self.default_agent_id.clone());
        let records = self
            .storage
            .list_memories_by_agent_ordered(&agent_id, thread_id, 10000)
            .await?;
        Ok(crate::hash::verify_chain(&records))
    }

    pub async fn trace_causality(
        &self,
        event_id: uuid::Uuid,
        max_depth: usize,
    ) -> Result<causality::CausalChain> {
        causality::trace_causality(self, event_id, max_depth, causality::TraceDirection::Down, None).await
    }

    pub async fn trace_causality_with_options(
        &self,
        event_id: uuid::Uuid,
        max_depth: usize,
        direction: causality::TraceDirection,
        event_type_filter: Option<crate::model::event::EventType>,
    ) -> Result<causality::CausalChain> {
        causality::trace_causality(self, event_id, max_depth, direction, event_type_filter).await
    }

    pub async fn verify_event_integrity(
        &self,
        agent_id: Option<String>,
        thread_id: Option<&str>,
    ) -> Result<crate::hash::ChainVerificationResult> {
        let agent_id = agent_id.unwrap_or_else(|| self.default_agent_id.clone());
        let events = if let Some(tid) = thread_id {
            self.storage.get_events_by_thread(tid, 10000).await?
        } else {
            // list_events returns DESC order; reverse to chronological for chain verification
            let mut evts = self.storage.list_events(&agent_id, 10000, 0).await?;
            evts.reverse();
            evts
        };
        Ok(crate::hash::verify_event_chain(&events))
    }

    pub async fn detect_conflicts(
        &self,
        agent_id: Option<String>,
        threshold: f32,
    ) -> Result<conflict::ConflictDetectionResult> {
        let agent_id = agent_id.unwrap_or_else(|| self.default_agent_id.clone());
        conflict::detect_conflicts(self, &agent_id, threshold).await
    }

    pub async fn resolve_conflict(
        &self,
        conflict_pair: &conflict::ConflictPair,
        strategy: conflict::ResolutionStrategy,
    ) -> Result<()> {
        conflict::resolve_conflict(self, conflict_pair, strategy).await
    }
}
