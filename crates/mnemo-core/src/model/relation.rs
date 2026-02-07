use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Relation {
    pub id: Uuid,
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub relation_type: String,
    pub weight: f32,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_serde_roundtrip() {
        let relation = Relation {
            id: Uuid::now_v7(),
            source_id: Uuid::now_v7(),
            target_id: Uuid::now_v7(),
            relation_type: "related_to".to_string(),
            weight: 0.9,
            metadata: serde_json::json!({}),
            created_at: "2025-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&relation).unwrap();
        let deserialized: Relation = serde_json::from_str(&json).unwrap();
        assert_eq!(relation, deserialized);
    }
}
