//! Session manager — creates, restores, and persists sessions.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::Mutex;

use crate::borderless::agent_core::ChatMessage;

use super::store::{SessionData, SessionState, SessionStore, SessionSummary};

/// Manages agent sessions with per-session save locking.
pub struct SessionManager {
    store: Arc<dyn SessionStore>,
    /// Per-session save locks to prevent concurrent writes.
    save_locks: DashMap<String, Arc<Mutex<()>>>,
}

impl SessionManager {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self {
            store,
            save_locks: DashMap::new(),
        }
    }

    /// Create a new session.
    pub async fn create(&self) -> Result<SessionData, String> {
        let now = chrono::Utc::now().timestamp_millis();
        let session = SessionData {
            id: uuid::Uuid::new_v4().to_string(),
            state: SessionState::Active,
            history: Vec::new(),
            context: Default::default(),
            created_at: now,
            updated_at: now,
        };

        self.save(&session).await?;
        Ok(session)
    }

    /// Restore an existing session.
    pub async fn restore(&self, session_id: &str) -> Result<Option<SessionData>, String> {
        self.store.get(session_id).await
    }

    /// Save a session (with per-session locking).
    pub async fn save(&self, session: &SessionData) -> Result<(), String> {
        let lock = self
            .save_locks
            .entry(session.id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();

        let _guard = lock.lock().await;

        let mut data = session.clone();
        data.updated_at = chrono::Utc::now().timestamp_millis();

        self.store.put(&data.id, &data).await
    }

    /// Add a message to a session's history and save.
    pub async fn add_message(
        &self,
        session_id: &str,
        message: ChatMessage,
    ) -> Result<(), String> {
        let mut session = self
            .store
            .get(session_id)
            .await?
            .ok_or_else(|| format!("Session '{}' not found", session_id))?;

        session.history.push(message);
        self.save(&session).await
    }

    /// List all session IDs.
    pub async fn list_ids(&self) -> Result<Vec<String>, String> {
        self.store.list_ids().await
    }

    /// List session summaries.
    pub async fn list_summaries(&self, limit: usize) -> Result<Vec<SessionSummary>, String> {
        self.store.list_summaries(limit).await
    }
}
