//! Hard guard: a benchmark must never emit a score under a no-op embedder.
//!
//! A silently-`NoopEmbedding` retrieval benchmark reports semantic recall it did
//! not actually perform (the no-op embedder returns all-zero vectors) — worse
//! than publishing no number at all. Every real-embedder bench routes its
//! resolved embedder through [`guard_real_embedder`] before scoring; the runner
//! refuses to continue if the embedder is not semantic-capable.

use mnemo_core::embedding::EmbeddingProvider;

/// Error returned when the resolved embedder cannot back a semantic benchmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoopBenchmarkRefused {
    /// The backend label the runner tried to use (e.g. `"onnx"`, `"noop"`).
    pub backend: String,
    /// The concrete embedder type name that resolved.
    pub embedder_type: &'static str,
}

impl std::fmt::Display for NoopBenchmarkRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "refusing to emit a benchmark score: the resolved embedder (backend='{}', \
             type='{}') is NOT semantic-capable — it returns all-zero vectors (a no-op / \
             degenerate implementation). A NoopEmbedding benchmark is worse than no benchmark. \
             Configure a real embedder: ONNX (default, set MNEMO_ONNX_MODEL_PATH and build with \
             --features onnx), OpenAI (OPENAI_API_KEY), or Ollama.",
            self.backend, self.embedder_type
        )
    }
}

impl std::error::Error for NoopBenchmarkRefused {}

/// Refuse to proceed unless `embedding` is a real, semantic-capable provider.
///
/// Keyed on [`EmbeddingProvider::is_semantic_capable`] — `false` only for the
/// no-op zero-vector provider (real providers: ONNX, OpenAI, Ollama, the
/// deterministic hashing embedder all return `true`).
///
/// Generic over `E: ?Sized` so that a **concrete** `&NoopEmbedding` reports its
/// real type name in the error (`type_name_of_val` through a `&dyn Trait` would
/// only ever yield `"dyn …EmbeddingProvider"`); the resolver's `&dyn` path still
/// works — there the name is diagnostic only and a real embedder never trips it.
pub fn guard_real_embedder<E: EmbeddingProvider + ?Sized>(
    embedding: &E,
    backend: &str,
) -> Result<(), NoopBenchmarkRefused> {
    if embedding.is_semantic_capable() {
        Ok(())
    } else {
        Err(NoopBenchmarkRefused {
            backend: backend.to_string(),
            embedder_type: std::any::type_name_of_val(embedding),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnemo_core::embedding::{DeterministicEmbedding, NoopEmbedding};

    #[test]
    fn refuses_noop_embedder() {
        let noop = NoopEmbedding::new(384);
        let err = guard_real_embedder(&noop, "noop").unwrap_err();
        assert_eq!(err.backend, "noop");
        assert!(err.embedder_type.contains("Noop"));
        assert!(err.to_string().contains("worse than no benchmark"));
    }

    #[test]
    fn accepts_real_embedder() {
        // A deterministic (real, non-zero) embedder stands in for ONNX/OpenAI.
        let real = DeterministicEmbedding::new(384);
        assert!(guard_real_embedder(&real, "deterministic").is_ok());
    }
}
