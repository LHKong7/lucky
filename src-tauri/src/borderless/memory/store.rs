//! Memory store trait and types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single memory item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub importance: f64,
    pub created_at: i64,
    pub accessed_at: i64,
    pub access_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f64>>,
    #[serde(default)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// Type of memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Episodic,
    Semantic,
}

/// Trait for memory storage backends.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn load(&self) -> Result<Vec<MemoryItem>, String>;
    async fn save(&self, items: &[MemoryItem]) -> Result<(), String>;
}

/// In-memory store (useful for testing).
pub struct InMemoryStore {
    items: tokio::sync::RwLock<Vec<MemoryItem>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            items: tokio::sync::RwLock::new(Vec::new()),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryStore for InMemoryStore {
    async fn load(&self) -> Result<Vec<MemoryItem>, String> {
        Ok(self.items.read().await.clone())
    }

    async fn save(&self, items: &[MemoryItem]) -> Result<(), String> {
        *self.items.write().await = items.to_vec();
        Ok(())
    }
}
