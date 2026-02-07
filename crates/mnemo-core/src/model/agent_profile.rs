use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentProfile {
    pub agent_id: String,
    pub avg_importance: f64,
    pub avg_content_length: f64,
    pub total_memories: u64,
    pub last_updated: String,
}
