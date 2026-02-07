//! ONNX Runtime local embedding provider.
//!
//! Provides local embedding inference using ONNX Runtime, eliminating the
//! need for an external API. Supports sentence-transformer models such as
//! `all-MiniLM-L6-v2` exported to ONNX format.
//!
//! # Feature gating
//!
//! When compiled **without** the `onnx` feature the module provides a stub
//! that validates the model path but returns [`Error::Embedding`] from
//! `embed()` and `embed_batch()`.
//!
//! When compiled **with** the `onnx` feature the module loads the ONNX
//! session and a HuggingFace tokenizer, then performs real local inference
//! with mean-pooling and L2 normalisation.
//!
//! ```toml
//! [features]
//! onnx = ["dep:ort", "dep:tokenizers", "dep:ndarray"]
//!
//! [dependencies]
//! ort = { version = "2", optional = true }
//! tokenizers = { version = "0.21", optional = true, default-features = false }
//! ndarray = { version = "0.16", optional = true }
//! ```
//!
//! # Example (stub)
//!
//! ```rust,no_run
//! use mnemo_core::embedding::onnx::OnnxEmbedding;
//! use mnemo_core::embedding::EmbeddingProvider;
//!
//! // Will succeed only if the path exists on disk.
//! let provider = OnnxEmbedding::new("/models/all-MiniLM-L6-v2.onnx", 384)
//!     .expect("model path must exist");
//!
//! assert_eq!(provider.dimensions(), 384);
//! assert_eq!(provider.model_path(), "/models/all-MiniLM-L6-v2.onnx");
//! ```

use crate::embedding::EmbeddingProvider;
use crate::error::{Error, Result};

// ---------------------------------------------------------------------------
// Real implementation (feature = "onnx")
// ---------------------------------------------------------------------------
#[cfg(feature = "onnx")]
mod inner {
    use super::*;
    use ndarray::Array2;
    use ort::Session;
    use std::path::Path;
    use std::sync::Arc;
    use tokenizers::Tokenizer;

    /// ONNX-based local embedding provider.
    ///
    /// Wraps an ONNX sentence-transformer model (e.g. `all-MiniLM-L6-v2`)
    /// together with a HuggingFace tokenizer for on-device vector generation.
    pub struct OnnxEmbedding {
        dimensions: usize,
        model_path: String,
        session: Arc<Session>,
        tokenizer: Arc<Tokenizer>,
    }

    // `ort::Session` is Send + Sync in ort v2.
    // `tokenizers::Tokenizer` is Send + Sync.
    // The Arc wrappers enable cheap cloning for spawn_blocking moves.

    // Manual Debug because Session/Tokenizer do not implement Debug.
    impl std::fmt::Debug for OnnxEmbedding {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("OnnxEmbedding")
                .field("dimensions", &self.dimensions)
                .field("model_path", &self.model_path)
                .finish_non_exhaustive()
        }
    }

    impl OnnxEmbedding {
        /// Create a new ONNX embedding provider from a model path.
        ///
        /// The model should be an ONNX file for a sentence-transformer model
        /// (e.g. `all-MiniLM-L6-v2` exported to ONNX format).
        ///
        /// A `tokenizer.json` file **must** exist in the same directory as the
        /// model file. This is the standard layout produced by
        /// `optimum-cli export onnx` or manual HuggingFace model export.
        ///
        /// # Errors
        ///
        /// Returns [`Error::Validation`] if the model file does not exist.
        /// Returns [`Error::Embedding`] if the ONNX session or tokenizer
        /// fails to load.
        pub fn new(model_path: &str, dimensions: usize) -> Result<Self> {
            let model = Path::new(model_path);
            if !model.exists() {
                return Err(Error::Validation(format!(
                    "ONNX model not found at: {model_path}"
                )));
            }

            // Locate tokenizer.json next to the model file.
            let tokenizer_path = model
                .parent()
                .map(|p| p.join("tokenizer.json"))
                .unwrap_or_else(|| Path::new("tokenizer.json").to_path_buf());

            if !tokenizer_path.exists() {
                return Err(Error::Embedding(format!(
                    "tokenizer.json not found next to ONNX model (expected at {})",
                    tokenizer_path.display()
                )));
            }

            let session = Session::builder()
                .map_err(|e| Error::Embedding(format!("failed to create ONNX session builder: {e}")))?
                .with_intra_threads(4)
                .map_err(|e| Error::Embedding(format!("failed to set intra threads: {e}")))?
                .commit_from_file(model_path)
                .map_err(|e| Error::Embedding(format!("failed to load ONNX model: {e}")))?;

            let tokenizer = Tokenizer::from_file(&tokenizer_path)
                .map_err(|e| Error::Embedding(format!("failed to load tokenizer: {e}")))?;

            Ok(Self {
                dimensions,
                model_path: model_path.to_string(),
                session: Arc::new(session),
                tokenizer: Arc::new(tokenizer),
            })
        }

        /// Get the model path.
        #[must_use]
        pub fn model_path(&self) -> &str {
            &self.model_path
        }

        /// Tokenize a batch of texts and return (input_ids, attention_mask,
        /// token_type_ids) as 2-D i64 arrays with shape `[batch, max_len]`.
        fn tokenize_batch(
            tokenizer: &Tokenizer,
            texts: &[&str],
        ) -> Result<(Array2<i64>, Array2<i64>, Array2<i64>)> {
            let encodings = tokenizer
                .encode_batch(texts.to_vec(), true)
                .map_err(|e| Error::Embedding(format!("tokenization failed: {e}")))?;

            let batch_size = encodings.len();
            let max_len = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(0);

            let mut input_ids = Array2::<i64>::zeros((batch_size, max_len));
            let mut attention_mask = Array2::<i64>::zeros((batch_size, max_len));
            let mut token_type_ids = Array2::<i64>::zeros((batch_size, max_len));

            for (i, enc) in encodings.iter().enumerate() {
                for (j, &id) in enc.get_ids().iter().enumerate() {
                    input_ids[[i, j]] = i64::from(id);
                }
                for (j, &mask) in enc.get_attention_mask().iter().enumerate() {
                    attention_mask[[i, j]] = i64::from(mask);
                }
                for (j, &tid) in enc.get_type_ids().iter().enumerate() {
                    token_type_ids[[i, j]] = i64::from(tid);
                }
            }

            Ok((input_ids, attention_mask, token_type_ids))
        }

        /// Mean-pool the last hidden state over the token dimension, weighted
        /// by the attention mask, then L2-normalise each vector.
        fn mean_pool_and_normalize(
            hidden: &Array2<f32>,
            mask: &Array2<i64>,
            batch_size: usize,
            seq_len: usize,
            hidden_dim: usize,
        ) -> Vec<Vec<f32>> {
            // hidden shape: [batch * seq_len, hidden_dim] (flattened) OR
            // we receive it already as [batch, hidden_dim] after manual pooling.
            // We handle the [batch, seq_len, hidden_dim] case by reshaping.
            let _ = seq_len; // used only for the assertion below

            let mut results = Vec::with_capacity(batch_size);

            for i in 0..batch_size {
                let mut pooled = vec![0.0f32; hidden_dim];
                let mut count = 0.0f32;

                for j in 0..seq_len {
                    let m = mask[[i, j]] as f32;
                    if m > 0.0 {
                        for k in 0..hidden_dim {
                            pooled[k] += hidden[[i * seq_len + j, k]] * m;
                        }
                        count += m;
                    }
                }

                if count > 0.0 {
                    for v in &mut pooled {
                        *v /= count;
                    }
                }

                // L2 normalise
                let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for v in &mut pooled {
                        *v /= norm;
                    }
                }

                results.push(pooled);
            }

            results
        }

        /// Run inference on a batch of texts. This is the shared
        /// implementation used by both `embed` and `embed_batch`.
        async fn run_inference(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }

            let session = Arc::clone(&self.session);
            let tokenizer = Arc::clone(&self.tokenizer);
            let dims = self.dimensions;
            let owned_texts: Vec<String> = texts.iter().map(|t| (*t).to_string()).collect();

            let result = tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>> {
                let text_refs: Vec<&str> = owned_texts.iter().map(String::as_str).collect();
                let (input_ids, attention_mask, token_type_ids) =
                    Self::tokenize_batch(&tokenizer, &text_refs)?;

                let batch_size = input_ids.nrows();
                let seq_len = input_ids.ncols();

                let outputs = session
                    .run(ort::inputs![
                        "input_ids" => input_ids.view(),
                        "attention_mask" => attention_mask.view(),
                        "token_type_ids" => token_type_ids.view(),
                    ].map_err(|e| Error::Embedding(format!("failed to create inputs: {e}")))?)
                    .map_err(|e| Error::Embedding(format!("ONNX inference failed: {e}")))?;

                // Sentence-transformer models typically output
                // "last_hidden_state" at index 0 with shape
                // [batch, seq_len, hidden_dim].
                let output_tensor = outputs
                    .get("last_hidden_state")
                    .or_else(|| outputs.iter().next().map(|(_, v)| v))
                    .ok_or_else(|| Error::Embedding("no output tensor from ONNX model".to_string()))?;

                let output_array = output_tensor
                    .try_extract_tensor::<f32>()
                    .map_err(|e| Error::Embedding(format!("failed to extract output tensor: {e}")))?;

                let shape = output_array.shape();

                // Handle different output shapes:
                // - [batch, seq_len, hidden_dim]: needs mean-pooling
                // - [batch, hidden_dim]: already pooled (e.g. sentence_embedding output)
                if shape.len() == 3 {
                    let hidden_dim = shape[2];
                    if hidden_dim != dims {
                        return Err(Error::Embedding(format!(
                            "model hidden dim ({hidden_dim}) does not match configured dimensions ({dims})"
                        )));
                    }

                    // Reshape to [batch * seq_len, hidden_dim] for pooling
                    let flat = output_array
                        .to_shape((batch_size * seq_len, hidden_dim))
                        .map_err(|e| Error::Embedding(format!("reshape failed: {e}")))?;

                    let flat_owned: Array2<f32> = flat.to_owned();
                    Ok(Self::mean_pool_and_normalize(
                        &flat_owned,
                        &attention_mask,
                        batch_size,
                        seq_len,
                        hidden_dim,
                    ))
                } else if shape.len() == 2 {
                    // Already pooled output [batch, hidden_dim]
                    let hidden_dim = shape[1];
                    if hidden_dim != dims {
                        return Err(Error::Embedding(format!(
                            "model hidden dim ({hidden_dim}) does not match configured dimensions ({dims})"
                        )));
                    }

                    let mut results = Vec::with_capacity(batch_size);
                    for i in 0..batch_size {
                        let mut vec = Vec::with_capacity(hidden_dim);
                        for j in 0..hidden_dim {
                            vec.push(output_array[[i, j]]);
                        }
                        // L2 normalise
                        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                        if norm > 0.0 {
                            for v in &mut vec {
                                *v /= norm;
                            }
                        }
                        results.push(vec);
                    }
                    Ok(results)
                } else {
                    Err(Error::Embedding(format!(
                        "unexpected output tensor shape: {shape:?}"
                    )))
                }
            })
            .await
            .map_err(|e| Error::Embedding(format!("inference task panicked: {e}")))?;

            result
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for OnnxEmbedding {
        /// Generate an embedding vector for a single text input.
        ///
        /// Tokenizes the input, runs ONNX inference, applies mean-pooling
        /// weighted by the attention mask, and L2-normalises the result.
        ///
        /// # Errors
        ///
        /// Returns [`Error::Embedding`] if tokenization or inference fails.
        async fn embed(&self, text: &str) -> Result<Vec<f32>> {
            let mut results = self.run_inference(&[text]).await?;
            results
                .pop()
                .ok_or_else(|| Error::Embedding("empty inference result".to_string()))
        }

        /// Generate embedding vectors for a batch of text inputs.
        ///
        /// Processes all texts in a single batched ONNX inference call for
        /// maximum throughput.
        ///
        /// # Errors
        ///
        /// Returns [`Error::Embedding`] if tokenization or inference fails.
        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            self.run_inference(texts).await
        }

        fn dimensions(&self) -> usize {
            self.dimensions
        }
    }
}

// ---------------------------------------------------------------------------
// Stub implementation (no onnx feature)
// ---------------------------------------------------------------------------
#[cfg(not(feature = "onnx"))]
mod inner {
    use super::*;

    /// ONNX-based local embedding provider.
    ///
    /// Wraps an ONNX sentence-transformer model for on-device vector generation.
    /// When the `onnx` feature is not enabled, `embed` and `embed_batch` return
    /// an [`Error::Embedding`] explaining how to enable full inference.
    #[derive(Debug)]
    pub struct OnnxEmbedding {
        dimensions: usize,
        model_path: String,
        // In a full implementation, this would hold:
        // session: ort::Session,
        // tokenizer: tokenizers::Tokenizer,
    }

    impl OnnxEmbedding {
        /// Create a new ONNX embedding provider from a model path.
        ///
        /// The model should be an ONNX sentence-transformer model
        /// (e.g., `all-MiniLM-L6-v2` exported to ONNX format).
        ///
        /// # Errors
        ///
        /// Returns [`Error::Validation`] if the file at `model_path` does not
        /// exist on disk.
        pub fn new(model_path: &str, dimensions: usize) -> Result<Self> {
            if !std::path::Path::new(model_path).exists() {
                return Err(Error::Validation(format!(
                    "ONNX model not found at: {model_path}"
                )));
            }
            Ok(Self {
                dimensions,
                model_path: model_path.to_string(),
            })
        }

        /// Get the model path.
        #[must_use]
        pub fn model_path(&self) -> &str {
            &self.model_path
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for OnnxEmbedding {
        /// Generate an embedding vector for a single text input.
        ///
        /// # Errors
        ///
        /// Currently returns [`Error::Embedding`] because full ONNX Runtime
        /// inference requires the `onnx` feature (with `ort`, `tokenizers`,
        /// and `ndarray` crates).
        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Err(Error::Embedding(
                "ONNX Runtime not available: compile with full onnx dependencies \
                 (ort, tokenizers, ndarray) to enable local inference"
                    .to_string(),
            ))
        }

        /// Generate embedding vectors for a batch of text inputs.
        ///
        /// # Errors
        ///
        /// Currently returns [`Error::Embedding`] because full ONNX Runtime
        /// inference requires the `onnx` feature (with `ort`, `tokenizers`,
        /// and `ndarray` crates).
        async fn embed_batch(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            Err(Error::Embedding(
                "ONNX Runtime not available: compile with full onnx dependencies \
                 (ort, tokenizers, ndarray) to enable local inference"
                    .to_string(),
            ))
        }

        fn dimensions(&self) -> usize {
            self.dimensions
        }
    }
}

// Re-export `OnnxEmbedding` from the active inner module so that
// downstream code can use `crate::embedding::onnx::OnnxEmbedding`
// regardless of the feature flag.
pub use inner::OnnxEmbedding;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onnx_missing_model() {
        let result = OnnxEmbedding::new("/nonexistent/path/model.onnx", 384);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("ONNX model not found"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn test_onnx_dimensions() {
        // Use Cargo.toml as a stand-in file that is guaranteed to exist.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        #[cfg(not(feature = "onnx"))]
        {
            let provider = OnnxEmbedding::new(path, 384).expect("file should exist");
            assert_eq!(provider.dimensions(), 384);
        }
        // When the onnx feature is on, construction also requires
        // tokenizer.json, so we only test that the path validation
        // passes for the stub variant.
        #[cfg(feature = "onnx")]
        {
            // Without a tokenizer.json next to Cargo.toml, we expect an
            // embedding error rather than a validation error.
            let result = OnnxEmbedding::new(path, 384);
            assert!(result.is_err());
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains("tokenizer.json"),
                "expected tokenizer.json error, got: {msg}"
            );
        }
    }

    #[test]
    fn test_onnx_model_path() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        #[cfg(not(feature = "onnx"))]
        {
            let provider = OnnxEmbedding::new(path, 768).expect("file should exist");
            assert_eq!(provider.model_path(), path);
        }
        #[cfg(feature = "onnx")]
        {
            let result = OnnxEmbedding::new(path, 768);
            assert!(result.is_err());
        }
    }

    #[cfg(not(feature = "onnx"))]
    #[tokio::test]
    async fn test_onnx_embed_returns_error_without_runtime() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let provider = OnnxEmbedding::new(path, 384).expect("file should exist");
        let result = provider.embed("hello world").await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("ONNX Runtime not available"),
            "unexpected error: {msg}"
        );
    }

    #[cfg(not(feature = "onnx"))]
    #[tokio::test]
    async fn test_onnx_embed_batch_returns_error_without_runtime() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let provider = OnnxEmbedding::new(path, 384).expect("file should exist");
        let result = provider.embed_batch(&["a", "b"]).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("ONNX Runtime not available"),
            "unexpected error: {msg}"
        );
    }
}
