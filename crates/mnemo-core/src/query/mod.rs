pub mod branch;
pub mod causality;
pub mod checkpoint;
pub mod conflict;
pub mod current_fact_resolver;
pub mod event_builder;
pub mod evidence;
pub mod forget;
pub mod lifecycle;
pub mod maturity;
pub mod merge;
pub mod orientation_cache;
pub mod poisoning;
pub mod recall;
pub mod reflection;
pub mod remember;
pub mod replay;
pub mod retrieval;
pub mod share;

use std::sync::Arc;

use crate::cache::MemoryCache;
use crate::embedding::EmbeddingProvider;
use crate::encryption::ContentEncryption;
use crate::error::{Error, Result};
use crate::index::VectorIndex;
use crate::search::FullTextIndex;
use crate::storage::StorageBackend;
use crate::storage::cold::ColdStorage;

const MAX_AGENT_ID_LEN: usize = 256;

/// Maximum number of records returned by a single batch query.
/// Prevents unbounded memory growth while supporting reasonable workloads.
pub const MAX_BATCH_QUERY_LIMIT: usize = 10_000;

/// Validate that an agent_id contains only safe characters and is within length limits.
pub fn validate_agent_id(agent_id: &str) -> Result<()> {
    if agent_id.is_empty() {
        return Err(Error::Validation("agent_id cannot be empty".into()));
    }
    if agent_id.len() > MAX_AGENT_ID_LEN {
        return Err(Error::Validation(format!(
            "agent_id exceeds max length of {MAX_AGENT_ID_LEN}"
        )));
    }
    if !agent_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(Error::Validation(
            "agent_id must contain only alphanumeric characters, hyphens, underscores, or dots"
                .into(),
        ));
    }
    Ok(())
}

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
    /// Default TTL applied to `Working`-tier memories whose `remember`
    /// request does not supply an explicit `ttl_seconds`. Defaults to 1 hour.
    pub ttl_working_seconds: u64,
    /// Importance floor enforced on write for `Procedural`-tier memories.
    /// Defaults to 0.8.
    pub procedural_importance_floor: f32,
    /// Poisoning policy read by `check_for_anomaly`. Defaults to the v0.3.2
    /// behaviour (no z-score outlier gate). Override with
    /// [`MnemoEngine::with_poisoning_policy`].
    pub poisoning_policy: poisoning::PoisoningPolicy,
    /// v0.4.0-rc3 (Task B1) — when set, every
    /// `recall(req)` with `req.with_provenance == Some(true)` returns
    /// an HMAC-signed [`ReadProvenance`](crate::provenance::ReadProvenance)
    /// receipt. `None` keeps the recall hot-path overhead at zero.
    /// Attach via [`MnemoEngine::with_provenance_signer`].
    pub provenance_signer: Option<Arc<crate::provenance::ProvenanceSigner>>,
    /// v0.4.8 — when set, every `recall(req)` with
    /// `req.orientation_cache == Some(_)` updates this in-process,
    /// namespace-scoped, constant-token "context map" and returns a
    /// bounded rendering alongside the top-k. PEEK-anchored
    /// (arXiv:2605.19932). `None` keeps the recall hot-path
    /// overhead at zero. Attach via
    /// [`MnemoEngine::with_orientation_cache_store`].
    pub orientation_cache_store: Option<Arc<orientation_cache::OrientationCacheStore>>,
    /// v0.4.10 — feedback-driven consolidation trigger metric. Default
    /// [`maturity::ConsolidationPolicy::FixedSize`] preserves the
    /// v0.4.x behaviour. Attach a
    /// [`maturity::ConsolidationPolicy::MaturityDriven`] policy via
    /// [`MnemoEngine::with_consolidation_policy`] to opt in to the
    /// scalar maturity gate (recency / hit-success / edge-degree /
    /// redundancy). Internal anchor: FluxMem (arXiv:2605.28773), prior
    /// art only — mnemo's policy is a structural cousin, not a
    /// reproduction.
    pub consolidation_policy: maturity::ConsolidationPolicy,
    /// v0.4.12 — optional answer-impact scorer for the cost-aware
    /// evidence budget. When a recall sets
    /// [`RecallRequest::evidence_budget`](recall::RecallRequest::evidence_budget)
    /// with [`ScorerKind::Delta`](evidence::ScorerKind::Delta) AND this
    /// is `Some`, the budget uses this scorer to decide sufficiency;
    /// otherwise it falls back to [`evidence::CosineScorer`]. `None`
    /// keeps the recall hot-path at zero overhead. Attach via
    /// [`MnemoEngine::with_evidence_scorer`].
    pub evidence_scorer: Option<Arc<dyn evidence::EvidenceScorer>>,
}

/// Default TTL (in seconds) applied to Working-tier memories.
pub const DEFAULT_TTL_WORKING_SECONDS: u64 = 3600;

/// Minimum importance floor applied to Procedural-tier memories on write.
pub const DEFAULT_PROCEDURAL_IMPORTANCE_FLOOR: f32 = 0.8;

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
            ttl_working_seconds: DEFAULT_TTL_WORKING_SECONDS,
            procedural_importance_floor: DEFAULT_PROCEDURAL_IMPORTANCE_FLOOR,
            poisoning_policy: poisoning::PoisoningPolicy::default(),
            provenance_signer: None,
            orientation_cache_store: None,
            consolidation_policy: maturity::ConsolidationPolicy::default(),
            evidence_scorer: None,
        }
    }

    /// Attach a [`provenance::ProvenanceSigner`](crate::provenance::ProvenanceSigner)
    /// (Task B1) so callers can request signed read-receipts via
    /// `RecallRequest.with_provenance = Some(true)`.
    pub fn with_provenance_signer(
        mut self,
        signer: Arc<crate::provenance::ProvenanceSigner>,
    ) -> Self {
        self.provenance_signer = Some(signer);
        self
    }

    /// Attach a [`poisoning::PoisoningPolicy`] to the engine. See
    /// [`poisoning::PoisoningPolicy::with_outlier_threshold`] for the
    /// v0.3.3 z-score outlier gate.
    pub fn with_poisoning_policy(mut self, policy: poisoning::PoisoningPolicy) -> Self {
        self.poisoning_policy = policy;
        self
    }

    /// Override the default 1-hour TTL applied to `Working`-tier memories
    /// when a caller does not supply an explicit `ttl_seconds`.
    pub fn with_ttl_working_seconds(mut self, seconds: u64) -> Self {
        self.ttl_working_seconds = seconds;
        self
    }

    /// Override the default 0.8 importance floor applied to Procedural
    /// memories on write.
    pub fn with_procedural_importance_floor(mut self, floor: f32) -> Self {
        self.procedural_importance_floor = floor.clamp(0.0, 1.0);
        self
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

    /// v0.4.8 — attach a per-engine orientation-cache store. Recall
    /// calls that set
    /// [`RecallRequest::orientation_cache`][crate::query::recall::RecallRequest::orientation_cache]
    /// will update + render the namespace-scoped, constant-token
    /// context map. See
    /// [`crate::query::orientation_cache`] for the contract +
    /// the PEEK arXiv:2605.19932 anchor.
    pub fn with_orientation_cache_store(
        mut self,
        store: Arc<orientation_cache::OrientationCacheStore>,
    ) -> Self {
        self.orientation_cache_store = Some(store);
        self
    }

    /// v0.4.10 — attach a [`maturity::ConsolidationPolicy`]. The default
    /// `FixedSize` policy preserves the legacy behaviour; pass
    /// `MaturityDriven(MaturityPolicy::balanced())` to opt in to the
    /// feedback-driven trigger metric. See
    /// [`crate::query::maturity`] for the score contract.
    pub fn with_consolidation_policy(mut self, policy: maturity::ConsolidationPolicy) -> Self {
        self.consolidation_policy = policy;
        self
    }

    /// v0.4.12 — attach an answer-impact [`evidence::EvidenceScorer`]
    /// (typically a [`evidence::DeltaScorer`] wrapping an LLM callback)
    /// used by the cost-aware evidence budget when a recall requests
    /// [`ScorerKind::Delta`](evidence::ScorerKind::Delta). Without an
    /// attached scorer, delta-mode budgets fall back to
    /// [`evidence::CosineScorer`]. See [`crate::query::evidence`].
    pub fn with_evidence_scorer(mut self, scorer: Arc<dyn evidence::EvidenceScorer>) -> Self {
        self.evidence_scorer = Some(scorer);
        self
    }

    pub async fn remember(
        &self,
        request: remember::RememberRequest,
    ) -> Result<remember::RememberResponse> {
        remember::execute(self, request).await
    }

    pub async fn recall(&self, request: recall::RecallRequest) -> Result<recall::RecallResponse> {
        recall::execute(self, request).await
    }

    pub async fn forget(&self, request: forget::ForgetRequest) -> Result<forget::ForgetResponse> {
        forget::execute(self, request).await
    }

    /// Subject-scoped erasure for GDPR / DPDPA compliance.
    /// See [`forget::forget_subject`] for strategy semantics.
    pub async fn forget_subject(
        &self,
        request: forget::ForgetSubjectRequest,
    ) -> Result<forget::ForgetSubjectResponse> {
        forget::forget_subject(self, request).await
    }

    /// Hard-delete every memory whose `expires_at` is in the past and emit
    /// one `MemoryExpired` audit event per deletion.
    pub async fn run_ttl_sweep(&self) -> Result<lifecycle::TtlReport> {
        lifecycle::run_ttl_sweep(self).await
    }

    /// Auto-Dream-compatible reflection pass: date absolutization, external
    /// rewrite acceptance, semantic dedup, low-importance conflict
    /// resolution, and stale archival. See [`reflection::run_reflection_pass`].
    pub async fn run_reflection_pass(
        &self,
        agent_id: Option<String>,
    ) -> Result<reflection::ReflectionReport> {
        let agent_id = agent_id.unwrap_or_else(|| self.default_agent_id.clone());
        reflection::run_reflection_pass(self, &agent_id).await
    }

    /// Reflection pass that honours the new `ReflectionMode` gate (v0.3.1).
    /// Use `Coordinated` to avoid double-work when Auto Dream is also running.
    pub async fn run_reflection_pass_with_mode(
        &self,
        agent_id: Option<String>,
        mode: reflection::ReflectionMode,
        force: bool,
    ) -> Result<reflection::ReflectionReport> {
        let agent_id = agent_id.unwrap_or_else(|| self.default_agent_id.clone());
        reflection::run_reflection_pass_with_mode(self, &agent_id, mode, force).await
    }

    /// List quarantined memories for operator review. See
    /// [`poisoning::replay_quarantine`].
    pub async fn replay_quarantine(
        &self,
        agent_id: Option<String>,
        since: Option<&str>,
    ) -> Result<Vec<poisoning::QuarantineReplayEntry>> {
        let agent_id = agent_id.unwrap_or_else(|| self.default_agent_id.clone());
        poisoning::replay_quarantine(self, &agent_id, since).await
    }

    pub async fn share(&self, request: share::ShareRequest) -> Result<share::ShareResponse> {
        share::execute(self, request).await
    }

    pub async fn checkpoint(
        &self,
        request: checkpoint::CheckpointRequest,
    ) -> Result<checkpoint::CheckpointResponse> {
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
        causality::trace_causality(
            self,
            event_id,
            max_depth,
            causality::TraceDirection::Down,
            None,
        )
        .await
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
