use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// MCP tool input for `mnemo.trajectory_audit`. Mirrors the
/// (`agent_id`, `thread_id`) shape of `mnemo.verify` and adds the
/// trajectory-specific tunables documented in
/// [`mnemo_compliance::trajectory::TrajectoryAuditRequest`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TrajectoryAuditInput {
    /// Agent scope to audit. Uses the engine default when omitted.
    pub agent_id: Option<String>,
    /// Optional thread scope (matches `mnemo.verify`).
    pub thread_id: Option<String>,
    /// Active-bank-size ceiling for signal (a) unregulated-growth.
    /// Defaults to `1024`.
    pub active_bank_ceiling: Option<usize>,
    /// Payload key used by signal (b) missing-semantic-revision to
    /// detect supersession. Defaults to `"fact_id"`.
    pub fact_key: Option<String>,
    /// Forget strategies considered policy-driven by signal (c).
    /// Defaults to the five canonical strategies: `soft_delete`,
    /// `hard_delete`, `decay`, `consolidate`, `archive`.
    pub named_forget_strategies: Option<Vec<String>>,
}
