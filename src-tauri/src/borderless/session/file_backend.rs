//! File-based session storage backend.

use std::path::PathBuf;

use async_trait::async_trait;

use super::store::{SessionData, SessionStore, SessionSummary};

/// File-based session store. Each session is a JSON file.
pub struct FileSessionStore {
    dir: PathBuf,
}

impl FileSessionStore {
    pub async fn new(dir: impl Into<PathBuf>) -> Result<Self, String> {
        let dir = dir.into();
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| format!("Failed to create session dir: {}", e))?;
        Ok(Self { dir })
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", session_id))
    }
}

#[async_trait]
impl SessionStore for FileSessionStore {
    async fn get(&self, session_id: &str) -> Result<Option<SessionData>, String> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read session: {}", e))?;

        let data: SessionData = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse session: {}", e))?;

        Ok(Some(data))
    }

    async fn put(&self, session_id: &str, data: &SessionData) -> Result<(), String> {
        let path = self.session_path(session_id);

        // Atomic write: write to tmp then rename
        let tmp_path = path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| format!("Failed to serialize session: {}", e))?;

        tokio::fs::write(&tmp_path, &content)
            .await
            .map_err(|e| format!("Failed to write session: {}", e))?;

        tokio::fs::rename(&tmp_path, &path)
            .await
            .map_err(|e| format!("Failed to rename session file: {}", e))?;

        Ok(())
    }

    async fn list_ids(&self) -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.dir)
            .await
            .map_err(|e| format!("Failed to read session dir: {}", e))?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with(".json") && !name_str.ends_with(".tmp") {
                ids.push(name_str.trim_end_matches(".json").to_string());
            }
        }

        Ok(ids)
    }

    async fn list_summaries(&self, limit: usize) -> Result<Vec<SessionSummary>, String> {
        let ids = self.list_ids().await?;
        let mut summaries = Vec::new();

        for id in ids.iter().take(limit) {
            if let Ok(Some(data)) = self.get(id).await {
                summaries.push(SessionSummary {
                    id: data.id,
                    state: data.state,
                    message_count: data.history.len(),
                    created_at: data.created_at,
                    updated_at: data.updated_at,
                });
            }
        }

        // Sort by updated_at descending
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }
}
