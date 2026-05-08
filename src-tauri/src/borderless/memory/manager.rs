//! Memory manager — coordinates storage, retrieval, and garbage collection.

use std::sync::Arc;

use crate::borderless::agent_core::EmbeddingProvider;
use tokio::sync::RwLock;

use super::retrieval;
use super::sanitizer;
use super::store::{MemoryItem, MemoryStore, MemoryType};

/// Configuration for the memory manager.
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Maximum number of memory items to retain. Default: 500.
    pub max_items: usize,
    /// TTL for memory items in days. Default: 90.
    pub ttl_days: u64,
    /// Number of items to retrieve per query. Default: 10.
    pub top_k: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_items: 500,
            ttl_days: 90,
            top_k: 10,
        }
    }
}

/// Manages long-term episodic and semantic memory.
pub struct MemoryManager {
    store: Box<dyn MemoryStore>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    config: MemoryConfig,
    items: RwLock<Vec<MemoryItem>>,
}

impl MemoryManager {
    pub async fn new(
        store: Box<dyn MemoryStore>,
        embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
        config: MemoryConfig,
    ) -> Result<Self, String> {
        let items = store.load().await?;
        Ok(Self {
            store,
            embedding_provider,
            config,
            items: RwLock::new(items),
        })
    }

    /// Add a new memory item.
    pub async fn add(
        &self,
        content: &str,
        memory_type: MemoryType,
        importance: f64,
    ) -> Result<String, String> {
        let sanitized = sanitizer::sanitize_credentials(content);
        let now = chrono::Utc::now().timestamp_millis();

        let embedding = if let Some(ref provider) = self.embedding_provider {
            provider.embed(&sanitized).await.ok()
        } else {
            None
        };

        let item = MemoryItem {
            id: uuid::Uuid::new_v4().to_string(),
            content: sanitized,
            memory_type,
            importance: importance.min(1.0).max(0.0),
            created_at: now,
            accessed_at: now,
            access_count: 0,
            embedding,
            metadata: Default::default(),
        };

        let id = item.id.clone();
        let mut items = self.items.write().await;
        items.push(item);

        // Enforce max items
        if items.len() > self.config.max_items {
            // Remove oldest items
            items.sort_by_key(|i| i.accessed_at);
            let remove_count = items.len() - self.config.max_items;
            items.drain(0..remove_count);
        }

        // Persist
        self.store.save(&items).await?;

        Ok(id)
    }

    /// Retrieve relevant memories for a query.
    pub async fn retrieve(&self, query: &str) -> Vec<(f64, MemoryItem)> {
        let query_embedding = if let Some(ref provider) = self.embedding_provider {
            provider.embed(query).await.ok()
        } else {
            None
        };

        let items = self.items.read().await;
        let results = retrieval::retrieve_top_k(
            &items,
            query,
            self.config.top_k,
            query_embedding.as_deref(),
        );

        results
            .into_iter()
            .map(|(score, item)| (score, item.clone()))
            .collect()
    }

    /// Run garbage collection (remove expired items).
    pub async fn gc(&self) -> Result<usize, String> {
        let ttl_ms = self.config.ttl_days as i64 * 24 * 3600 * 1000;
        let now = chrono::Utc::now().timestamp_millis();
        let cutoff = now - ttl_ms;

        let mut items = self.items.write().await;
        let before = items.len();
        items.retain(|item| item.created_at > cutoff);
        let removed = before - items.len();

        if removed > 0 {
            self.store.save(&items).await?;
        }

        Ok(removed)
    }

    /// Get the current number of stored memories.
    pub async fn count(&self) -> usize {
        self.items.read().await.len()
    }
}
