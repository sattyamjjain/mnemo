use crate::embedding::EmbeddingProvider;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

pub struct OpenAiEmbedding {
    api_key: String,
    model: String,
    dimensions: usize,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
    dimensions: usize,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl OpenAiEmbedding {
    pub fn new(api_key: String, model: String, dimensions: usize) -> Self {
        Self {
            api_key,
            model,
            dimensions,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|e| {
                    tracing::error!(error = %e, "failed to build HTTP client with timeouts, using default");
                    reqwest::Client::default()
                }),
        }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OpenAiEmbedding {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| Error::Embedding("empty response from OpenAI".to_string()))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
            dimensions: self.dimensions,
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Embedding(format!(
                "OpenAI API error {status}: {body}"
            )));
        }

        let resp: EmbeddingResponse = response.json().await?;
        Ok(resp.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::NoopEmbedding;

    #[tokio::test]
    async fn test_noop_embedding() {
        let provider = NoopEmbedding::new(1536);
        let result = provider.embed("test").await.unwrap();
        assert_eq!(result.len(), 1536);
        assert!(result.iter().all(|&v| v == 0.0));
    }

    #[tokio::test]
    async fn test_noop_batch() {
        let provider = NoopEmbedding::new(768);
        let result = provider.embed_batch(&["a", "b", "c"]).await.unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.iter().all(|v| v.len() == 768));
    }

    #[tokio::test]
    async fn test_noop_dimensions() {
        let provider = NoopEmbedding::new(256);
        assert_eq!(provider.dimensions(), 256);
    }

    #[tokio::test]
    #[ignore] // Requires OPENAI_API_KEY
    async fn test_openai_embedding() {
        let api_key = std::env::var("OPENAI_API_KEY").unwrap();
        let provider = OpenAiEmbedding::new(
            api_key,
            "text-embedding-3-small".to_string(),
            1536,
        );
        let result = provider.embed("hello world").await.unwrap();
        assert_eq!(result.len(), 1536);
    }
}
