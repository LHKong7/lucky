//! Lightweight tracing span primitives.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// Status of a completed span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanStatus {
    Ok,
    Error,
}

/// An event recorded within a span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, serde_json::Value>>,
}

/// Completed span data suitable for export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanData {
    pub name: String,
    pub trace_id: String,
    pub span_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub start_time_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub status: SpanStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    pub attributes: HashMap<String, serde_json::Value>,
    pub events: Vec<SpanEvent>,
}

/// A live span that can be mutated until ended.
#[derive(Clone)]
pub struct Span {
    inner: Arc<Mutex<SpanInner>>,
    on_end: Arc<dyn Fn(SpanData) + Send + Sync>,
}

struct SpanInner {
    data: SpanData,
    ended: bool,
}

impl Span {
    pub(crate) fn new(data: SpanData, on_end: Arc<dyn Fn(SpanData) + Send + Sync>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SpanInner { data, ended: false })),
            on_end,
        }
    }

    pub fn name(&self) -> String {
        self.inner.lock().unwrap().data.name.clone()
    }

    pub fn trace_id(&self) -> String {
        self.inner.lock().unwrap().data.trace_id.clone()
    }

    pub fn span_id(&self) -> String {
        self.inner.lock().unwrap().data.span_id.clone()
    }

    pub fn set_attribute(&self, key: impl Into<String>, value: impl Into<serde_json::Value>) {
        let mut inner = self.inner.lock().unwrap();
        if inner.ended {
            return;
        }
        inner.data.attributes.insert(key.into(), value.into());
    }

    pub fn set_attributes(&self, attrs: HashMap<String, serde_json::Value>) {
        let mut inner = self.inner.lock().unwrap();
        if inner.ended {
            return;
        }
        inner.data.attributes.extend(attrs);
    }

    pub fn add_event(&self, name: impl Into<String>, attrs: Option<HashMap<String, serde_json::Value>>) {
        let mut inner = self.inner.lock().unwrap();
        if inner.ended {
            return;
        }
        inner.data.events.push(SpanEvent {
            name: name.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            attributes: attrs,
        });
    }

    pub fn set_status(&self, status: SpanStatus, message: Option<String>) {
        let mut inner = self.inner.lock().unwrap();
        if inner.ended {
            return;
        }
        inner.data.status = status;
        if message.is_some() {
            inner.data.status_message = message;
        }
    }

    pub fn end(&self) {
        let data = {
            let mut inner = self.inner.lock().unwrap();
            if inner.ended {
                return;
            }
            inner.ended = true;
            let now = chrono::Utc::now().timestamp_millis();
            inner.data.end_time_ms = Some(now);
            inner.data.duration_ms = Some(now - inner.data.start_time_ms);
            inner.data.clone()
        };
        (self.on_end)(data);
    }

    pub fn data(&self) -> SpanData {
        self.inner.lock().unwrap().data.clone()
    }
}

impl std::fmt::Debug for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.lock().unwrap();
        f.debug_struct("Span")
            .field("name", &inner.data.name)
            .field("span_id", &inner.data.span_id)
            .field("ended", &inner.ended)
            .finish()
    }
}
