//! Typed error hierarchy for the borderless-agent SDK.
//!
//! Provides structured errors for LLM calls, tool execution, validation,
//! and configuration. Uses `thiserror` for idiomatic Rust error handling.

use std::time::Duration;

use serde::Serialize;

// ---------------------------------------------------------------------------
// Top-level agent error
// ---------------------------------------------------------------------------

/// Top-level error type that encompasses all borderless-agent errors.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    Llm(#[from] LlmError),
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Session error: {0}")]
    Session(String),
    #[error("{0}")]
    Other(String),
}

impl AgentError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Llm(e) => e.is_retryable(),
            _ => false,
        }
    }
}

impl Serialize for AgentError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("error", &self.to_string())?;
        map.serialize_entry("code", &self.error_code())?;
        map.end()
    }
}

impl AgentError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Llm(e) => e.error_code(),
            Self::Tool(e) => e.error_code(),
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Configuration(_) => "CONFIGURATION_ERROR",
            Self::Storage(_) => "STORAGE_ERROR",
            Self::Session(_) => "SESSION_ERROR",
            Self::Other(_) => "UNKNOWN",
        }
    }
}

// ---------------------------------------------------------------------------
// LLM errors
// ---------------------------------------------------------------------------

/// Errors from LLM provider calls.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("Rate limited (retry after {retry_after:?})")]
    RateLimit {
        retry_after: Option<Duration>,
    },
    #[error("Authentication failed: {0}")]
    Authentication(String),
    #[error("Context overflow: {token_count} tokens exceeds budget of {budget}")]
    ContextOverflow {
        token_count: usize,
        budget: usize,
    },
    #[error("LLM call failed: {0}")]
    Request(String),
    #[error("Retries exhausted after {attempts} attempts: {message}")]
    RetryExhausted {
        attempts: u32,
        message: String,
    },
}

impl LlmError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimit { .. })
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            Self::RateLimit { .. } => "RATE_LIMIT",
            Self::Authentication(_) => "AUTH_ERROR",
            Self::ContextOverflow { .. } => "CONTEXT_OVERFLOW",
            Self::Request(_) => "LLM_ERROR",
            Self::RetryExhausted { .. } => "LLM_RETRY_EXHAUSTED",
        }
    }
}

// ---------------------------------------------------------------------------
// Tool errors
// ---------------------------------------------------------------------------

/// Errors from tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool '{name}' timed out after {timeout:?}")]
    Timeout {
        name: String,
        timeout: Duration,
    },
    #[error("Tool '{name}' execution failed: {message}")]
    Execution {
        name: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    #[error("Tool '{name}' not found")]
    NotFound {
        name: String,
    },
    #[error("Permission denied for tool '{name}': {reason}")]
    PermissionDenied {
        name: String,
        reason: String,
    },
    #[error("User denied tool '{name}'")]
    UserDenied {
        name: String,
    },
}

impl ToolError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Timeout { .. } => "TOOL_TIMEOUT",
            Self::Execution { .. } => "TOOL_EXECUTION",
            Self::NotFound { .. } => "TOOL_NOT_FOUND",
            Self::PermissionDenied { .. } => "TOOL_PERMISSION_DENIED",
            Self::UserDenied { .. } => "TOOL_USER_DENIED",
        }
    }

    pub fn tool_name(&self) -> &str {
        match self {
            Self::Timeout { name, .. }
            | Self::Execution { name, .. }
            | Self::NotFound { name }
            | Self::PermissionDenied { name, .. }
            | Self::UserDenied { name } => name,
        }
    }
}

// ---------------------------------------------------------------------------
// Result alias
// ---------------------------------------------------------------------------

/// Convenience alias for `Result<T, AgentError>`.
pub type AgentResult<T> = Result<T, AgentError>;
