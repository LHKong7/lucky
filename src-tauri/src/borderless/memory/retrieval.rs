//! Hybrid keyword + vector retrieval scoring.

use super::store::MemoryItem;

/// Scoring weights for memory retrieval.
pub const ALPHA_RECENCY: f64 = 0.25;
pub const BETA_IMPORTANCE: f64 = 0.35;
pub const GAMMA_RELEVANCE: f64 = 0.40;
pub const DELTA_EMBEDDING: f64 = 0.0; // Increases if embeddings are available

/// Score a memory item against a query.
pub fn score_memory(
    item: &MemoryItem,
    query: &str,
    now_ms: i64,
    query_embedding: Option<&[f64]>,
) -> f64 {
    let recency = compute_recency(item.accessed_at, now_ms);
    let importance = item.importance.min(1.0).max(0.0);
    let relevance = compute_keyword_relevance(&item.content, query);

    let mut score = ALPHA_RECENCY * recency + BETA_IMPORTANCE * importance + GAMMA_RELEVANCE * relevance;

    // Add embedding similarity if both are available
    if let (Some(item_emb), Some(query_emb)) = (&item.embedding, query_embedding) {
        let similarity = crate::borderless::agent_core::cosine_similarity(item_emb, query_emb);
        // Normalized cosine similarity (0-1 range)
        let normalized = (similarity + 1.0) / 2.0;
        // Redistribute weights when embeddings are available
        let delta = 0.30; // Give 30% weight to embeddings
        score = (ALPHA_RECENCY * recency
            + BETA_IMPORTANCE * importance
            + (GAMMA_RELEVANCE - delta / 2.0) * relevance
            + delta * normalized)
            / (1.0 + delta / 2.0);
    }

    score
}

/// Compute recency score (0.0 = ancient, 1.0 = just now).
fn compute_recency(accessed_at: i64, now_ms: i64) -> f64 {
    let age_hours = (now_ms - accessed_at).max(0) as f64 / (1000.0 * 3600.0);
    // Exponential decay with half-life of ~24 hours
    (-age_hours / 24.0_f64).exp()
}

/// Compute keyword relevance using simple TF-like scoring.
fn compute_keyword_relevance(content: &str, query: &str) -> f64 {
    let content_lower = content.to_lowercase();
    let query_words: Vec<&str> = query.split_whitespace().collect();

    if query_words.is_empty() {
        return 0.0;
    }

    let matched = query_words
        .iter()
        .filter(|w| content_lower.contains(&w.to_lowercase()))
        .count();

    matched as f64 / query_words.len() as f64
}

/// Rank memories by relevance to a query and return top-k.
pub fn retrieve_top_k<'a>(
    items: &'a [MemoryItem],
    query: &str,
    k: usize,
    query_embedding: Option<&[f64]>,
) -> Vec<(f64, &'a MemoryItem)> {
    let now = chrono::Utc::now().timestamp_millis();

    let mut scored: Vec<(f64, &MemoryItem)> = items
        .iter()
        .map(|item| (score_memory(item, query, now, query_embedding), item))
        .collect();

    // Sort descending by score
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::store::MemoryType;

    fn make_item(content: &str, importance: f64, age_hours: i64) -> MemoryItem {
        let now = chrono::Utc::now().timestamp_millis();
        MemoryItem {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.into(),
            memory_type: MemoryType::Episodic,
            importance,
            created_at: now - age_hours * 3600 * 1000,
            accessed_at: now - age_hours * 3600 * 1000,
            access_count: 1,
            embedding: None,
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_relevance_scoring() {
        let recent_relevant = make_item("the user prefers Rust for systems programming", 0.8, 1);
        let old_relevant = make_item("the user likes Rust code", 0.5, 100);
        let recent_irrelevant = make_item("the weather is nice today", 0.3, 1);

        let query = "Rust programming";
        let now = chrono::Utc::now().timestamp_millis();

        let s1 = score_memory(&recent_relevant, query, now, None);
        let s2 = score_memory(&old_relevant, query, now, None);
        let s3 = score_memory(&recent_irrelevant, query, now, None);

        // Recent + relevant should score highest
        assert!(s1 > s2);
        assert!(s1 > s3);
    }
}
