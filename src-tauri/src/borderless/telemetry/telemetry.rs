//! Main Telemetry struct — span creation, structured logging, GenAI helpers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::exporter::TelemetryExporter;
use super::span::{Span, SpanData, SpanStatus};

/// Log severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug = 10,
    Info = 20,
    Warn = 30,
    Error = 40,
}

/// A structured log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub module: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
}

/// Configuration for a Telemetry instance.
pub struct TelemetryConfig {
    pub service_name: Option<String>,
    pub exporter: Option<Arc<dyn TelemetryExporter>>,
    pub min_log_level: Option<LogLevel>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: None,
            exporter: None,
            min_log_level: None,
        }
    }
}

/// Lightweight telemetry with span creation and structured logging.
pub struct Telemetry {
    pub service_name: String,
    exporter: Option<Arc<dyn TelemetryExporter>>,
    min_log_level: LogLevel,
    active_stack: Mutex<Vec<Span>>,
}

impl Telemetry {
    pub fn new(config: TelemetryConfig) -> Self {
        Self {
            service_name: config.service_name.unwrap_or_else(|| "borderless-agent".to_string()),
            exporter: config.exporter,
            min_log_level: config.min_log_level.unwrap_or(LogLevel::Info),
            active_stack: Mutex::new(Vec::new()),
        }
    }

    /// A telemetry instance whose methods are all no-ops (no exporter).
    pub fn noop() -> Self {
        Self::new(TelemetryConfig::default())
    }

    /// Start a new span.
    pub fn start_span(
        &self,
        name: impl Into<String>,
        parent: Option<&Span>,
        attributes: Option<HashMap<String, serde_json::Value>>,
    ) -> Span {
        let name = name.into();
        let stack = self.active_stack.lock().unwrap();
        let parent_span = parent.or_else(|| stack.last());

        let trace_id = parent_span
            .map(|p| p.trace_id())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let span_id = uuid::Uuid::new_v4().to_string();
        let parent_span_id = parent_span.map(|p| p.span_id());

        let mut attrs = HashMap::new();
        attrs.insert(
            "service.name".to_string(),
            serde_json::Value::String(self.service_name.clone()),
        );
        if let Some(extra) = attributes {
            attrs.extend(extra);
        }

        let data = SpanData {
            name,
            trace_id,
            span_id: span_id.clone(),
            parent_span_id,
            start_time_ms: chrono::Utc::now().timestamp_millis(),
            end_time_ms: None,
            duration_ms: None,
            status: SpanStatus::Ok,
            status_message: None,
            attributes: attrs,
            events: Vec::new(),
        };

        let exporter = self.exporter.clone();
        let on_end: Arc<dyn Fn(SpanData) + Send + Sync> = Arc::new(move |data| {
            if let Some(ref exp) = exporter {
                exp.export_span(&data);
            }
        });

        let span = Span::new(data, on_end);
        drop(stack);

        self.active_stack.lock().unwrap().push(span.clone());
        span
    }

    /// Run an async function inside a new span, ending it automatically.
    pub async fn with_span<T, F, Fut>(
        &self,
        name: impl Into<String>,
        f: F,
        attributes: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce(Span) -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let span = self.start_span(name, None, attributes);
        match f(span.clone()).await {
            Ok(val) => {
                span.end();
                self.pop_span(&span);
                Ok(val)
            }
            Err(e) => {
                span.set_status(SpanStatus::Error, Some(e.to_string()));
                span.end();
                self.pop_span(&span);
                Err(e)
            }
        }
    }

    /// Returns the current top-of-stack span, if any.
    pub fn active_span(&self) -> Option<Span> {
        self.active_stack.lock().unwrap().last().cloned()
    }

    fn pop_span(&self, span: &Span) {
        let mut stack = self.active_stack.lock().unwrap();
        if let Some(top) = stack.last() {
            if top.span_id() == span.span_id() {
                stack.pop();
            }
        }
    }

    // -- Structured logging --

    pub fn log(
        &self,
        level: LogLevel,
        module: &str,
        message: &str,
        context: Option<HashMap<String, serde_json::Value>>,
    ) {
        if level < self.min_log_level {
            return;
        }
        let active = self.active_span();
        let entry = LogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            module: module.to_string(),
            message: message.to_string(),
            context,
            trace_id: active.as_ref().map(|s| s.trace_id()),
            span_id: active.as_ref().map(|s| s.span_id()),
        };
        if let Some(ref exp) = self.exporter {
            exp.export_log(&entry);
        }
    }

    pub fn debug(&self, module: &str, message: &str, context: Option<HashMap<String, serde_json::Value>>) {
        self.log(LogLevel::Debug, module, message, context);
    }

    pub fn info(&self, module: &str, message: &str, context: Option<HashMap<String, serde_json::Value>>) {
        self.log(LogLevel::Info, module, message, context);
    }

    pub fn warn(&self, module: &str, message: &str, context: Option<HashMap<String, serde_json::Value>>) {
        self.log(LogLevel::Warn, module, message, context);
    }

    pub fn error(&self, module: &str, message: &str, context: Option<HashMap<String, serde_json::Value>>) {
        self.log(LogLevel::Error, module, message, context);
    }

    // -- GenAI / agent-specific helpers --

    pub fn record_chat(&self, span: &Span, model: &str, input_tokens: u64, output_tokens: u64, duration_ms: u64) {
        let mut attrs = HashMap::new();
        attrs.insert("gen_ai.system".into(), serde_json::Value::String("agent".into()));
        attrs.insert("gen_ai.request.model".into(), serde_json::Value::String(model.into()));
        attrs.insert("gen_ai.usage.input_tokens".into(), serde_json::json!(input_tokens));
        attrs.insert("gen_ai.usage.output_tokens".into(), serde_json::json!(output_tokens));
        attrs.insert("gen_ai.usage.total_tokens".into(), serde_json::json!(input_tokens + output_tokens));
        attrs.insert("llm.duration_ms".into(), serde_json::json!(duration_ms));
        span.set_attributes(attrs);
    }

    pub fn record_tool_call(&self, span: &Span, tool_name: &str, duration_ms: u64, success: bool, error_code: Option<&str>) {
        let mut attrs = HashMap::new();
        attrs.insert("agent.tool.name".into(), serde_json::Value::String(tool_name.into()));
        attrs.insert("agent.tool.duration_ms".into(), serde_json::json!(duration_ms));
        attrs.insert("agent.tool.success".into(), serde_json::json!(success));
        span.set_attributes(attrs);
        if !success {
            let code = error_code.unwrap_or("UNKNOWN");
            span.set_attribute("agent.tool.error_code", serde_json::Value::String(code.into()));
            span.set_status(SpanStatus::Error, Some(code.to_string()));
        }
    }

    pub fn record_memory_retrieval(&self, span: &Span, retrieved_count: usize, scores: &[f64]) {
        let sum: f64 = scores.iter().sum();
        let avg = if scores.is_empty() { 0.0 } else { sum / scores.len() as f64 };
        let max = scores.iter().cloned().fold(0.0f64, f64::max);

        let mut attrs = HashMap::new();
        attrs.insert("agent.memory.retrieved_count".into(), serde_json::json!(retrieved_count));
        attrs.insert("agent.memory.avg_score".into(), serde_json::json!(avg));
        attrs.insert("agent.memory.max_score".into(), serde_json::json!(max));
        span.set_attributes(attrs);
    }
}

impl std::fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Telemetry")
            .field("service_name", &self.service_name)
            .field("has_exporter", &self.exporter.is_some())
            .finish()
    }
}
