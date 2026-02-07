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
}
