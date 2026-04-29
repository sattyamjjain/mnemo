//! Rolling per-agent profile (v0.4.1 P0-3).

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub type ToolId = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentBaseline {
    pub agent: String,
    /// Window the rolling rates cover (e.g. 5 minutes).
    pub window: Duration,
    pub recall_rate_per_min: f32,
    pub write_rate_per_min: f32,
    /// How many distinct namespaces this agent touched per minute.
    /// Spike → possible cross-tenant scan.
    pub namespace_fanout: f32,
    /// Per-tool fraction of total ops. Sums to ~1.0.
    pub tool_mix: HashMap<ToolId, f32>,
    /// Fraction of audit rows whose `prev_hash` matched the running
    /// chain head. 1.0 = perfect; <1.0 = HMAC chain has been
    /// tampered with or replayed.
    pub hmac_continuity: f32,
}

impl AgentBaseline {
    pub fn new(agent: impl Into<String>, window: Duration) -> Self {
        Self {
            agent: agent.into(),
            window,
            recall_rate_per_min: 0.0,
            write_rate_per_min: 0.0,
            namespace_fanout: 0.0,
            tool_mix: HashMap::new(),
            hmac_continuity: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_baseline_starts_at_zero() {
        let b = AgentBaseline::new("agent-1", Duration::from_secs(300));
        assert_eq!(b.agent, "agent-1");
        assert_eq!(b.recall_rate_per_min, 0.0);
        assert_eq!(b.hmac_continuity, 1.0);
    }
}
