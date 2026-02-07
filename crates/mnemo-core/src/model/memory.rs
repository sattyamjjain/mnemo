use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRecord {
    pub id: Uuid,
    pub agent_id: String,
    pub content: String,
    pub memory_type: MemoryType,
    pub scope: Scope,
    pub importance: f32,
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
    pub embedding: Option<Vec<f32>>,
    pub content_hash: Vec<u8>,
    pub prev_hash: Option<Vec<u8>>,
    pub source_type: SourceType,
    pub source_id: Option<String>,
    pub consolidation_state: ConsolidationState,
    pub access_count: u64,
    pub org_id: Option<String>,
    pub thread_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_accessed_at: Option<String>,
    pub expires_at: Option<String>,
    pub deleted_at: Option<String>,
    pub decay_rate: Option<f32>,
    pub created_by: Option<String>,
    pub version: u32,
    pub prev_version_id: Option<uuid::Uuid>,
    pub quarantined: bool,
    pub quarantine_reason: Option<String>,
    pub decay_function: Option<String>,
}

impl MemoryRecord {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Episodic,
    Semantic,
    Procedural,
    Working,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Episodic => write!(f, "episodic"),
            MemoryType::Semantic => write!(f, "semantic"),
            MemoryType::Procedural => write!(f, "procedural"),
            MemoryType::Working => write!(f, "working"),
        }
    }
}

impl std::str::FromStr for MemoryType {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "episodic" => Ok(MemoryType::Episodic),
            "semantic" => Ok(MemoryType::Semantic),
            "procedural" => Ok(MemoryType::Procedural),
            "working" => Ok(MemoryType::Working),
            _ => Err(crate::error::Error::Validation(format!(
                "invalid memory type: {s}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Private,
    Shared,
    Public,
    Global,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Private => write!(f, "private"),
            Scope::Shared => write!(f, "shared"),
            Scope::Public => write!(f, "public"),
            Scope::Global => write!(f, "global"),
        }
    }
}

impl std::str::FromStr for Scope {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "private" => Ok(Scope::Private),
            "shared" => Ok(Scope::Shared),
            "public" => Ok(Scope::Public),
            "global" => Ok(Scope::Global),
            _ => Err(crate::error::Error::Validation(format!(
                "invalid scope: {s}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsolidationState {
    Raw,
    Active,
    Pending,
    Consolidated,
    Archived,
    Forgotten,
}

impl std::fmt::Display for ConsolidationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsolidationState::Raw => write!(f, "raw"),
            ConsolidationState::Active => write!(f, "active"),
            ConsolidationState::Pending => write!(f, "pending"),
            ConsolidationState::Consolidated => write!(f, "consolidated"),
            ConsolidationState::Archived => write!(f, "archived"),
            ConsolidationState::Forgotten => write!(f, "forgotten"),
        }
    }
}

impl std::str::FromStr for ConsolidationState {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "raw" => Ok(ConsolidationState::Raw),
            "active" => Ok(ConsolidationState::Active),
            "pending" => Ok(ConsolidationState::Pending),
            "consolidated" => Ok(ConsolidationState::Consolidated),
            "archived" => Ok(ConsolidationState::Archived),
            "forgotten" => Ok(ConsolidationState::Forgotten),
            _ => Err(crate::error::Error::Validation(format!(
                "invalid consolidation state: {s}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Agent,
    Human,
    System,
    UserInput,
    ToolOutput,
    ModelResponse,
    Retrieval,
    Consolidation,
    Import,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Agent => write!(f, "agent"),
            SourceType::Human => write!(f, "human"),
            SourceType::System => write!(f, "system"),
            SourceType::UserInput => write!(f, "user_input"),
            SourceType::ToolOutput => write!(f, "tool_output"),
            SourceType::ModelResponse => write!(f, "model_response"),
            SourceType::Retrieval => write!(f, "retrieval"),
            SourceType::Consolidation => write!(f, "consolidation"),
            SourceType::Import => write!(f, "import"),
        }
    }
}

impl std::str::FromStr for SourceType {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "agent" => Ok(SourceType::Agent),
            "human" => Ok(SourceType::Human),
            "system" => Ok(SourceType::System),
            "user_input" => Ok(SourceType::UserInput),
            "tool_output" => Ok(SourceType::ToolOutput),
            "model_response" => Ok(SourceType::ModelResponse),
            "retrieval" => Ok(SourceType::Retrieval),
            "consolidation" => Ok(SourceType::Consolidation),
            "import" => Ok(SourceType::Import),
            _ => Err(crate::error::Error::Validation(format!(
                "invalid source type: {s}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_record() -> MemoryRecord {
        MemoryRecord {
            id: Uuid::now_v7(),
            agent_id: "agent-1".to_string(),
            content: "The user prefers dark mode".to_string(),
            memory_type: MemoryType::Semantic,
            scope: Scope::Private,
            importance: 0.8,
            tags: vec!["preference".to_string(), "ui".to_string()],
            metadata: serde_json::json!({"source": "conversation"}),
            embedding: None,
            content_hash: vec![1, 2, 3],
            prev_hash: None,
            source_type: SourceType::Agent,
            source_id: None,
            consolidation_state: ConsolidationState::Raw,
            access_count: 0,
            org_id: None,
            thread_id: None,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
            last_accessed_at: None,
            expires_at: None,
            deleted_at: None,
            decay_rate: None,
            created_by: None,
            version: 1,
            prev_version_id: None,
            quarantined: false,
            quarantine_reason: None,
            decay_function: None,
        }
    }

    #[test]
    fn test_serde_roundtrip() {
        let record = sample_record();
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: MemoryRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_enum_serde() {
        assert_eq!(
            serde_json::to_string(&MemoryType::Episodic).unwrap(),
            "\"episodic\""
        );
        assert_eq!(
            serde_json::to_string(&Scope::Private).unwrap(),
            "\"private\""
        );
        assert_eq!(
            serde_json::to_string(&SourceType::Agent).unwrap(),
            "\"agent\""
        );
        assert_eq!(
            serde_json::to_string(&ConsolidationState::Raw).unwrap(),
            "\"raw\""
        );
    }

    #[test]
    fn test_is_deleted() {
        let mut record = sample_record();
        assert!(!record.is_deleted());
        record.deleted_at = Some("2025-01-02T00:00:00Z".to_string());
        assert!(record.is_deleted());
    }

    #[test]
    fn test_enum_fromstr() {
        assert_eq!("episodic".parse::<MemoryType>().unwrap(), MemoryType::Episodic);
        assert_eq!("working".parse::<MemoryType>().unwrap(), MemoryType::Working);
        assert_eq!("shared".parse::<Scope>().unwrap(), Scope::Shared);
        assert_eq!("human".parse::<SourceType>().unwrap(), SourceType::Human);
        assert_eq!("active".parse::<ConsolidationState>().unwrap(), ConsolidationState::Active);
        assert_eq!("pending".parse::<ConsolidationState>().unwrap(), ConsolidationState::Pending);
        assert_eq!("forgotten".parse::<ConsolidationState>().unwrap(), ConsolidationState::Forgotten);
        assert!("invalid".parse::<MemoryType>().is_err());
    }

    #[test]
    fn test_extended_enums_parse() {
        // New SourceType variants
        assert_eq!("user_input".parse::<SourceType>().unwrap(), SourceType::UserInput);
        assert_eq!("tool_output".parse::<SourceType>().unwrap(), SourceType::ToolOutput);
        assert_eq!("model_response".parse::<SourceType>().unwrap(), SourceType::ModelResponse);
        assert_eq!("retrieval".parse::<SourceType>().unwrap(), SourceType::Retrieval);
        assert_eq!("consolidation".parse::<SourceType>().unwrap(), SourceType::Consolidation);
        assert_eq!("import".parse::<SourceType>().unwrap(), SourceType::Import);

        // New Scope variant
        assert_eq!("global".parse::<Scope>().unwrap(), Scope::Global);

        // Verify display roundtrip
        assert_eq!(SourceType::UserInput.to_string(), "user_input");
        assert_eq!(SourceType::Import.to_string(), "import");
        assert_eq!(Scope::Global.to_string(), "global");
    }
}
