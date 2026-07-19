use thiserror::Error;

/// Errors surfaced by the compliance primitives.
#[derive(Debug, Error)]
pub enum ComplianceError {
    /// The configured [`ConsentSource`](crate::ConsentSource) says the
    /// subject has not granted the scope this write requires.
    #[error("consent denied for subject '{subject_id}' on scope '{scope}'")]
    ConsentDenied { subject_id: String, scope: String },

    /// The consent manager returned a malformed or expired payload.
    #[error("invalid consent state from source: {0}")]
    InvalidConsent(String),

    /// The consent manager was unreachable.
    #[error("consent source transport error: {0}")]
    Transport(String),

    /// The requested audit-log window produced no events.
    #[error("no events in requested audit window")]
    EmptyAuditWindow,

    /// Ed25519 signing or key-handling failure.
    #[error("signature error: {0}")]
    Signature(String),

    /// Serialization/IO glue failure.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Audit-log chain verification detected a tampered row.
    #[error("audit chain broken at record {index}: {reason}")]
    ChainBroken { index: usize, reason: String },

    /// A retention-conformance profile requires an append-only `agent_events`
    /// log to honour its retention floor, but the active storage backend does
    /// not guarantee that. Returned — never a silent pass — so a caller sees
    /// that the floor cannot be met on this backend. Mirrors
    /// `mnemo_core::error::Error::EmbedderNotConfigured { requested, backend }`.
    #[error(
        "retention floor of {floor_days} days cannot be honoured by backend '{backend}': \
         the backend does not guarantee an append-only agent_events log \
         (StorageBackend::events_are_append_only() == false)"
    )]
    RetentionFloorUnsupported { backend: String, floor_days: u32 },

    /// An event timestamp could not be parsed as RFC3339, so its age relative
    /// to the retention floor cannot be evaluated.
    #[error("unparseable event timestamp '{timestamp}' for event {event_id}: {reason}")]
    UnparseableTimestamp {
        event_id: String,
        timestamp: String,
        reason: String,
    },
}

impl From<serde_json::Error> for ComplianceError {
    fn from(e: serde_json::Error) -> Self {
        ComplianceError::Serialization(e.to_string())
    }
}
