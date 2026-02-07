use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Checkpoint {
    pub id: Uuid,
    pub thread_id: String,
    pub agent_id: String,
    pub parent_id: Option<Uuid>,
    pub branch_name: String,
    pub state_snapshot: serde_json::Value,
    pub state_diff: Option<serde_json::Value>,
    pub memory_refs: Vec<Uuid>,
    pub event_cursor: Option<Uuid>,
    pub label: Option<String>,
    pub created_at: String,
    pub metadata: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_serde() {
        let cp = Checkpoint {
            id: Uuid::now_v7(),
            thread_id: "thread-1".to_string(),
            agent_id: "agent-1".to_string(),
            parent_id: None,
            branch_name: "main".to_string(),
            state_snapshot: serde_json::json!({"step": 1}),
            state_diff: None,
            memory_refs: vec![Uuid::now_v7()],
            event_cursor: None,
            label: Some("initial".to_string()),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_string(&cp).unwrap();
        let deserialized: Checkpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(cp, deserialized);
    }
}
