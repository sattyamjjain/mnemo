use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ForgetSubjectInput {
    /// Subject identifier whose memories should be erased. Memories are
    /// matched by the tag `subject:<subject_id>`.
    pub subject_id: String,
    /// Erasure strategy: "redact" (overwrite content but preserve hash
    /// chain for audit) or "hard_delete" (permanent removal). Defaults
    /// to "redact" since it preserves verifiability.
    pub strategy: Option<String>,
    /// Optional agent scope; defaults to the server's default agent id.
    pub agent_id: Option<String>,
}
