use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ForgetCriteriaInput {
    /// Maximum age in hours. Memories older than this will be affected.
    pub max_age_hours: Option<f64>,
    /// Importance threshold. Memories with importance below this will be affected.
    pub min_importance_below: Option<f32>,
    /// Filter by memory type.
    pub memory_type: Option<String>,
    /// Filter by tags.
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ForgetInput {
    /// List of memory IDs to forget/delete. Can be empty if criteria is specified.
    pub memory_ids: Vec<String>,
    /// Delete strategy: "soft_delete" (mark as deleted, recoverable), "hard_delete" (permanent), "decay" (reduce importance), "archive" (mark as archived), or "consolidate" (mark as consolidated). Defaults to "soft_delete".
    pub strategy: Option<String>,
    /// Criteria-based forget: find and apply strategy to memories matching these filters. Used when memory_ids is empty.
    pub criteria: Option<ForgetCriteriaInput>,
}
