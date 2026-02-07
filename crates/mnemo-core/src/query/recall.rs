use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::hash::compute_content_hash;
#[allow(unused_imports)]
use base64::Engine as _;
use crate::model::event::{AgentEvent, EventType};
use crate::model::memory::{MemoryRecord, MemoryType, Scope};
use crate::query::MnemoEngine;
use crate::storage::MemoryFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalRange {
    pub after: Option<String>,
    pub before: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallResponse {
    pub memories: Vec<ScoredMemory>,
    pub total: usize,
}

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
        }
    }
}

/// Get a memory by ID, checking cache first then falling back to storage.
async fn get_memory_cached(engine: &MnemoEngine, id: Uuid) -> Result<Option<MemoryRecord>> {
    if let Some(ref cache) = engine.cache {
        if let Some(record) = cache.get(id) {
            return Ok(Some(record));
        }
    }
    let result = engine.storage.get_memory(id).await?;
    if let Some(ref record) = result {
        if let Some(ref cache) = engine.cache {
            cache.put(record.clone());
        }
    }
    Ok(result)
}

pub async fn execute(engine: &MnemoEngine, request: RecallRequest) -> Result<RecallResponse> {
    let limit = request.limit.unwrap_or(10).min(100);
    let agent_id = request.agent_id.clone().unwrap_or_else(|| engine.default_agent_id.clone());

    // Determine strategy
    let strategy = request.strategy.as_deref().unwrap_or("auto");

    // Compute query embedding (needed for semantic/hybrid/auto)
    let query_embedding = engine.embedding.embed(&request.query).await?;

    // Pre-compute accessible memory IDs for permission-safe ANN pre-filtering
    let accessible_ids: HashSet<Uuid> = engine
        .storage
        .list_accessible_memory_ids(&agent_id, 10000)
        .await?
        .into_iter()
        .collect();
    let perm_filter = |id: Uuid| accessible_ids.contains(&id);

    let mut scored_memories: Vec<(MemoryRecord, f32)> = Vec::new();

    match strategy {
        "lexical" => {
            // BM25-only path
            if let Some(ref ft) = engine.full_text {
                let bm25_results = ft.search(&request.query, limit * 3)?;
                for (id, score) in bm25_results {
                    if let Some(record) = get_memory_cached(engine, id).await? {
                        if passes_filters(&record, &request, &agent_id, engine).await {
                            scored_memories.push((record, score));
                        }
                    }
                }
            }
        }
        "semantic" => {
            // Vector-only path with permission pre-filtering
            let search_results = engine.index.filtered_search(&query_embedding, limit * 3, &perm_filter)?;
            for (id, distance) in search_results {
                if let Some(record) = get_memory_cached(engine, id).await? {
                    if passes_filters(&record, &request, &agent_id, engine).await {
                        let score = 1.0 - distance;
                        scored_memories.push((record, score));
                    }
                }
            }
        }
        "graph" => {
            // Seed from vector results with permission pre-filtering, then expand via graph relations
            let search_results = engine.index.filtered_search(&query_embedding, limit * 3, &perm_filter)?;
            let mut seeds: Vec<(Uuid, f32)> = Vec::new();
            for (id, distance) in &search_results {
                if let Some(record) = get_memory_cached(engine, *id).await? {
                    if passes_filters(&record, &request, &agent_id, engine).await {
                        seeds.push((*id, 1.0 - distance));
                    }
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
                        let related_id = if rel.source_id == id { rel.target_id } else { rel.source_id };
                        if seen.insert(related_id) {
                            if let Some(record) = get_memory_cached(engine, related_id).await? {
                                if passes_filters(&record, &request, &agent_id, engine).await {
                                    graph_ranked.push((related_id, decay));
                                    next_frontier.push(related_id);
                                }
                            }
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
                crate::query::retrieval::weighted_reciprocal_rank_fusion(&ranked_lists, rrf_k, weights)
            } else {
                crate::query::retrieval::reciprocal_rank_fusion(&ranked_lists, rrf_k)
            };

            for (id, score) in fused {
                if let Some(record) = get_memory_cached(engine, id).await? {
                    if passes_filters(&record, &request, &agent_id, engine).await {
                        scored_memories.push((record, score));
                    }
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
            let vector_results = engine.index.filtered_search(&query_embedding, limit * 3, &perm_filter)?;
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
                        let r_score = crate::query::retrieval::recency_score(&record.created_at, request.recency_half_life_hours.unwrap_or(168.0));
                        recency_ranked.push((id, r_score));
                    }
                }
                // Also add BM25 candidates to recency
                for &(id, _) in &bm25_results {
                    if !recency_ranked.iter().any(|(rid, _)| *rid == id) {
                        if let Some(record) = get_memory_cached(engine, id).await? {
                            let r_score = crate::query::retrieval::recency_score(&record.created_at, request.recency_half_life_hours.unwrap_or(168.0));
                            recency_ranked.push((id, r_score));
                        }
                    }
                }

                // Sort each list by score descending
                let mut v_sorted = vector_ranked.clone();
                v_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let mut b_sorted = bm25_results;
                b_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                recency_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                // Graph expansion signal: from top-10 vector results, multi-hop expansion
                let max_hops = 2;
                let mut graph_ranked: Vec<(Uuid, f32)> = Vec::new();
                let top_seeds: Vec<Uuid> = vector_ranked.iter().take(10).map(|(id, _)| *id).collect();
                let mut graph_seen: HashSet<Uuid> = top_seeds.iter().copied().collect();
                for &seed_id in &top_seeds {
                    graph_ranked.push((seed_id, 1.0));
                }
                let mut frontier: Vec<Uuid> = top_seeds;
                let mut decay = 0.5_f32;
                for _hop in 0..max_hops {
                    let mut next_frontier: Vec<Uuid> = Vec::new();
                    for &fid in &frontier {
                        if let Ok(from_rels) = engine.storage.get_relations_from(fid).await {
                            for rel in &from_rels {
                                if graph_seen.insert(rel.target_id) {
                                    graph_ranked.push((rel.target_id, decay));
                                    next_frontier.push(rel.target_id);
                                }
                            }
                        }
                        if let Ok(to_rels) = engine.storage.get_relations_to(fid).await {
                            for rel in &to_rels {
                                if graph_seen.insert(rel.source_id) {
                                    graph_ranked.push((rel.source_id, decay));
                                    next_frontier.push(rel.source_id);
                                }
                            }
                        }
                    }
                    frontier = next_frontier;
                    decay *= 0.5;
                }
                graph_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                let ranked_lists = vec![v_sorted, b_sorted, recency_ranked, graph_ranked];
                let rrf_k = request.rrf_k.unwrap_or(60.0);
                let fused = if let Some(ref weights) = request.hybrid_weights {
                    crate::query::retrieval::weighted_reciprocal_rank_fusion(&ranked_lists, rrf_k, weights)
                } else {
                    crate::query::retrieval::reciprocal_rank_fusion(&ranked_lists, rrf_k)
                };

                for (id, score) in fused {
                    if let Some(record) = get_memory_cached(engine, id).await? {
                        if passes_filters(&record, &request, &agent_id, engine).await {
                            scored_memories.push((record, score));
                        }
                    }
                }
            } else {
                // Fallback to semantic-only
                for (id, score) in vector_ranked {
                    if let Some(record) = get_memory_cached(engine, id).await? {
                        if passes_filters(&record, &request, &agent_id, engine).await {
                            scored_memories.push((record, score));
                        }
                    }
                }
            }
        }
    }

    // Sort by score descending
    scored_memories.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored_memories.truncate(limit);

    let total = scored_memories.len();

    // Touch accessed memories
    for (record, _) in &scored_memories {
        let _ = engine.storage.touch_memory(record.id).await;
    }

    // Decrypt content if encryption is configured
    if let Some(ref enc) = engine.encryption {
        for (record, _) in &mut scored_memories {
            if let Ok(encrypted_bytes) = base64::engine::general_purpose::STANDARD.decode(&record.content) {
                if let Ok(decrypted) = enc.decrypt(&encrypted_bytes) {
                    if let Ok(plaintext) = String::from_utf8(decrypted) {
                        record.content = plaintext;
                    }
                }
            }
        }
    }

    let memories: Vec<ScoredMemory> = scored_memories
        .into_iter()
        .map(ScoredMemory::from)
        .collect();

    // Emit MemoryRead event with hash chain linking (fire-and-forget)
    let now = chrono::Utc::now().to_rfc3339();
    let event_content_hash = compute_content_hash(&request.query, &agent_id, &now);
    let prev_event_hash = engine.storage.get_latest_event_hash(&agent_id, None).await.unwrap_or(None);
    let event_prev_hash = Some(crate::hash::compute_chain_hash(&event_content_hash, prev_event_hash.as_deref()));
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
    if engine.embed_events {
        if let Ok(emb) = engine.embedding.embed(&event.payload.to_string()).await {
            event.embedding = Some(emb);
        }
    }
    let _ = engine.storage.insert_event(&event).await;

    Ok(RecallResponse { memories, total })
}

async fn passes_filters(
    record: &MemoryRecord,
    request: &RecallRequest,
    agent_id: &str,
    engine: &MnemoEngine,
) -> bool {
    // Skip deleted (unless as_of is set — the as_of filter handles deleted records)
    if request.as_of.is_none() && record.is_deleted() {
        return false;
    }

    // Skip expired
    if let Some(ref expires_at) = record.expires_at {
        if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires_at) {
            if exp < chrono::Utc::now() {
                return false;
            }
        }
    }

    // Skip quarantined
    if record.quarantined {
        return false;
    }

    // Scope filter (explicit request scope filter, separate from visibility below)
    if let Some(ref s) = request.scope {
        if record.scope != *s {
            return false;
        }
    }

    // Type filter: memory_types (multi) takes precedence over memory_type (single)
    if let Some(ref mts) = request.memory_types {
        if !mts.contains(&record.memory_type) {
            return false;
        }
    } else if let Some(ref mt) = request.memory_type {
        if record.memory_type != *mt {
            return false;
        }
    }

    // Importance filter
    if let Some(min_imp) = request.min_importance {
        if record.importance < min_imp {
            return false;
        }
    }

    // Tags filter
    if let Some(ref req_tags) = request.tags {
        if !req_tags.iter().any(|t| record.tags.contains(t)) {
            return false;
        }
    }

    // Temporal range filter
    if let Some(ref tr) = request.temporal_range {
        if let Some(ref after) = tr.after {
            if record.created_at < *after {
                return false;
            }
        }
        if let Some(ref before) = tr.before {
            if record.created_at > *before {
                return false;
            }
        }
    }

    // Point-in-time as_of filter: show memory state at time T
    if let Some(ref as_of) = request.as_of {
        // Exclude memories created after as_of
        if record.created_at > *as_of {
            return false;
        }
        // Exclude memories already deleted at as_of
        if let Some(ref deleted_at) = record.deleted_at {
            if *deleted_at <= *as_of {
                return false;
            }
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
                    .unwrap_or(false)
        }
        Scope::Private => record.agent_id == agent_id,
    }
}
