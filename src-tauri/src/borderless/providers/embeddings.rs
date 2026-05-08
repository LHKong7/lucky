//! Embedding provider implementations.

use async_trait::async_trait;
use crate::borderless::agent_core::{LlmError, EmbeddingProvider};

/// OpenAI-compatible embedding provider.
pub struct OpenAIEmbeddingProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAIEmbeddingProvider {
    pub fn new(
        api_key: impl Into<String>,
        model: Option<String>,
        base_url: Option<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");

        Self {
            api_key: api_key.into(),
            model: model.unwrap_or_else(|| "text-embedding-3-small".to_string()),
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            client,
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f64>, LlmError> {
        let url = format!("{}/embeddings", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "input": text,
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Request(e.to_string()))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Request(format!("Embedding request failed: {}", text)));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlmError::Request(e.to_string()))?;

        let embedding = json["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| LlmError::Request("No embedding in response".into()))?
            .iter()
            .filter_map(|v| v.as_f64())
            .collect();

        Ok(embedding)
    }
}
