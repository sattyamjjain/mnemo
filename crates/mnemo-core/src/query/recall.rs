use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::hash::compute_content_hash;
use crate::model::event::{AgentEvent, EventType};
use crate::model::memory::{MemoryRecord, MemoryType, Scope};
use crate::query::MnemoEngine;
use crate::storage::MemoryFilter;
#[allow(unused_imports)]
use base64::Engine as _;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemporalRange {
    pub after: Option<String>,
    pub before: Option<String>,
}

impl TemporalRange {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallRequest {
    pub query: String,
    pub agent_id: Option<String>,
    pub limit: Option<usize>,
    pub memory_type: Option<MemoryType>,
    pub memory_types: Option<Vec<MemoryType>>,
    pub scope: Option<Scope>,
    pub min_importance: Option<f32>,
    pub tags: Option<Vec<String>>,
    pub org_id: Option<String>,
    pub strategy: Option<String>,
    pub temporal_range: Option<TemporalRange>,
    pub recency_half_life_hours: Option<f64>,
    pub hybrid_weights: Option<Vec<f32>>,
    pub rrf_k: Option<f32>,
    pub as_of: Option<String>,
    /// When set, each `ScoredMemory` is augmented with a `score_breakdown`
    /// that reports the per-signal score contributions (vector, bm25, graph,
    /// recency) and final RRF rank.
    pub explain: Option<bool>,
    /// v0.4.0-rc3 (Task B1) — when `Some(true)` AND the engine has a
    /// [`ProvenanceSigner`](crate::provenance::ProvenanceSigner)
    /// attached, the response carries a [`ReadProvenance`](crate::provenance::ReadProvenance)
    /// HMAC receipt over the recalled records. Default `None` keeps
    /// the recall hot-path overhead at zero for callers that don't
    /// need verifiable receipts.
    pub with_provenance: Option<bool>,
    /// v0.4.4 — typed retrieval mode. When `Some`, takes precedence
    /// over the legacy `strategy` field (which stays in place for
    /// backwards compatibility). When `None`, the engine falls back
    /// to parsing `strategy` exactly as in v0.4.3. See
    /// [`crate::retrieval::RetrievalMode`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<crate::retrieval::RetrievalMode>,
    /// v0.4.7 — opt-in current-fact resolver. When `Some`, the
    /// engine runs a post-processor over the standard recall result
    /// set that groups candidates by `cfg.fact_key` and keeps the
    /// most-recent write per group. See
    /// [`crate::query::current_fact_resolver`] for the contract +
    /// the MINTEval arXiv:2605.18565 anchor. Default `None` keeps
    /// the read path unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_fact_resolver:
        Option<crate::query::current_fact_resolver::CurrentFactResolverConfig>,
    /// v0.4.8 — opt-in orientation cache. When `Some` AND the
    /// engine has an
    /// [`OrientationCacheStore`][crate::query::orientation_cache::OrientationCacheStore]
    /// attached, the engine maintains a per-namespace, constant-token
    /// "context map" updated from each recall hit, and returns a
    /// bounded rendering in
    /// [`RecallResponse::orientation_cache`]. PEEK-anchored
    /// (arXiv:2605.19932). Default `None` keeps the read path
    /// unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation_cache: Option<crate::query::orientation_cache::OrientationCacheConfig>,
    /// v0.4.12 — opt-in cost-aware evidence budget. When `Some`, the
    /// engine runs the [`crate::query::evidence`] selector over the
    /// ranked candidate set and returns the smallest prefix that
    /// clears the configured sufficiency bar (capped by
    /// `max_evidence`). Purely subtractive — it never reorders the
    /// retrieval's top-k. Default `None` keeps the read path unchanged
    /// (front-loaded top-`limit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_budget: Option<crate::query::evidence::EvidenceBudget>,
    /// EMBER (arXiv:2606.05894) — opt-in budgeted evidence retention.
    /// When `Some(budget)`, the engine builds a
    /// [`RetentionReport`](crate::query::retained::RetentionReport) that
    /// packs the recalled hits into at most `budget` retained tokens as
    /// verbatim *evidence capsules* (excerpt + retrieval key), ranked by
    /// a `recency × hit-rate` recoverability heuristic, and returns it in
    /// [`RecallResponse::retained_evidence`]. Purely **additive** — the
    /// `memories` list is unchanged, so the default read path is
    /// unaffected. See [`crate::query::retained`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_token_budget: Option<usize>,
    /// v0.4.15 — domain-scoped recall predicate (MASDR-RAG,
    /// arXiv:2606.11350). When set (or when
    /// [`mode`](Self::mode) is [`RetrievalMode::DomainScoped`][crate::retrieval::RetrievalMode::DomainScoped]),
    /// the candidate set is restricted to the metadata-defined
    /// sub-corpus described by this [`DomainScope`][crate::retrieval::DomainScope]
    /// *before* the dense similarity step, countering vector-search
    /// dilution at scale. Default `None` keeps the read path unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_scope: Option<crate::retrieval::DomainScope>,
}

impl RecallRequest {
    pub fn new(query: String) -> Self {
        Self {
            query,
            agent_id: None,
            limit: None,
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: None,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
            explain: None,
            with_provenance: None,
            mode: None,
            current_fact_resolver: None,
            orientation_cache: None,
            evidence_budget: None,
            retained_token_budget: None,
            domain_scope: None,
        }
    }
}

/// v0.4.7 — one entry of the supersession chain returned when the
/// current-fact resolver is enabled with
/// [`CurrentFactResolverConfig::include_supersession_chain`][crate::query::current_fact_resolver::CurrentFactResolverConfig::include_supersession_chain]
/// set to `true`. Carries the prior fact version's id + the
/// timestamps so an auditor can reconstruct the timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SupersededRecord {
    pub id: Uuid,
    pub fact_id: String,
    pub superseded_by: Uuid,
    /// Timestamp of the winning current record.
    pub superseded_at: String,
    /// Timestamp of the older record being marked superseded.
    pub prior_updated_at: String,
}

/// Per-signal score contributions for a single recall hit.
///
/// Emitted when `RecallRequest.explain = Some(true)`. Each field is the
/// raw signal score used as input to reciprocal-rank fusion (0 when the
/// memory didn't appear in that list).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub vector: f32,
    pub bm25: f32,
    pub graph: f32,
    pub recency: f32,
    /// 0-based position of the memory in the fused ranking.
    pub rrf_rank: u32,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallResponse {
    pub memories: Vec<ScoredMemory>,
    pub total: usize,
    /// HMAC receipt over the recalled records — present iff the
    /// caller set `RecallRequest.with_provenance = Some(true)` AND
    /// the engine has a `ProvenanceSigner` attached.
    /// See [`crate::provenance`].
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provenance: Option<crate::provenance::ReadProvenance>,
    /// v0.4.7 — older fact-versions dropped by the current-fact
    /// resolver, in newest-superseded → oldest order. Present iff
    /// the caller set
    /// [`CurrentFactResolverConfig::include_supersession_chain`][crate::query::current_fact_resolver::CurrentFactResolverConfig::include_supersession_chain]
    /// to `true` AND the resolver actually dropped any candidates.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub superseded: Option<Vec<SupersededRecord>>,
    /// v0.4.8 — bounded, namespace-scoped orientation map rendered
    /// after the recall ran. Present iff the caller set
    /// [`RecallRequest::orientation_cache`] AND the engine has an
    /// [`OrientationCacheStore`][crate::query::orientation_cache::OrientationCacheStore]
    /// attached AND the config did not set `include_in_response =
    /// false`. PEEK-anchored (arXiv:2605.19932).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub orientation_cache: Option<crate::query::orientation_cache::RenderedContextMap>,
    /// v0.4.12 — diagnostics from the cost-aware evidence budget.
    /// Present iff the caller set [`RecallRequest::evidence_budget`].
    /// Reports the scorer used, how many candidates were examined vs
    /// returned, the cumulative sufficiency score, and whether
    /// early-stop / the cap fired. See [`crate::query::evidence`].
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub evidence_selection: Option<crate::query::evidence::EvidenceSelectionReport>,
    /// EMBER (arXiv:2606.05894) — budgeted evidence-retention view.
    /// Present iff the caller set
    /// [`RecallRequest::retained_token_budget`]. Carries verbatim
    /// evidence capsules (excerpt + retrieval key) packed under the
    /// requested token cap, ranked by recoverability. Additive: the
    /// `memories` list above is unchanged. See [`crate::query::retained`].
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub retained_evidence: Option<crate::query::retained::RetentionReport>,
    /// v0.5.1 — active-reconstruction belief-state node (MRAgent,
    /// arXiv:2606.06036). Present iff the caller selected the
    /// `reconstruct` strategy ([`RetrievalMode::Reconstruct`][crate::retrieval::RetrievalMode::Reconstruct]).
    /// Carries a deterministic summary synthesised from the retrieved
    /// candidates plus the linked/causal context gathered by walking the
    /// memory graph. Additive: `memories` is exactly the top-k the default
    /// hybrid (`auto`) path returns, so the raw read path is unchanged.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reconstruction: Option<ReconstructedBelief>,
}

impl RecallResponse {
    pub fn new(memories: Vec<ScoredMemory>, total: usize) -> Self {
        Self {
            memories,
            total,
            provenance: None,
            superseded: None,
            orientation_cache: None,
            evidence_selection: None,
            retained_evidence: None,
            reconstruction: None,
        }
    }
}

/// v0.5.1 — a reconstructed belief-state node (MRAgent, arXiv:2606.06036).
///
/// Produced by the `reconstruct` recall strategy. Rather than returning
/// the top-k hits alone, the strategy walks the memory graph from those
/// hits to gather linked/causal context and synthesises a deterministic
/// summary the caller receives ALONGSIDE the raw `memories`. The synthesis
/// is rule-based (no LLM), so the same inputs always yield the same node —
/// it is an honest substrate for A/B-ing reconstruction vs. retrieval on
/// your own data, not a claim that retrieval is wrong.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructedBelief {
    /// The cue (query) the belief was reconstructed for.
    pub cue: String,
    /// Deterministic summary: direct evidence (the retrieved hits) followed
    /// by the linked/causal context gathered from the memory graph.
    pub summary: String,
    /// Ids of the retrieved candidates that seeded the reconstruction.
    pub source_ids: Vec<Uuid>,
    /// Ids of graph-linked memories pulled in as causal/linked context
    /// (not present in `source_ids`).
    pub linked_context_ids: Vec<Uuid>,
    /// Mean retrieval score of the source hits — a coarse confidence proxy.
    pub confidence: f32,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub id: Uuid,
    pub content: String,
    pub agent_id: String,
    pub memory_type: MemoryType,
    pub scope: Scope,
    pub importance: f32,
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
    pub score: f32,
    pub access_count: u64,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_breakdown: Option<ScoreBreakdown>,
}

impl From<(MemoryRecord, f32)> for ScoredMemory {
    fn from((record, score): (MemoryRecord, f32)) -> Self {
        Self {
            id: record.id,
            content: record.content,
            agent_id: record.agent_id,
            memory_type: record.memory_type,
            scope: record.scope,
            importance: record.importance,
            tags: record.tags,
            metadata: record.metadata,
            score,
            access_count: record.access_count,
            created_at: record.created_at,
            updated_at: record.updated_at,
            score_breakdown: None,
        }
    }
}

/// Get a memory by ID, checking cache first then falling back to storage.
async fn get_memory_cached(engine: &MnemoEngine, id: Uuid) -> Result<Option<MemoryRecord>> {
    if let Some(ref cache) = engine.cache
        && let Some(record) = cache.get(id)
    {
        return Ok(Some(record));
    }
    let result = engine.storage.get_memory(id).await?;
    if let Some(ref record) = result
        && let Some(ref cache) = engine.cache
    {
        cache.put(record.clone());
    }
    Ok(result)
}

pub async fn execute(engine: &MnemoEngine, request: RecallRequest) -> Result<RecallResponse> {
    let limit = request.limit.unwrap_or(10).min(100);
    let agent_id = request
        .agent_id
        .clone()
        .unwrap_or_else(|| engine.default_agent_id.clone());
    super::validate_agent_id(&agent_id)?;

    // Determine strategy. v0.4.4: prefer the typed
    // `mode: Option<RetrievalMode>` field when set; fall back to the
    // legacy `strategy: Option<String>` field otherwise. Backwards
    // compatible — SDKs that only marshal `strategy` continue to work.
    let strategy = if let Some(ref mode) = request.mode {
        mode.to_strategy_str()
    } else if request
        .domain_scope
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false)
    {
        // v0.4.15 — a domain_scope predicate selects domain-scoped recall
        // even when the caller didn't set the typed mode (ergonomic for
        // SDKs that only marshal a `scope` kwarg).
        "domain_scoped"
    } else {
        request.strategy.as_deref().unwrap_or("auto")
    };

    // Compute query embedding (needed for semantic/hybrid/auto)
    let query_embedding = engine.embedding.embed(&request.query).await?;

    // Pre-compute accessible memory IDs for permission-safe ANN pre-filtering
    let accessible_ids: HashSet<Uuid> = engine
        .storage
        .list_accessible_memory_ids(&agent_id, super::MAX_BATCH_QUERY_LIMIT)
        .await?
        .into_iter()
        .collect();
    let perm_filter = |id: Uuid| accessible_ids.contains(&id);

    let mut scored_memories: Vec<(MemoryRecord, f32)> = Vec::new();
    let mut breakdowns: std::collections::HashMap<Uuid, ScoreBreakdown> =
        std::collections::HashMap::new();

    match strategy {
        "lexical" => {
            // BM25-only path
            if let Some(ref ft) = engine.full_text {
                let bm25_results = ft.search(&request.query, limit * 3)?;
                for (id, score) in bm25_results {
                    if let Some(record) = get_memory_cached(engine, id).await?
                        && passes_filters(&record, &request, &agent_id, engine).await
                    {
                        scored_memories.push((record, score));
                    }
                }
            }
        }
        "semantic" => {
            // Vector-only path with permission pre-filtering
            let search_results =
                engine
                    .index
                    .filtered_search(&query_embedding, limit * 3, &perm_filter)?;
            for (id, distance) in search_results {
                if let Some(record) = get_memory_cached(engine, id).await?
                    && passes_filters(&record, &request, &agent_id, engine).await
                {
                    let score = 1.0 - distance;
                    scored_memories.push((record, score));
                }
            }
        }
        "domain_scoped" => {
            // v0.4.15 — domain-scoped recall (MASDR-RAG, arXiv:2606.11350).
            // Restrict the candidate universe to the metadata-defined
            // sub-corpus BEFORE the dense similarity step, so off-domain
            // (but semantically similar) records can never enter the
            // top-k. Then a single vector pass over the sub-corpus.
            //
            // The sub-corpus id-set is resolved from storage by the
            // `DomainScope` predicate and composed with the permission
            // filter, so the ANN sees only (accessible ∩ in-domain) ids.
            let domain_ids: Option<HashSet<Uuid>> = match request.domain_scope.as_ref() {
                Some(scope) if !scope.is_empty() => {
                    // Coarse narrowing on org_id at the storage layer, then
                    // exact predicate matching (namespace / doc_class / tags).
                    let coarse = MemoryFilter {
                        agent_id: None,
                        memory_type: None,
                        scope: None,
                        tags: None,
                        min_importance: None,
                        org_id: scope.org_id.clone(),
                        thread_id: None,
                        include_deleted: false,
                    };
                    let records = engine
                        .storage
                        .list_memories(&coarse, super::MAX_BATCH_QUERY_LIMIT, 0)
                        .await?;
                    Some(
                        records
                            .iter()
                            .filter(|r| scope.matches(r))
                            .map(|r| r.id)
                            .collect(),
                    )
                }
                // DomainScoped selected without a predicate degrades to a
                // plain vector pass (no extra restriction).
                _ => None,
            };

            let domain_filter = |id: Uuid| {
                perm_filter(id) && domain_ids.as_ref().map(|d| d.contains(&id)).unwrap_or(true)
            };
            let search_results =
                engine
                    .index
                    .filtered_search(&query_embedding, limit * 3, &domain_filter)?;
            for (id, distance) in search_results {
                if let Some(record) = get_memory_cached(engine, id).await?
                    && passes_filters(&record, &request, &agent_id, engine).await
                {
                    let score = 1.0 - distance;
                    scored_memories.push((record, score));
                }
            }
        }
        "graph" => {
            // Seed from vector results with permission pre-filtering, then expand via graph relations
            let search_results =
                engine
                    .index
                    .filtered_search(&query_embedding, limit * 3, &perm_filter)?;
            let mut seeds: Vec<(Uuid, f32)> = Vec::new();
            for (id, distance) in &search_results {
                if let Some(record) = get_memory_cached(engine, *id).await?
                    && passes_filters(&record, &request, &agent_id, engine).await
                {
                    seeds.push((*id, 1.0 - distance));
                }
            }

            // Collect graph-expanded results with configurable multi-hop traversal
            let max_hops = 2;
            let mut seen: HashSet<Uuid> = seeds.iter().map(|(id, _)| *id).collect();
            let mut graph_ranked: Vec<(Uuid, f32)> = Vec::new();

            // Seeds get score 1.0
            for &(id, _) in &seeds {
                graph_ranked.push((id, 1.0));
            }

            // Multi-hop expansion with exponential decay
            let mut frontier: Vec<Uuid> = seeds.iter().map(|(id, _)| *id).collect();
            let mut decay = 0.5_f32;
            for _hop in 0..max_hops {
                let mut next_frontier: Vec<Uuid> = Vec::new();
                for &id in &frontier {
                    let from_rels = engine.storage.get_relations_from(id).await?;
                    let to_rels = engine.storage.get_relations_to(id).await?;
                    for rel in from_rels.iter().chain(to_rels.iter()) {
                        let related_id = if rel.source_id == id {
                            rel.target_id
                        } else {
                            rel.source_id
                        };
                        if seen.insert(related_id)
                            && let Some(record) = get_memory_cached(engine, related_id).await?
                            && passes_filters(&record, &request, &agent_id, engine).await
                        {
                            graph_ranked.push((related_id, decay));
                            next_frontier.push(related_id);
                        }
                    }
                }
                frontier = next_frontier;
                decay *= 0.5;
            }

            // Use RRF fusion with vector + graph lists
            let mut v_sorted: Vec<(Uuid, f32)> = seeds.clone();
            v_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            graph_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let ranked_lists = vec![v_sorted, graph_ranked];
            let rrf_k = request.rrf_k.unwrap_or(60.0);
            let fused = if let Some(ref weights) = request.hybrid_weights {
                crate::query::retrieval::weighted_reciprocal_rank_fusion(
                    &ranked_lists,
                    rrf_k,
                    weights,
                )
            } else {
                crate::query::retrieval::reciprocal_rank_fusion(&ranked_lists, rrf_k)
            };

            for (id, score) in fused {
                if let Some(record) = get_memory_cached(engine, id).await?
                    && passes_filters(&record, &request, &agent_id, engine).await
                {
                    scored_memories.push((record, score));
                }
            }
        }
        "exact" => {
            // Filter-based exact matching, no embedding needed
            // When as_of is set, include deleted records so the as_of filter can evaluate them
            let filter = MemoryFilter {
                agent_id: Some(agent_id.clone()),
                memory_type: request.memory_type,
                scope: request.scope,
                tags: request.tags.clone(),
                min_importance: request.min_importance,
                org_id: request.org_id.clone(),
                thread_id: None,
                include_deleted: request.as_of.is_some(),
            };
            let memories = engine.storage.list_memories(&filter, limit, 0).await?;
            for record in memories {
                if passes_filters(&record, &request, &agent_id, engine).await {
                    scored_memories.push((record, 1.0));
                }
            }
        }
        _ => {
            // "auto" or "hybrid" — use hybrid if full_text available, else semantic
            let vector_results =
                engine
                    .index
                    .filtered_search(&query_embedding, limit * 3, &perm_filter)?;
            let mut vector_ranked: Vec<(Uuid, f32)> = Vec::new();
            for (id, distance) in vector_results {
                vector_ranked.push((id, 1.0 - distance));
            }

            if let Some(ref ft) = engine.full_text {
                // Hybrid: RRF fusion of vector + BM25 + recency
                let bm25_results = ft.search(&request.query, limit * 3)?;

                // Build recency-scored list from vector candidates
                let mut recency_ranked: Vec<(Uuid, f32)> = Vec::new();
                for &(id, _) in &vector_ranked {
                    if let Some(record) = get_memory_cached(engine, id).await? {
                        let r_score = crate::query::retrieval::recency_score(
                            &record.created_at,
                            request.recency_half_life_hours.unwrap_or(168.0),
                        );
                        recency_ranked.push((id, r_score));
                    }
                }
                // Also add BM25 candidates to recency
                for &(id, _) in &bm25_results {
                    if !recency_ranked.iter().any(|(rid, _)| *rid == id)
                        && let Some(record) = get_memory_cached(engine, id).await?
                    {
                        let r_score = crate::query::retrieval::recency_score(
                            &record.created_at,
                            request.recency_half_life_hours.unwrap_or(168.0),
                        );
                        recency_ranked.push((id, r_score));
                    }
                }

                // Sort each list by score descending
                let mut v_sorted = vector_ranked.clone();
                v_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let mut b_sorted = bm25_results;
                b_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                recency_ranked
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                // Graph expansion signal: from top-10 vector results, multi-hop expansion
                let max_hops = 2;
                let mut graph_ranked: Vec<(Uuid, f32)> = Vec::new();
                let top_seeds: Vec<Uuid> =
                    vector_ranked.iter().take(10).map(|(id, _)| *id).collect();
                let mut graph_seen: HashSet<Uuid> = top_seeds.iter().copied().collect();
                for &seed_id in &top_seeds {
                    graph_ranked.push((seed_id, 1.0));
                }
                let mut frontier: Vec<Uuid> = top_seeds;
                let mut decay = 0.5_f32;
                for _hop in 0..max_hops {
                    let mut next_frontier: Vec<Uuid> = Vec::new();
                    for &fid in &frontier {
                        match engine.storage.get_relations_from(fid).await {
                            Ok(from_rels) => {
                                for rel in &from_rels {
                                    if graph_seen.insert(rel.target_id) {
                                        graph_ranked.push((rel.target_id, decay));
                                        next_frontier.push(rel.target_id);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(memory_id = %fid, error = %e, "graph expansion: failed to get outgoing relations");
                            }
                        }
                        match engine.storage.get_relations_to(fid).await {
                            Ok(to_rels) => {
                                for rel in &to_rels {
                                    if graph_seen.insert(rel.source_id) {
                                        graph_ranked.push((rel.source_id, decay));
                                        next_frontier.push(rel.source_id);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(memory_id = %fid, error = %e, "graph expansion: failed to get incoming relations");
                            }
                        }
                    }
                    frontier = next_frontier;
                    decay *= 0.5;
                }
                graph_ranked
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                // Capture per-signal score maps before moving the ranked lists
                // into the fusion call, so `explain=true` can surface each
                // signal's contribution in the response.
                let explain = request.explain.unwrap_or(false);
                type SignalMap = std::collections::HashMap<Uuid, f32>;
                let (vector_map, bm25_map, recency_map, graph_map): (
                    SignalMap,
                    SignalMap,
                    SignalMap,
                    SignalMap,
                ) = if explain {
                    (
                        v_sorted.iter().copied().collect(),
                        b_sorted.iter().copied().collect(),
                        recency_ranked.iter().copied().collect(),
                        graph_ranked.iter().copied().collect(),
                    )
                } else {
                    Default::default()
                };

                let ranked_lists = vec![v_sorted, b_sorted, recency_ranked, graph_ranked];
                let rrf_k = request.rrf_k.unwrap_or(60.0);
                let fused = if let Some(ref weights) = request.hybrid_weights {
                    crate::query::retrieval::weighted_reciprocal_rank_fusion(
                        &ranked_lists,
                        rrf_k,
                        weights,
                    )
                } else {
                    crate::query::retrieval::reciprocal_rank_fusion(&ranked_lists, rrf_k)
                };

                for (rank, (id, score)) in fused.into_iter().enumerate() {
                    if let Some(record) = get_memory_cached(engine, id).await?
                        && passes_filters(&record, &request, &agent_id, engine).await
                    {
                        scored_memories.push((record, score));
                        if explain {
                            breakdowns.insert(
                                id,
                                ScoreBreakdown {
                                    vector: vector_map.get(&id).copied().unwrap_or(0.0),
                                    bm25: bm25_map.get(&id).copied().unwrap_or(0.0),
                                    graph: graph_map.get(&id).copied().unwrap_or(0.0),
                                    recency: recency_map.get(&id).copied().unwrap_or(0.0),
                                    rrf_rank: rank as u32,
                                },
                            );
                        }
                    }
                }
            } else {
                // Fallback to semantic-only
                for (id, score) in vector_ranked {
                    if let Some(record) = get_memory_cached(engine, id).await?
                        && passes_filters(&record, &request, &agent_id, engine).await
                    {
                        scored_memories.push((record, score));
                    }
                }
            }
        }
    }

    // Sort by score descending
    scored_memories.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored_memories.truncate(limit);

    // v0.4.12 — opt-in cost-aware evidence budget. Runs only when the
    // caller set `request.evidence_budget`. The selector operates on
    // the already-ranked list and returns the smallest prefix that
    // clears the sufficiency bar (capped by `max_evidence`); it never
    // reorders, so the top-k cosine/RRF ordering is preserved. Applied
    // BEFORE `touch_memory` so we do not mark-accessed evidence the
    // budget trimmed away (cost-aware on the write side too). See
    // [`crate::query::evidence`].
    let evidence_selection = if let Some(ref budget) = request.evidence_budget {
        let cosine_default = crate::query::evidence::CosineScorer;
        let scorer: &dyn crate::query::evidence::EvidenceScorer =
            match (budget.scorer, engine.evidence_scorer.as_ref()) {
                (crate::query::evidence::ScorerKind::Delta, Some(s)) => s.as_ref(),
                _ => &cosine_default,
            };
        // Pass the query embedding only when it is non-degenerate
        // (NoopEmbedding yields all-zero vectors, for which cosine is
        // undefined and the scorer should fall back to retrieval score).
        let q_emb: Option<&[f32]> = if query_embedding.iter().any(|v| *v != 0.0) {
            Some(query_embedding.as_slice())
        } else {
            None
        };
        let candidates: Vec<crate::query::evidence::EvidenceCandidate<'_>> = scored_memories
            .iter()
            .map(|(r, score)| crate::query::evidence::EvidenceCandidate {
                content: &r.content,
                embedding: r.embedding.as_deref(),
                retrieval_score: *score,
            })
            .collect();
        let selection = crate::query::evidence::select_within_budget(
            &candidates,
            budget,
            scorer,
            &request.query,
            q_emb,
        );
        let keep = selection.keep;
        drop(candidates);
        scored_memories.truncate(keep);
        Some(selection.report)
    } else {
        None
    };

    let _total_pre_resolver = scored_memories.len();

    // Touch accessed memories
    for (record, _) in &scored_memories {
        if let Err(e) = engine.storage.touch_memory(record.id).await {
            tracing::warn!(memory_id = %record.id, error = %e, "failed to update access timestamp");
        }
    }

    // Decrypt content if encryption is configured
    if let Some(ref enc) = engine.encryption {
        for (record, _) in &mut scored_memories {
            match base64::engine::general_purpose::STANDARD.decode(&record.content) {
                Ok(encrypted_bytes) => match enc.decrypt(&encrypted_bytes) {
                    Ok(decrypted) => match String::from_utf8(decrypted) {
                        Ok(plaintext) => record.content = plaintext,
                        Err(e) => {
                            tracing::error!(memory_id = %record.id, error = %e, "decrypted content is not valid UTF-8");
                            record.content = "[content unavailable: decryption error]".to_string();
                        }
                    },
                    Err(e) => {
                        tracing::error!(memory_id = %record.id, error = %e, "failed to decrypt memory content");
                        record.content = "[content unavailable: decryption error]".to_string();
                    }
                },
                Err(e) => {
                    tracing::error!(memory_id = %record.id, error = %e, "failed to decode encrypted content");
                    record.content = "[content unavailable: decryption error]".to_string();
                }
            }
        }
    }

    // Keep the underlying records around if the caller asked for a
    // provenance receipt (Task B1) — the HMAC chain needs the
    // content_hash + prev_hash off each record before they get
    // collapsed into ScoredMemory.
    let provenance_records: Option<Vec<MemoryRecord>> =
        if request.with_provenance == Some(true) && engine.provenance_signer.is_some() {
            Some(scored_memories.iter().map(|(r, _)| r.clone()).collect())
        } else {
            None
        };

    let memories: Vec<ScoredMemory> = scored_memories
        .into_iter()
        .map(|(record, score)| {
            let id = record.id;
            let mut scored = ScoredMemory::from((record, score));
            if let Some(breakdown) = breakdowns.remove(&id) {
                scored.score_breakdown = Some(breakdown);
            }
            scored
        })
        .collect();

    // v0.4.7 — opt-in current-fact resolver post-process. Runs only
    // when the caller set `request.current_fact_resolver`. The
    // resolver groups by `cfg.fact_key`, keeps the most-recent
    // write per group, and (optionally) returns the older versions
    // as a supersession chain. See
    // [`crate::query::current_fact_resolver`] for the MINTEval
    // arXiv:2605.18565 anchor + the contract.
    let (memories, superseded_chain) = if let Some(ref cfg) = request.current_fact_resolver {
        let out = crate::query::current_fact_resolver::resolve(cfg, memories);
        let chain = if cfg.include_supersession_chain && !out.superseded.is_empty() {
            Some(out.superseded)
        } else {
            None
        };
        (out.kept, chain)
    } else {
        (memories, None)
    };
    let total = memories.len();

    // v0.5.1 — active reconstruction (MRAgent, arXiv:2606.06036). When the
    // caller selected the `reconstruct` strategy, walk the memory graph from
    // the retrieved hits to gather linked/causal context and synthesise a
    // deterministic belief-state node returned ALONGSIDE the raw hits. The
    // `memories` list above is untouched, so this is purely additive.
    let reconstruction = if strategy == "reconstruct" {
        Some(reconstruct_belief(engine, &request, &agent_id, &memories).await)
    } else {
        None
    };

    // v0.4.8 — opt-in orientation cache. Runs only when the caller
    // set `request.orientation_cache` AND the engine has an
    // `OrientationCacheStore` attached. Per-namespace map is
    // updated from the hits + a bounded rendering is returned. See
    // [`crate::query::orientation_cache`] for the PEEK
    // arXiv:2605.19932 anchor + the contract.
    let orientation_rendered = match (
        request.orientation_cache.as_ref(),
        engine.orientation_cache_store.as_ref(),
    ) {
        (Some(cfg), Some(store)) => {
            let ns = crate::query::orientation_cache::resolve_namespace(
                cfg,
                &agent_id,
                request.org_id.as_deref(),
            );
            let rendered =
                crate::query::orientation_cache::update_and_render(store, cfg, &ns, &memories);
            if cfg.include_in_response {
                Some(rendered)
            } else {
                None
            }
        }
        _ => None,
    };

    // Emit MemoryRead event with hash chain linking (fire-and-forget)
    let now = chrono::Utc::now().to_rfc3339();
    let event_content_hash = compute_content_hash(&request.query, &agent_id, &now);
    let prev_event_hash = match engine.storage.get_latest_event_hash(&agent_id, None).await {
        Ok(hash) => hash,
        Err(e) => {
            tracing::warn!(error = %e, "failed to get latest event hash, starting new chain segment");
            None
        }
    };
    let event_prev_hash = Some(crate::hash::compute_chain_hash(
        &event_content_hash,
        prev_event_hash.as_deref(),
    ));
    let mut event = AgentEvent {
        id: Uuid::now_v7(),
        agent_id: agent_id.clone(),
        thread_id: None,
        run_id: None,
        parent_event_id: None,
        event_type: EventType::MemoryRead,
        payload: serde_json::json!({
            "query": request.query,
            "results": total,
            "strategy": strategy,
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
        content_hash: event_content_hash,
        prev_hash: event_prev_hash,
        embedding: None,
    };
    // Optionally embed the event payload
    if engine.embed_events
        && let Ok(emb) = engine.embedding.embed(&event.payload.to_string()).await
    {
        event.embedding = Some(emb);
    }
    if let Err(e) = engine.storage.insert_event(&event).await {
        tracing::error!(event_id = %event.id, error = %e, "failed to insert audit event");
    }

    // v0.4.0-rc3 (B1) — sign a ReadProvenance over the recalled
    // records when the caller opted in. Failures are non-fatal:
    // missing signer or HMAC error degrades to "no provenance" so the
    // recall still returns. The caller can detect by `provenance.is_none()`.
    let provenance = if let (Some(records), Some(signer)) =
        (provenance_records, engine.provenance_signer.as_ref())
    {
        match signer.sign(&agent_id, &request.query, &records) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(error = %e, "failed to sign read provenance; degrading to no-provenance response");
                None
            }
        }
    } else {
        None
    };

    // EMBER (arXiv:2606.05894) — opt-in budgeted evidence retention.
    // Runs only when the caller set `request.retained_token_budget`.
    // Builds verbatim evidence capsules (excerpt + retrieval key) packed
    // under the token cap, ranked by `recency × hit-rate` recoverability.
    // Computed from the FINAL `memories` (post current-fact resolver,
    // decrypted) and returned ALONGSIDE them — `memories` is not
    // modified, so the default read path is unaffected. See
    // [`crate::query::retained`].
    let retained_evidence = request.retained_token_budget.map(|budget| {
        let retain_now = chrono::Utc::now();
        let candidates: Vec<crate::query::retained::RetentionCandidate<'_>> = memories
            .iter()
            .map(|m| {
                let age_hours = chrono::DateTime::parse_from_rfc3339(&m.updated_at)
                    .or_else(|_| chrono::DateTime::parse_from_rfc3339(&m.created_at))
                    .map(|ts| {
                        (retain_now - ts.with_timezone(&chrono::Utc)).num_seconds() as f64 / 3600.0
                    })
                    .unwrap_or(0.0);
                crate::query::retained::RetentionCandidate {
                    id: m.id,
                    content: &m.content,
                    access_count: m.access_count,
                    age_hours,
                    retrieval_score: m.score,
                }
            })
            .collect();
        crate::query::retained::retain_within_budget(
            &candidates,
            budget,
            crate::query::retained::DEFAULT_EXCERPT_TOKENS,
        )
    });

    Ok(RecallResponse {
        memories,
        total,
        provenance,
        superseded: superseded_chain,
        orientation_cache: orientation_rendered,
        evidence_selection,
        retained_evidence,
        reconstruction,
    })
}

/// v0.5.1 — synthesise a [`ReconstructedBelief`] from the retrieved hits
/// (MRAgent, arXiv:2606.06036). Walks one hop of memory-graph relations
/// outward from each hit to gather linked/causal context, then renders a
/// deterministic, rule-based summary (no LLM). Used only by the
/// `reconstruct` strategy; the raw `memories` are left unchanged.
async fn reconstruct_belief(
    engine: &MnemoEngine,
    request: &RecallRequest,
    agent_id: &str,
    memories: &[ScoredMemory],
) -> ReconstructedBelief {
    let cue = request.query.clone();
    if memories.is_empty() {
        return ReconstructedBelief {
            cue: cue.clone(),
            summary: format!("No memories matched the cue \"{cue}\"."),
            source_ids: Vec::new(),
            linked_context_ids: Vec::new(),
            confidence: 0.0,
        };
    }

    let source_ids: Vec<Uuid> = memories.iter().map(|m| m.id).collect();
    let mut seen: HashSet<Uuid> = source_ids.iter().copied().collect();

    // Walk one hop of relations outward from each hit to gather
    // linked/causal context. Deterministic order: hits in rank order, and
    // within a hit, outgoing relations before incoming.
    let mut linked: Vec<(Uuid, String)> = Vec::new();
    for m in memories {
        let from_rels = engine
            .storage
            .get_relations_from(m.id)
            .await
            .unwrap_or_default();
        let to_rels = engine
            .storage
            .get_relations_to(m.id)
            .await
            .unwrap_or_default();
        for rel in from_rels.iter().chain(to_rels.iter()) {
            let linked_id = if rel.source_id == m.id {
                rel.target_id
            } else {
                rel.source_id
            };
            if seen.insert(linked_id)
                && let Ok(Some(mut rec)) = engine.storage.get_memory(linked_id).await
                && passes_filters(&rec, request, agent_id, engine).await
            {
                decrypt_record_content(engine, &mut rec);
                linked.push((linked_id, rec.content));
            }
        }
    }

    // Deterministic, rule-based belief summary (no LLM).
    let mut summary = format!("Reconstructed belief for cue \"{cue}\":\n\nDirect evidence:\n");
    for (i, m) in memories.iter().enumerate() {
        summary.push_str(&format!("{}. {}\n", i + 1, excerpt(&m.content, 200)));
    }
    if linked.is_empty() {
        summary.push_str("\n(No linked context found in the memory graph.)\n");
    } else {
        summary.push_str("\nLinked context (from graph relations):\n");
        for (_, content) in &linked {
            summary.push_str(&format!("- {}\n", excerpt(content, 160)));
        }
    }

    let confidence = memories.iter().map(|m| m.score).sum::<f32>() / memories.len() as f32;

    ReconstructedBelief {
        cue,
        summary,
        source_ids,
        linked_context_ids: linked.into_iter().map(|(id, _)| id).collect(),
        confidence,
    }
}

/// First non-empty line of `content`, truncated to `max` chars (char-safe).
fn excerpt(content: &str, max: usize) -> String {
    let line = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let trimmed = line.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_string()
    } else {
        let mut out: String = trimmed.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// Decrypt a record's content in place if engine-level encryption is on.
/// Mirrors the read-path decryption in [`execute`]; used by
/// [`reconstruct_belief`] for graph-linked records fetched after the main
/// decrypt loop.
fn decrypt_record_content(engine: &MnemoEngine, record: &mut MemoryRecord) {
    if let Some(ref enc) = engine.encryption {
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&record.content)
            && let Ok(plain) = enc.decrypt(&bytes)
            && let Ok(text) = String::from_utf8(plain)
        {
            record.content = text;
        } else {
            record.content = "[content unavailable: decryption error]".to_string();
        }
    }
}

async fn passes_filters(
    record: &MemoryRecord,
    request: &RecallRequest,
    agent_id: &str,
    engine: &MnemoEngine,
) -> bool {
    // Experience-tier plan records (DocTrace, arXiv:2606.10921) are never
    // surfaced by ordinary recall — they are replayed only via
    // `recall_plan`. Skip them unless the caller explicitly asks for the
    // reserved tag.
    if record
        .tags
        .iter()
        .any(|t| t == crate::query::experience::EXPERIENCE_PLAN_TAG)
        && !request
            .tags
            .as_ref()
            .map(|ts| {
                ts.iter()
                    .any(|t| t == crate::query::experience::EXPERIENCE_PLAN_TAG)
            })
            .unwrap_or(false)
    {
        return false;
    }

    // Skip deleted (unless as_of is set — the as_of filter handles deleted records)
    if request.as_of.is_none() && record.is_deleted() {
        return false;
    }

    // Skip expired
    if let Some(ref expires_at) = record.expires_at
        && let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires_at)
        && exp < chrono::Utc::now()
    {
        return false;
    }

    // Skip quarantined
    if record.quarantined {
        return false;
    }

    // Scope filter (explicit request scope filter, separate from visibility below)
    if let Some(ref s) = request.scope
        && record.scope != *s
    {
        return false;
    }

    // Type filter: memory_types (multi) takes precedence over memory_type (single)
    if let Some(ref mts) = request.memory_types {
        if !mts.contains(&record.memory_type) {
            return false;
        }
    } else if let Some(ref mt) = request.memory_type
        && record.memory_type != *mt
    {
        return false;
    }

    // Importance filter
    if let Some(min_imp) = request.min_importance
        && record.importance < min_imp
    {
        return false;
    }

    // Tags filter
    if let Some(ref req_tags) = request.tags
        && !req_tags.iter().any(|t| record.tags.contains(t))
    {
        return false;
    }

    // Temporal range filter (parse to DateTime for correct comparison)
    if let Some(ref tr) = request.temporal_range {
        if let Some(ref after) = tr.after
            && let (Ok(after_dt), Ok(record_dt)) = (
                chrono::DateTime::parse_from_rfc3339(after),
                chrono::DateTime::parse_from_rfc3339(&record.created_at),
            )
            && record_dt < after_dt
        {
            return false;
        }
        if let Some(ref before) = tr.before
            && let (Ok(before_dt), Ok(record_dt)) = (
                chrono::DateTime::parse_from_rfc3339(before),
                chrono::DateTime::parse_from_rfc3339(&record.created_at),
            )
            && record_dt > before_dt
        {
            return false;
        }
    }

    // Point-in-time as_of filter: show memory state at time T
    if let Some(ref as_of) = request.as_of {
        if let (Ok(as_of_dt), Ok(record_dt)) = (
            chrono::DateTime::parse_from_rfc3339(as_of),
            chrono::DateTime::parse_from_rfc3339(&record.created_at),
        ) && record_dt > as_of_dt
        {
            // Exclude memories created after as_of
            return false;
        }
        // Exclude memories already deleted at as_of
        if let Some(ref deleted_at) = record.deleted_at
            && let (Ok(del_dt), Ok(as_of_dt)) = (
                chrono::DateTime::parse_from_rfc3339(deleted_at),
                chrono::DateTime::parse_from_rfc3339(as_of),
            )
            && del_dt <= as_of_dt
        {
            return false;
        }
    }

    // Scope-based visibility
    match record.scope {
        Scope::Public | Scope::Global => true,
        Scope::Shared => {
            record.agent_id == agent_id
                || engine
                    .storage
                    .check_permission(
                        record.id,
                        agent_id,
                        crate::model::acl::Permission::Read,
                    )
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(memory_id = %record.id, error = %e, "permission check failed, denying access");
                        false
                    })
        }
        Scope::Private => record.agent_id == agent_id,
    }
}
