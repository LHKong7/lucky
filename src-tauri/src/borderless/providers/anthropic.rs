//! Anthropic (Claude) provider implementation.

use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use crate::borderless::agent_core::{ChatMessage, ChatOptions, LlmError, LlmResponse, LlmProvider, MessageContent};
use crate::borderless::agent_core::provider_meta::get_context_window_for_model;

/// Anthropic Claude LLM provider.
pub struct AnthropicProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
    context_window: usize,
}

impl AnthropicProvider {
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
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
            client,
            context_window,
        }
    }

    /// Convert ChatMessage slice to Anthropic message format.
    fn to_anthropic_messages(messages: &[ChatMessage]) -> (Option<String>, Vec<serde_json::Value>) {
        let mut system_prompt = None;
        let mut anthropic_msgs: Vec<serde_json::Value> = Vec::new();

        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    system_prompt = Some(content.clone());
                }
                ChatMessage::User { content } => {
                    let content_value = match content {
                        MessageContent::Text(text) => serde_json::json!(text),
                        MessageContent::Parts(parts) => {
                            let blocks: Vec<serde_json::Value> = parts
                                .iter()
                                .map(|p| match p {
                                    crate::borderless::agent_core::ContentPart::Text { text } => {
                                        serde_json::json!({"type": "text", "text": text})
                                    }
                                    crate::borderless::agent_core::ContentPart::ImageUrl { image_url } => {
                                        serde_json::json!({
                                            "type": "image",
                                            "source": { "type": "url", "url": image_url.url }
                                        })
                                    }
                                })
                                .collect();
                            serde_json::json!(blocks)
                        }
                    };

                    // Merge consecutive user messages
                    if let Some(last) = anthropic_msgs.last() {
                        if last["role"] == "user" {
                            // Need to merge - pop last and combine
                            let mut last = anthropic_msgs.pop().unwrap();
                            let prev_content = last["content"].take();
                            let combined = serde_json::json!([
                                {"type": "text", "text": prev_content.as_str().unwrap_or("")},
                                {"type": "text", "text": content_value.as_str().unwrap_or("")}
                            ]);
                            last["content"] = combined;
                            anthropic_msgs.push(last);
                            continue;
                        }
                    }

                    anthropic_msgs.push(serde_json::json!({
                        "role": "user",
                        "content": content_value,
                    }));
                }
                ChatMessage::Assistant {
                    content,
                    tool_calls,
                    ..
                } => {
                    let mut blocks: Vec<serde_json::Value> = Vec::new();
                    if let Some(text) = content {
                        if !text.is_empty() {
                            blocks.push(serde_json::json!({"type": "text", "text": text}));
                        }
                    }
                    for tc in tool_calls {
                        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::json!({}));
                        blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.function.name,
                            "input": args,
                        }));
                    }
                    if !blocks.is_empty() {
                        anthropic_msgs.push(serde_json::json!({
                            "role": "assistant",
                            "content": blocks,
                        }));
                    }
                }
                ChatMessage::Tool {
                    tool_call_id,
                    content,
                } => {
                    anthropic_msgs.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": content,
                        }],
                    }));
                }
            }
        }

        (system_prompt, anthropic_msgs)
    }

    fn build_request_body(
        &self,
        messages: &[ChatMessage],
        options: &ChatOptions,
        stream: bool,
    ) -> serde_json::Value {
        let (system, msgs) = Self::to_anthropic_messages(messages);

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": msgs,
            "max_tokens": options.max_tokens.unwrap_or(8192),
            "stream": stream,
        });

        if let Some(sys) = system {
            body["system"] = serde_json::json!(sys);
        }
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(ref tools) = options.tools {
            // Convert OpenAI tool format to Anthropic format
            if let Some(tool_arr) = tools.as_array() {
                let anthropic_tools: Vec<serde_json::Value> = tool_arr
                    .iter()
                    .filter_map(|t| {
                        let func = t.get("function")?;
                        Some(serde_json::json!({
                            "name": func["name"],
                            "description": func["description"],
                            "input_schema": func.get("parameters").cloned()
                                .unwrap_or(serde_json::json!({"type": "object", "properties": {}})),
                        }))
                    })
                    .collect();
                body["tools"] = serde_json::json!(anthropic_tools);
            }
        }

        body
    }

    fn parse_response(&self, json: serde_json::Value) -> Result<LlmResponse, LlmError> {
        let mut content = None;
        let mut tool_calls = Vec::new();
        let mut thinking = None;

        if let Some(blocks) = json["content"].as_array() {
            let mut text_parts = Vec::new();
            for block in blocks {
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = block["text"].as_str() {
                            text_parts.push(t.to_string());
                        }
                    }
                    Some("tool_use") => {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        let arguments = block["input"].clone();
                        tool_calls.push(crate::borderless::agent_core::ToolCall {
                            id,
                            name,
                            arguments,
                        });
                    }
                    Some("thinking") => {
                        if let Some(t) = block["thinking"].as_str() {
                            thinking = Some(t.to_string());
                        }
                    }
                    _ => {}
                }
            }
            if !text_parts.is_empty() {
                content = Some(text_parts.join(""));
            }
        }

        let usage = json["usage"]
            .as_object()
            .map(|u| {
                let mut map = HashMap::new();
                if let Some(v) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                    map.insert("input_tokens".into(), v);
                }
                if let Some(v) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                    map.insert("output_tokens".into(), v);
                }
                if let Some(v) = u.get("cache_read_input_tokens").and_then(|v| v.as_u64()) {
                    map.insert("cache_read_input_tokens".into(), v);
                }
                if let Some(v) = u.get("cache_creation_input_tokens").and_then(|v| v.as_u64()) {
                    map.insert("cache_creation_input_tokens".into(), v);
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
impl LlmProvider for AnthropicProvider {
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
        let url = format!("{}/v1/messages", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
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
        let url = format!("{}/v1/messages", self.base_url);

        Box::pin(async_stream::try_stream! {
            let resp = self
                .client
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
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

                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            match json["type"].as_str() {
                                Some("content_block_delta") => {
                                    if let Some(text_delta) = json["delta"]["text"].as_str() {
                                        yield LlmResponse {
                                            content: Some(text_delta.to_string()),
                                            tool_calls: Vec::new(),
                                            usage: HashMap::new(),
                                            model: self.model.clone(),
                                            thinking: None,
                                        };
                                    }
                                    // Handle tool use input_json_delta
                                    if let Some(partial_json) = json["delta"]["partial_json"].as_str() {
                                        if !partial_json.is_empty() {
                                            yield LlmResponse {
                                                content: Some(partial_json.to_string()),
                                                tool_calls: Vec::new(),
                                                usage: HashMap::new(),
                                                model: self.model.clone(),
                                                thinking: None,
                                            };
                                        }
                                    }
                                }
                                Some("content_block_start") => {
                                    // Tool use block starts with name + id
                                    if json["content_block"]["type"].as_str() == Some("tool_use") {
                                        if let (Some(id), Some(name)) = (
                                            json["content_block"]["id"].as_str(),
                                            json["content_block"]["name"].as_str(),
                                        ) {
                                            yield LlmResponse {
                                                content: None,
                                                tool_calls: vec![crate::borderless::agent_core::ToolCall {
                                                    id: id.to_string(),
                                                    name: name.to_string(),
                                                    arguments: serde_json::json!({}),
                                                }],
                                                usage: HashMap::new(),
                                                model: self.model.clone(),
                                                thinking: None,
                                            };
                                        }
                                    }
                                }
                                Some("message_start") => {
                                    let mut usage = HashMap::new();
                                    if let Some(u) = json["message"]["usage"].as_object() {
                                        if let Some(v) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                                            usage.insert("input_tokens".into(), v);
                                        }
                                    }
                                    if !usage.is_empty() {
                                        yield LlmResponse {
                                            content: None,
                                            tool_calls: Vec::new(),
                                            usage,
                                            model: self.model.clone(),
                                            thinking: None,
                                        };
                                    }
                                }
                                Some("message_delta") => {
                                    let mut usage = HashMap::new();
                                    if let Some(u) = json["usage"].as_object() {
                                        if let Some(v) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                                            usage.insert("output_tokens".into(), v);
                                        }
                                    }
                                    if !usage.is_empty() {
                                        yield LlmResponse {
                                            content: None,
                                            tool_calls: Vec::new(),
                                            usage,
                                            model: self.model.clone(),
                                            thinking: None,
                                        };
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        })
    }
}
