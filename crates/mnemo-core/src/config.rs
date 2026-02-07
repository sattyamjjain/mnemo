use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MnemoConfig {
    pub db_path: PathBuf,
    pub embedding_dimensions: usize,
    pub default_agent_id: String,
    pub default_org_id: Option<String>,
    pub openai_api_key: Option<String>,
    pub embedding_model: String,
}

impl Default for MnemoConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("mnemo.db"),
            embedding_dimensions: 1536,
            default_agent_id: "default".to_string(),
            default_org_id: None,
            openai_api_key: None,
            embedding_model: "text-embedding-3-small".to_string(),
        }
    }
}
