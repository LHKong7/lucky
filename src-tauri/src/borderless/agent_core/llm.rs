//! LLM provider abstraction trait.
//!
//! Defines the interface that all LLM providers (OpenAI, Anthropic, Google)
//! must implement. Uses separate methods for chat vs streaming.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use super::{ChatMessage, ChatOptions, LlmError, LlmResponse};

/// Trait for LLM provider implementations.
///
/// Each provider (OpenAI, Anthropic, Google) implements this trait.
/// The agent loop uses this abstraction to call any backend without
/// knowing the specifics.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Maximum context window size in tokens for the configured model.
    fn context_window_size(&self) -> usize;

    /// Whether this provider supports streaming responses.
    fn supports_streaming(&self) -> bool;

    /// The model identifier string.
    fn model(&self) -> &str;

    /// Send a chat completion request and get a complete response.
    async fn chat(
        &self,
        messages: &[ChatMessage],
        options: &ChatOptions,
    ) -> Result<LlmResponse, LlmError>;

    /// Send a chat completion request and get a streaming response.
    ///
    /// Returns a stream of partial `LlmResponse` chunks. Each chunk may
    /// contain partial content, tool calls, or usage information.
    fn chat_stream<'a>(
        &'a self,
        messages: &'a [ChatMessage],
        options: &'a ChatOptions,
    ) -> Pin<Box<dyn Stream<Item = Result<LlmResponse, LlmError>> + Send + 'a>>;
}

/// Embedding provider abstraction.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for the given text.
    async fn embed(&self, text: &str) -> Result<Vec<f64>, LlmError>;
}

/// Compute the cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &b)).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }
}
