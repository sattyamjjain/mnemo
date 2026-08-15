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
    /// Lease token from a preceding `mnemo.recall`, when the server has
    /// capability-leased reads enabled (#126).
    ///
    /// Required in that configuration and ignored otherwise. The lease must be
    /// unexpired, name the `forget_subject` scope, and be bound to the calling
    /// principal — which is what ties this destructive act to a read the same
    /// caller just performed, breaking the exfiltrate-then-act chain.
    pub lease_token: Option<String>,
}
