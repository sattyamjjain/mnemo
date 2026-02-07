pub mod onnx;
pub mod openai;

use crate::error::Result;

#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
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
}
