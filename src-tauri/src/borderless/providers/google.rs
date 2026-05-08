//! Google Gemini provider implementation.

use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures::stream::{self, Stream};
use crate::borderless::agent_core::{ChatMessage, ChatOptions, LlmError, LlmResponse, LlmProvider, MessageContent};
use crate::borderless::agent_core::provider_meta::get_context_window_for_model;

/// Google Gemini LLM provider.
pub struct GoogleProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
    context_window: usize,
}

impl GoogleProvider {
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
            base_url: base_url.unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string()),
            client,
            context_window,
        }
    }

    fn to_gemini_contents(messages: &[ChatMessage]) -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();

        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    system_instruction = Some(serde_json::json!({
                        "parts": [{"text": content}]
                    }));
                }
                ChatMessage::User { content } => {
                    let parts = match content {
                        MessageContent::Text(text) => vec![serde_json::json!({"text": text})],
                        MessageContent::Parts(parts) => {
                            parts.iter().map(|p| match p {
                                crate::borderless::agent_core::ContentPart::Text { text } => {
                                    serde_json::json!({"text": text})
                                }
                                crate::borderless::agent_core::ContentPart::ImageUrl { image_url } => {
                                    serde_json::json!({
                                        "inlineData": {
                                            "mimeType": "image/jpeg",
                                            "data": image_url.url
                                        }
                                    })
                                }
                            }).collect()
                        }
                    };
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": parts,
                    }));
                }
                ChatMessage::Assistant { content, tool_calls, .. } => {
                    let mut parts: Vec<serde_json::Value> = Vec::new();
                    if let Some(text) = content {
                        if !text.is_empty() {
                            parts.push(serde_json::json!({"text": text}));
                        }
                    }
                    for tc in tool_calls {
                        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::json!({}));
                        parts.push(serde_json::json!({
                            "functionCall": {
                                "name": tc.function.name,
                                "args": args,
                            }
                        }));
                    }
                    if !parts.is_empty() {
                        contents.push(serde_json::json!({
                            "role": "model",
                            "parts": parts,
                        }));
                    }
                }
                ChatMessage::Tool { tool_call_id: _, content } => {
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": "tool",
                                "response": {"result": content},
                            }
                        }],
                    }));
                }
            }
        }

        (system_instruction, contents)
    }

    fn parse_response(&self, json: serde_json::Value) -> Result<LlmResponse, LlmError> {
        let candidate = json["candidates"]
            .as_array()
            .and_then(|c| c.first())
            .ok_or_else(|| LlmError::Request("No candidates in response".into()))?;

        let mut content = None;
        let mut tool_calls = Vec::new();

        if let Some(parts) = candidate["content"]["parts"].as_array() {
            let mut text_parts = Vec::new();
            for part in parts {
                if let Some(text) = part["text"].as_str() {
                    text_parts.push(text.to_string());
                }
                if let Some(fc) = part.get("functionCall") {
                    let name = fc["name"].as_str().unwrap_or("").to_string();
                    let arguments = fc["args"].clone();
                    tool_calls.push(crate::borderless::agent_core::ToolCall {
                        id: uuid::Uuid::new_v4().to_string(),
                        name,
                        arguments,
                    });
                }
            }
            if !text_parts.is_empty() {
                content = Some(text_parts.join(""));
            }
        }

        let usage = json["usageMetadata"]
            .as_object()
            .map(|u| {
                let mut map = HashMap::new();
                if let Some(v) = u.get("promptTokenCount").and_then(|v| v.as_u64()) {
                    map.insert("input_tokens".into(), v);
                }
                if let Some(v) = u.get("candidatesTokenCount").and_then(|v| v.as_u64()) {
                    map.insert("output_tokens".into(), v);
                }
                map
            })
            .unwrap_or_default();

        Ok(LlmResponse {
            content,
            tool_calls,
            usage,
            model: self.model.clone(),
            thinking: None,
        })
    }
}

#[async_trait]
impl LlmProvider for GoogleProvider {
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
        let (system_instruction, contents) = Self::to_gemini_contents(messages);

        let mut body = serde_json::json!({ "contents": contents });
        if let Some(sys) = system_instruction {
            body["systemInstruction"] = sys;
        }

        let mut generation_config = serde_json::json!({});
        if let Some(temp) = options.temperature {
            generation_config["temperature"] = serde_json::json!(temp);
        }
        if let Some(max) = options.max_tokens {
            generation_config["maxOutputTokens"] = serde_json::json!(max);
        }
        body["generationConfig"] = generation_config;

        if let Some(ref tools) = options.tools {
            if let Some(tool_arr) = tools.as_array() {
                let declarations: Vec<serde_json::Value> = tool_arr
                    .iter()
                    .filter_map(|t| {
                        let func = t.get("function")?;
                        Some(serde_json::json!({
                            "name": func["name"],
                            "description": func["description"],
                            "parameters": func.get("parameters").cloned()
                                .unwrap_or(serde_json::json!({"type": "object", "properties": {}})),
                        }))
                    })
                    .collect();
                body["tools"] = serde_json::json!([{"functionDeclarations": declarations}]);
            }
        }

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let resp = self
            .client
            .post(&url)
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
        // For simplicity, delegate to non-streaming for now
        // TODO: Implement proper streaming via streamGenerateContent
        Box::pin(stream::once(async move { self.chat(messages, options).await }))
    }
}
