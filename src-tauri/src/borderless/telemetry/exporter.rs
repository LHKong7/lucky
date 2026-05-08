//! Telemetry exporters.

use std::sync::Mutex;

use super::span::SpanData;
use super::telemetry::LogEntry;

/// Trait for receiving telemetry data (spans and logs).
pub trait TelemetryExporter: Send + Sync {
    fn export_span(&self, span: &SpanData);
    fn export_log(&self, entry: &LogEntry);
}

/// Pretty-prints spans/logs to stderr. Useful for development.
pub struct ConsoleExporter {
    verbose: bool,
}

impl ConsoleExporter {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }
}

impl TelemetryExporter for ConsoleExporter {
    fn export_span(&self, span: &SpanData) {
        let status = if span.status == super::SpanStatus::Error {
            "ERR"
        } else {
            "OK "
        };
        let duration = span.duration_ms.unwrap_or(0);
        if self.verbose {
            eprintln!(
                "[span] {} {} {}ms {:?}",
                status, span.name, duration, span.attributes
            );
        } else {
            eprintln!("[span] {} {} {}ms", status, span.name, duration);
        }
    }

    fn export_log(&self, entry: &LogEntry) {
        let ctx = if let Some(ref context) = entry.context {
            if !context.is_empty() {
                format!(" {}", serde_json::to_string(context).unwrap_or_default())
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        eprintln!("[{:?}] {}: {}{}", entry.level, entry.module, entry.message, ctx);
    }
}

/// Buffers spans/logs in memory. Useful for tests and post-hoc inspection.
pub struct MemoryExporter {
    spans: Mutex<Vec<SpanData>>,
    logs: Mutex<Vec<LogEntry>>,
}

impl MemoryExporter {
    pub fn new() -> Self {
        Self {
            spans: Mutex::new(Vec::new()),
            logs: Mutex::new(Vec::new()),
        }
    }

    pub fn spans(&self) -> Vec<SpanData> {
        self.spans.lock().unwrap().clone()
    }

    pub fn logs(&self) -> Vec<LogEntry> {
        self.logs.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.spans.lock().unwrap().clear();
        self.logs.lock().unwrap().clear();
    }
}

impl Default for MemoryExporter {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryExporter for MemoryExporter {
    fn export_span(&self, span: &SpanData) {
        self.spans.lock().unwrap().push(span.clone());
    }

    fn export_log(&self, entry: &LogEntry) {
        self.logs.lock().unwrap().push(entry.clone());
    }
}
