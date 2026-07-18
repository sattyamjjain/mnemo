pub mod onnx;
pub mod openai;

use crate::error::Result;

#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;

    /// Whether this provider produces real query vectors usable for semantic
    /// (dense-similarity) recall. Defaults to `true`; the no-op provider — which
    /// returns all-zero vectors — overrides this to `false` so the recall path
    /// can refuse semantic/hybrid queries with a typed error instead of silently
    /// returning an empty result set. Real providers (OpenAI, ONNX) inherit
    /// `true`.
    fn is_semantic_capable(&self) -> bool {
        true
    }
}

pub struct NoopEmbedding {
    dimensions: usize,
}

impl NoopEmbedding {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for NoopEmbedding {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.0; self.dimensions])
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.0; self.dimensions]).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// The no-op embedder returns all-zero vectors — it cannot back semantic
    /// recall. Reporting `false` makes the recall path fail loud instead of
    /// silently returning empty.
    fn is_semantic_capable(&self) -> bool {
        false
    }
}

/// A deterministic, offline **bag-of-words hashing** embedder for tests,
/// examples, and demos.
///
/// Each whitespace token is FNV-1a hashed into a bucket and the resulting
/// vector is L2-normalized, so two texts that share tokens are close under
/// cosine similarity. It needs no model file and no network and — unlike
/// [`NoopEmbedding`] — produces real, non-zero vectors, so it reports
/// `is_semantic_capable() == true` and can back semantic/hybrid recall.
///
/// It is **not** a production-quality semantic model (there is no learned
/// meaning, only lexical hashing); use OpenAI (HTTP) or ONNX embeddings for real
/// semantic recall. Its purpose is a reproducible, dependency-free stand-in so
/// tests and examples can exercise the vector path without an API key or model.
pub struct DeterministicEmbedding {
    dimensions: usize,
}

impl DeterministicEmbedding {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }

    fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0f32; self.dimensions];
        for tok in text.split_whitespace() {
            // FNV-1a over the token, mapped into a bucket.
            let mut h = 0xcbf29ce484222325u64;
            for b in tok.bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            let idx = (h as usize) % self.dimensions.max(1);
            v[idx] += 1.0;
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for DeterministicEmbedding {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self.embed_one(text))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    // Inherits `is_semantic_capable() == true` — it emits real, non-zero vectors.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_is_not_semantic_capable_but_deterministic_is() {
        assert!(!NoopEmbedding::new(8).is_semantic_capable());
        assert!(DeterministicEmbedding::new(8).is_semantic_capable());
    }

    #[tokio::test]
    async fn deterministic_embedding_is_stable_and_nonzero() {
        let e = DeterministicEmbedding::new(16);
        let a = e.embed("clinician adjusted the dosage").await.unwrap();
        let b = e.embed("clinician adjusted the dosage").await.unwrap();
        assert_eq!(a, b, "same text embeds identically");
        assert_eq!(a.len(), 16);
        assert!(a.iter().any(|x| *x != 0.0), "produces non-zero vectors");
    }
}
