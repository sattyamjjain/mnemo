use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemporalRange {
    /// Only return memories created after this timestamp (RFC 3339 format).
    pub after: Option<String>,
    /// Only return memories created before this timestamp (RFC 3339 format).
    pub before: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecallInput {
    /// Natural language query to search memories semantically.
    pub query: String,
    /// Maximum number of memories to return. Defaults to 10, max 100.
    pub limit: Option<usize>,
    /// Filter by memory type: "episodic", "semantic", "procedural", or "working".
    pub memory_type: Option<String>,
    /// Filter by multiple memory types. Takes precedence over memory_type if both are set.
    pub memory_types: Option<Vec<String>>,
    /// Filter by scope: "private", "shared", "public", or "global".
    pub scope: Option<String>,
    /// Filter by minimum importance score (0.0 to 1.0).
    pub min_importance: Option<f32>,
    /// Filter by tags. Returns memories matching any of the specified tags.
    pub tags: Option<Vec<String>>,
    /// Retrieval strategy: "semantic" (vector only), "lexical" (BM25 only), "hybrid" (vector + BM25 + recency), "graph" (graph traversal), "exact" (filter-based), or "auto" (hybrid if available, else semantic). Defaults to "auto".
    pub strategy: Option<String>,
    /// Filter by time range.
    pub temporal_range: Option<TemporalRange>,
    /// Organization ID for multi-tenant filtering.
    pub org_id: Option<String>,
    /// Recency half-life in hours for scoring. Controls how fast older memories lose relevance. Defaults to 168 (1 week).
    pub recency_half_life_hours: Option<f64>,
    /// Custom weights for hybrid RRF fusion. One weight per ranked list (vector, BM25, recency, graph). Defaults to uniform weights.
    pub hybrid_weights: Option<Vec<f32>>,
    /// Custom k parameter for RRF fusion. Higher k reduces the impact of rank differences. Defaults to 60.0.
    pub rrf_k: Option<f32>,
    /// Point-in-time query: show memory state as it existed at this timestamp (RFC 3339). Excludes memories created after this time and memories already deleted by this time.
    pub as_of: Option<String>,
    /// When true, each returned memory includes a `score_breakdown` object
    /// with the per-signal contributions (vector, bm25, graph, recency) and
    /// the final RRF rank.
    pub explain: Option<bool>,
    /// v0.4.7 — opt-in current-fact resolver. When set, the engine
    /// post-processes the recall result set: candidates sharing the
    /// same value under `fact_key` (a metadata JSON pointer the
    /// operator chose to scope fact identity by — typical
    /// convention is `"fact_id"`) are grouped, and only the
    /// most-recent write per group is kept. When
    /// `include_supersession_chain = true`, older fact-versions
    /// are returned in the response's `superseded` field. Anchored
    /// on arXiv:2605.18565 (MINTEval). Default `None` leaves the
    /// read path unchanged.
    pub current_fact_resolver: Option<RecallCurrentFactResolverInput>,
    /// v0.4.8 — opt-in orientation cache. When set AND the server
    /// has an `OrientationCacheStore` attached, the recall maintains
    /// a per-namespace, constant-token "context map" updated from
    /// each recall hit and returns a bounded rendering in the
    /// response's `orientation_cache` field. PEEK-anchored
    /// (arXiv:2605.19932). Default `None` leaves the read path
    /// unchanged.
    pub orientation_cache: Option<RecallOrientationCacheInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecallOrientationCacheInput {
    /// Operator-chosen namespace label. When omitted, the server
    /// derives one from `(org_id, agent_id)`, falling back to
    /// `"__global__"`.
    pub namespace: Option<String>,
    /// Maximum rendered tokens. Defaults to 512.
    pub token_budget: Option<u32>,
    /// When `true` (default), the rendered map is returned in the
    /// response. Set to `false` to update the in-process store
    /// without growing the response payload (warm-up calls).
    pub include_in_response: Option<bool>,
    /// When `true` (default), the Distiller runs over recall hits
    /// and updates the in-process map. Set to `false` for
    /// read-only inspection.
    pub distill: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecallCurrentFactResolverInput {
    /// JSON metadata key used to group candidates by fact identity.
    /// Operator-chosen — typical convention is `"fact_id"`. Records
    /// missing this key are passed through untouched.
    pub fact_key: String,
    /// When `true`, the response carries the older fact-versions
    /// in its `superseded` field. Defaults to `false`.
    pub include_supersession_chain: Option<bool>,
}
