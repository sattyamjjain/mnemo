use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RememberInput {
    /// The content to remember. This can be a fact, preference, instruction, or any text.
    pub content: String,
    /// The type of memory: "episodic" (events/experiences), "semantic" (facts/knowledge), "procedural" (how-to/instructions), or "working" (temporary/active). Defaults to "episodic".
    pub memory_type: Option<String>,
    /// Visibility scope: "private" (only this agent), "shared" (specific agents), or "public" (all agents). Defaults to "private".
    pub scope: Option<String>,
    /// Importance score from 0.0 to 1.0. Higher values indicate more important memories. Defaults to 0.5.
    pub importance: Option<f32>,
    /// Tags for categorizing and filtering memories.
    pub tags: Option<Vec<String>>,
    /// Additional metadata as key-value pairs.
    pub metadata: Option<serde_json::Value>,
    /// Time-to-live in seconds. The memory will expire after this duration.
    pub ttl_seconds: Option<u64>,
    /// List of memory IDs that this memory is related to.
    pub related_to: Option<Vec<String>>,
    /// Thread ID for grouping related memories in a conversation thread.
    pub thread_id: Option<String>,
    /// Source type: "agent", "human", "system", "user_input", "tool_output", "model_response", "retrieval", "consolidation", or "import".
    pub source_type: Option<String>,
    /// Source identifier (e.g., the tool name or user ID that produced this memory).
    pub source_id: Option<String>,
    /// Organization ID for multi-tenant isolation.
    pub org_id: Option<String>,
    /// Custom decay rate (0.0 to 1.0) controlling how quickly this memory loses importance.
    pub decay_rate: Option<f32>,
    /// ID of the agent or user who created this memory.
    pub created_by: Option<String>,
}
