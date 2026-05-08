//! OpenAI provider implementation.

use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use crate::borderless::agent_core::{ChatMessage, ChatOptions, LlmError, LlmResponse, LlmProvider};
use crate::borderless::agent_core::provider_meta::get_context_window_for_model;

/// OpenAI-compatible LLM provider.
///
/// Works with OpenAI API, Azure OpenAI, and any OpenAI-compatible endpoint
/// (Together, Ollama, vLLM, etc.) via `base_url`.
pub struct OpenAIProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
    context_window: usize,
}

impl OpenAIProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>, base_url: Option<String>) -> Self {
        let model = model.into();
        let context_window = get_context_window_for_model(&model, None);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build HTTP client");

        Self {
            api_key: api_key.into(),
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            client,
            context_window,
        }
    }

    fn build_request_body(
        &self,
        messages: &[ChatMessage],
        options: &ChatOptions,
        stream: bool,
    ) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": stream,
        });

        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max) = options.max_tokens {
            body["max_tokens"] = serde_json::json!(max);
        }
        if let Some(ref tools) = options.tools {
            body["tools"] = tools.clone();
        }

        body
    }

    fn parse_response(&self, json: serde_json::Value) -> Result<LlmResponse, LlmError> {
        let choice = json["choices"]
            .as_array()
            .and_then(|c| c.first())
            .ok_or_else(|| LlmError::Request("No choices in response".into()))?;

        let message = &choice["message"];
        let content = message["content"].as_str().map(String::from);
        let thinking = message["thinking"].as_str().map(String::from);

        let tool_calls = message["tool_calls"]
            .as_array()
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|tc| {
                        let id = tc["id"].as_str()?.to_string();
                        let name = tc["function"]["name"].as_str()?.to_string();
                        let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                        let arguments: serde_json::Value =
                            serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                        Some(crate::borderless::agent_core::ToolCall {
                            id,
                            name,
                            arguments,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let usage = json["usage"]
            .as_object()
            .map(|u| {
                let mut map = HashMap::new();
                if let Some(v) = u.get("prompt_tokens").and_then(|v| v.as_u64()) {
                    map.insert("input_tokens".into(), v);
                }
                if let Some(v) = u.get("completion_tokens").and_then(|v| v.as_u64()) {
                    map.insert("output_tokens".into(), v);
                }
                map
            })
            .unwrap_or_default();

        let model = json["model"]
            .as_str()
            .unwrap_or(&self.model)
            .to_string();

        Ok(LlmResponse {
            content,
            tool_calls,
            usage,
            model,
            thinking,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    fn context_window_size(&self) -> usize {
        self.context_window
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        options: &ChatOptions,
    ) -> Result<LlmResponse, LlmError> {
        let body = self.build_request_body(messages, options, false);
        let url = format!("{}/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Request(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(LlmError::Authentication(text));
            }
            if status.as_u16() == 429 {
                return Err(LlmError::RateLimit { retry_after: None });
            }
            return Err(LlmError::Request(format!("HTTP {}: {}", status, text)));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlmError::Request(e.to_string()))?;

        self.parse_response(json)
    }

    fn chat_stream<'a>(
        &'a self,
        messages: &'a [ChatMessage],
        options: &'a ChatOptions,
    ) -> Pin<Box<dyn Stream<Item = Result<LlmResponse, LlmError>> + Send + 'a>> {
        let body = self.build_request_body(messages, options, true);
        let url = format!("{}/chat/completions", self.base_url);

        Box::pin(async_stream::try_stream! {
            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| LlmError::Request(e.to_string()))?;

            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                Err(LlmError::Request(format!("HTTP {}: {}", status, text)))?;
                unreachable!();
            }

            let mut byte_stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = StreamExt::next(&mut byte_stream).await {
                let chunk = chunk.map_err(|e| LlmError::Request(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete lines from the buffer
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            return;
                        }

                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            let delta_content = json["choices"][0]["delta"]["content"]
                                .as_str()
                                .map(String::from);

                            // Parse tool call deltas
                            let tool_calls: Vec<crate::borderless::agent_core::ToolCall> = json["choices"][0]["delta"]["tool_calls"]
                                .as_array()
                                .map(|calls| {
                                    calls.iter().filter_map(|tc| {
                                        let id = tc["id"].as_str().unwrap_or("").to_string();
                                        let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                                        let args_str = tc["function"]["arguments"].as_str().unwrap_or("");
                                        if name.is_empty() && args_str.is_empty() {
                                            return None;
                                        }
                                        let arguments = serde_json::from_str(args_str)
                                            .unwrap_or(serde_json::json!({}));
                                        Some(crate::borderless::agent_core::ToolCall { id, name, arguments })
                                    }).collect()
                                })
                                .unwrap_or_default();

                            let mut usage = HashMap::new();
                            if let Some(u) = json["usage"].as_object() {
                                if let Some(v) = u.get("prompt_tokens").and_then(|v| v.as_u64()) {
                                    usage.insert("input_tokens".into(), v);
                                }
                                if let Some(v) = u.get("completion_tokens").and_then(|v| v.as_u64()) {
                                    usage.insert("output_tokens".into(), v);
                                }
                            }

                            if delta_content.is_some() || !tool_calls.is_empty() || !usage.is_empty() {
                                yield LlmResponse {
                                    content: delta_content,
                                    tool_calls,
                                    usage,
                                    model: self.model.clone(),
                                    thinking: None,
                                };
                            }
                        }
                    }
                }
            }
        })
    }
}
