//! Session store trait and types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::borderless::agent_core::ChatMessage;

/// Persisted session data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub id: String,
    pub state: SessionState,
    pub history: Vec<ChatMessage>,
    pub context: serde_json::Map<String, serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Active,
    Archived,
}

/// Summary of a session (for listing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub state: SessionState,
    pub message_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Trait for session persistence backends.
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn get(&self, session_id: &str) -> Result<Option<SessionData>, String>;
    async fn put(&self, session_id: &str, data: &SessionData) -> Result<(), String>;
    async fn list_ids(&self) -> Result<Vec<String>, String>;
    async fn list_summaries(&self, limit: usize) -> Result<Vec<SessionSummary>, String>;
}
