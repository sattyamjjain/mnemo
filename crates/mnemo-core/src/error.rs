use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("storage error: {message}")]
    StorageSource {
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("index error: {0}")]
    Index(String),

    /// A storage/index backend does not implement a requested capability.
    /// Returned (not silently empty) when a caller asks a backend to do
    /// something it cannot — e.g. semantic/vector recall on the PostgreSQL
    /// backend, whose pgvector ANN search is not wired. `detail` carries the
    /// actionable guidance (which backend to use + a tracking link).
    #[error("backend '{backend}' does not support capability '{capability}': {detail}")]
    BackendUnsupported {
        backend: String,
        capability: String,
        detail: String,
    },

    /// A semantic/vector recall was requested, but the configured embedder
    /// cannot produce query vectors — the no-op embedder (which returns
    /// all-zero vectors) or none configured. Returned instead of a silent
    /// empty result set so the caller sees the misconfiguration. `requested`
    /// is the recall strategy; `backend` is the storage backend in use.
    #[error(
        "embedder not configured for '{requested}' recall on backend '{backend}': \
         semantic recall requires a configured embedder (OpenAI HTTP or local ONNX); \
         the noop embedder returns no vectors — refusing to silently return empty"
    )]
    EmbedderNotConfigured { requested: String, backend: String },

    #[error("index error: {message}")]
    IndexSource {
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("embedding error: {message}")]
    EmbeddingSource {
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<duckdb::Error> for Error {
    fn from(e: duckdb::Error) -> Self {
        Error::Storage(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Internal(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Embedding(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
