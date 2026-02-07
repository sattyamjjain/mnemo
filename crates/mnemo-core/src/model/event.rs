use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentEvent {
    pub id: Uuid,
    pub agent_id: String,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub parent_event_id: Option<Uuid>,
    pub event_type: EventType,
    pub payload: serde_json::Value,
    // OTel fields
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub model: Option<String>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
    pub latency_ms: Option<i64>,
    pub cost_usd: Option<f64>,
    // Temporal
    pub timestamp: String,
    pub logical_clock: i64,
    // Integrity
    pub content_hash: Vec<u8>,
    pub prev_hash: Option<Vec<u8>>,
    // Optional embedding of the event payload
    pub embedding: Option<Vec<f32>>,
}

impl AgentEvent {
    /// Create a new `AgentEvent` with required fields; all optional fields default to `None`.
    pub fn new(
        agent_id: String,
        event_type: EventType,
        payload: serde_json::Value,
        timestamp: String,
        content_hash: Vec<u8>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            agent_id,
            thread_id: None,
            run_id: None,
            parent_event_id: None,
            event_type,
            payload,
            trace_id: None,
            span_id: None,
            model: None,
            tokens_input: None,
            tokens_output: None,
            latency_ms: None,
            cost_usd: None,
            timestamp,
            logical_clock: 0,
            content_hash,
            prev_hash: None,
            embedding: None,
        }
    }

    /// Create an `AgentEvent` with all fields specified.
    /// Intended for storage backends that reconstruct events from database rows.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        id: Uuid,
        agent_id: String,
        thread_id: Option<String>,
        run_id: Option<String>,
        parent_event_id: Option<Uuid>,
        event_type: EventType,
        payload: serde_json::Value,
        trace_id: Option<String>,
        span_id: Option<String>,
        model: Option<String>,
        tokens_input: Option<i64>,
        tokens_output: Option<i64>,
        latency_ms: Option<i64>,
        cost_usd: Option<f64>,
        timestamp: String,
        logical_clock: i64,
        content_hash: Vec<u8>,
        prev_hash: Option<Vec<u8>>,
        embedding: Option<Vec<f32>>,
    ) -> Self {
        Self {
            id,
            agent_id,
            thread_id,
            run_id,
            parent_event_id,
            event_type,
            payload,
            trace_id,
            span_id,
            model,
            tokens_input,
            tokens_output,
            latency_ms,
            cost_usd,
            timestamp,
            logical_clock,
            content_hash,
            prev_hash,
            embedding,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    MemoryWrite,
    MemoryRead,
    MemoryDelete,
    MemoryShare,
    Checkpoint,
    Branch,
    Merge,
    UserMessage,
    AssistantMessage,
    ToolCall,
    ToolResult,
    Error,
    RetrievalQuery,
    RetrievalResult,
    Decision,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::MemoryWrite => write!(f, "memory_write"),
            EventType::MemoryRead => write!(f, "memory_read"),
            EventType::MemoryDelete => write!(f, "memory_delete"),
            EventType::MemoryShare => write!(f, "memory_share"),
            EventType::Checkpoint => write!(f, "checkpoint"),
            EventType::Branch => write!(f, "branch"),
            EventType::Merge => write!(f, "merge"),
            EventType::UserMessage => write!(f, "user_message"),
            EventType::AssistantMessage => write!(f, "assistant_message"),
            EventType::ToolCall => write!(f, "tool_call"),
            EventType::ToolResult => write!(f, "tool_result"),
            EventType::Error => write!(f, "error"),
            EventType::RetrievalQuery => write!(f, "retrieval_query"),
            EventType::RetrievalResult => write!(f, "retrieval_result"),
            EventType::Decision => write!(f, "decision"),
        }
    }
}

impl std::str::FromStr for EventType {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "memory_write" => Ok(EventType::MemoryWrite),
            "memory_read" => Ok(EventType::MemoryRead),
            "memory_delete" => Ok(EventType::MemoryDelete),
            "memory_share" => Ok(EventType::MemoryShare),
            "checkpoint" => Ok(EventType::Checkpoint),
            "branch" => Ok(EventType::Branch),
            "merge" => Ok(EventType::Merge),
            "user_message" => Ok(EventType::UserMessage),
            "assistant_message" => Ok(EventType::AssistantMessage),
            "tool_call" => Ok(EventType::ToolCall),
            "tool_result" => Ok(EventType::ToolResult),
            "error" => Ok(EventType::Error),
            "retrieval_query" => Ok(EventType::RetrievalQuery),
            "retrieval_result" => Ok(EventType::RetrievalResult),
            "decision" => Ok(EventType::Decision),
            _ => Err(crate::error::Error::Validation(format!(
                "invalid event type: {s}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_event_serde() {
        let event = AgentEvent {
            id: Uuid::now_v7(),
            agent_id: "agent-1".to_string(),
            thread_id: Some("thread-1".to_string()),
            run_id: None,
            parent_event_id: None,
            event_type: EventType::MemoryWrite,
            payload: serde_json::json!({"memory_id": "abc"}),
            trace_id: None,
            span_id: None,
            model: None,
            tokens_input: None,
            tokens_output: None,
            latency_ms: None,
            cost_usd: None,
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            logical_clock: 1,
            content_hash: vec![1, 2, 3],
            prev_hash: None,
            embedding: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: AgentEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_event_type_display_fromstr() {
        assert_eq!(EventType::MemoryWrite.to_string(), "memory_write");
        assert_eq!("memory_read".parse::<EventType>().unwrap(), EventType::MemoryRead);
        assert_eq!("checkpoint".parse::<EventType>().unwrap(), EventType::Checkpoint);
        assert_eq!("error".parse::<EventType>().unwrap(), EventType::Error);
        assert!("invalid".parse::<EventType>().is_err());
    }
}
