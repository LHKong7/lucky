//! SSE (Server-Sent Events) parsing utilities.
//!
//! Shared across provider implementations for parsing streaming responses.

/// A parsed SSE event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Parse a single SSE message from raw text.
/// Returns None for empty lines or comments.
pub fn parse_sse_line(line: &str) -> Option<SseEvent> {
    let line = line.trim();

    if line.is_empty() || line.starts_with(':') {
        return None;
    }

    if let Some(data) = line.strip_prefix("data: ") {
        Some(SseEvent {
            event: None,
            data: data.to_string(),
        })
    } else if line.starts_with("data:") {
        Some(SseEvent {
            event: None,
            data: line[5..].to_string(),
        })
    } else {
        None
    }
}
