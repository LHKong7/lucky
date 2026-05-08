//! Context assembly with token budget enforcement.
//!
//! Registers context sources with priorities, then assembles them into a
//! system prompt that fits within the configured token budget.

use crate::borderless::agent_core::estimate_tokens;

/// Category of a context source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextCategory {
    System,
    Project,
    Preferences,
    Rag,
    Summary,
    Skill,
    Pattern,
    Instruction,
}

/// A single source of context to be injected into the system prompt.
#[derive(Debug, Clone)]
pub struct ContextSource {
    pub name: String,
    pub content: String,
    pub priority: f64,
    pub category: ContextCategory,
    pub max_tokens: Option<usize>,
    pub title: Option<String>,
}

/// Result of context assembly.
#[derive(Debug, Clone)]
pub struct AssemblyResult {
    /// The final assembled system prompt.
    pub system_prompt: String,
    /// Token count of the assembled prompt.
    pub token_count: usize,
    /// Sources that were included.
    pub included: Vec<String>,
    /// Sources that were truncated.
    pub truncated: Vec<String>,
    /// Sources that were dropped (didn't fit).
    pub dropped: Vec<String>,
}

/// Registry of context sources that can be assembled into a system prompt.
pub struct SourceRegistry {
    sources: Vec<ContextSource>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Register a context source.
    pub fn register(&mut self, source: ContextSource) {
        self.sources.push(source);
    }

    /// Get all registered sources.
    pub fn sources(&self) -> &[ContextSource] {
        &self.sources
    }
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a system prompt from registered context sources within a token budget.
pub struct ContextBuilder {
    base_prompt: String,
    token_budget: usize,
}

impl ContextBuilder {
    pub fn new(base_prompt: String, token_budget: usize) -> Self {
        Self {
            base_prompt,
            token_budget,
        }
    }

    /// Assemble context sources into a final system prompt.
    pub fn assemble(&self, registry: &SourceRegistry) -> AssemblyResult {
        let mut parts = vec![self.base_prompt.clone()];
        let mut used_tokens = estimate_tokens(&self.base_prompt);
        let mut included = Vec::new();
        let mut truncated = Vec::new();
        let mut dropped = Vec::new();

        // Sort by priority descending
        let mut sources: Vec<&ContextSource> = registry.sources().iter().collect();
        sources.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));

        for source in sources {
            let source_tokens = estimate_tokens(&source.content);
            let max = source.max_tokens.unwrap_or(usize::MAX);
            let available = self.token_budget.saturating_sub(used_tokens);

            if available == 0 {
                dropped.push(source.name.clone());
                continue;
            }

            let allowed = available.min(max);

            if source_tokens <= allowed {
                // Include fully
                let header = source
                    .title
                    .as_ref()
                    .map(|t| format!("\n\n## {}\n", t))
                    .unwrap_or_else(|| "\n\n".to_string());
                parts.push(format!("{}{}", header, source.content));
                used_tokens += source_tokens + estimate_tokens(&header);
                included.push(source.name.clone());
            } else if allowed > 50 {
                // Truncate: take approximately `allowed` tokens worth of chars
                let char_limit = allowed * 3; // inverse of the 1/3 heuristic
                let truncated_content: String = source.content.chars().take(char_limit).collect();
                let header = source
                    .title
                    .as_ref()
                    .map(|t| format!("\n\n## {}\n", t))
                    .unwrap_or_else(|| "\n\n".to_string());
                parts.push(format!("{}{}...", header, truncated_content));
                used_tokens += allowed;
                truncated.push(source.name.clone());
            } else {
                dropped.push(source.name.clone());
            }
        }

        AssemblyResult {
            system_prompt: parts.join(""),
            token_count: used_tokens,
            included,
            truncated,
            dropped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assembly_basic() {
        let mut registry = SourceRegistry::new();
        registry.register(ContextSource {
            name: "test".into(),
            content: "Hello world".into(),
            priority: 1.0,
            category: ContextCategory::System,
            max_tokens: None,
            title: Some("Test".into()),
        });

        let builder = ContextBuilder::new("Base prompt.".into(), 10000);
        let result = builder.assemble(&registry);
        assert!(result.system_prompt.contains("Base prompt."));
        assert!(result.system_prompt.contains("Hello world"));
        assert_eq!(result.included, vec!["test"]);
        assert!(result.dropped.is_empty());
    }

    #[test]
    fn test_assembly_drops_over_budget() {
        let mut registry = SourceRegistry::new();
        registry.register(ContextSource {
            name: "big".into(),
            content: "x".repeat(30000), // ~10000 tokens
            priority: 1.0,
            category: ContextCategory::System,
            max_tokens: None,
            title: None,
        });

        let builder = ContextBuilder::new("Base.".into(), 100); // tiny budget
        let result = builder.assemble(&registry);
        // Should be truncated or dropped
        assert!(result.truncated.contains(&"big".to_string()) || result.dropped.contains(&"big".to_string()));
    }
}
