//! Model pricing lookup table for cost estimation.
//!
//! Prices in USD per million tokens. Users can override via `set_model_pricing()`.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// Pricing for a model in USD per 1M tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// USD per 1M input tokens.
    pub input: f64,
    /// USD per 1M output tokens.
    pub output: f64,
    /// USD per 1M cached read input tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    /// USD per 1M cache creation input tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation: Option<f64>,
}

/// Token usage from an LLM call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_tokens: Option<u64>,
}

// ---------------------------------------------------------------------------
// Default pricing table
// ---------------------------------------------------------------------------

fn default_pricing() -> HashMap<String, ModelPricing> {
    let mut m = HashMap::new();

    // OpenAI
    m.insert("gpt-4o".into(), ModelPricing { input: 2.50, output: 10.00, cache_read: None, cache_creation: None });
    m.insert("gpt-4o-mini".into(), ModelPricing { input: 0.15, output: 0.60, cache_read: None, cache_creation: None });
    m.insert("gpt-4-turbo".into(), ModelPricing { input: 10.00, output: 30.00, cache_read: None, cache_creation: None });
    m.insert("gpt-4".into(), ModelPricing { input: 30.00, output: 60.00, cache_read: None, cache_creation: None });
    m.insert("gpt-3.5-turbo".into(), ModelPricing { input: 0.50, output: 1.50, cache_read: None, cache_creation: None });
    m.insert("o1".into(), ModelPricing { input: 15.00, output: 60.00, cache_read: None, cache_creation: None });
    m.insert("o1-mini".into(), ModelPricing { input: 3.00, output: 12.00, cache_read: None, cache_creation: None });
    m.insert("o3".into(), ModelPricing { input: 10.00, output: 40.00, cache_read: None, cache_creation: None });
    m.insert("o3-mini".into(), ModelPricing { input: 1.10, output: 4.40, cache_read: None, cache_creation: None });
    m.insert("o4-mini".into(), ModelPricing { input: 1.10, output: 4.40, cache_read: None, cache_creation: None });

    // Anthropic
    m.insert("claude-opus-4".into(), ModelPricing { input: 15.00, output: 75.00, cache_read: Some(1.50), cache_creation: Some(18.75) });
    m.insert("claude-sonnet-4".into(), ModelPricing { input: 3.00, output: 15.00, cache_read: Some(0.30), cache_creation: Some(3.75) });
    m.insert("claude-3-5-sonnet".into(), ModelPricing { input: 3.00, output: 15.00, cache_read: Some(0.30), cache_creation: Some(3.75) });
    m.insert("claude-3-5-haiku".into(), ModelPricing { input: 0.80, output: 4.00, cache_read: Some(0.08), cache_creation: Some(1.00) });
    m.insert("claude-3-opus".into(), ModelPricing { input: 15.00, output: 75.00, cache_read: None, cache_creation: None });
    m.insert("claude-3-sonnet".into(), ModelPricing { input: 3.00, output: 15.00, cache_read: None, cache_creation: None });
    m.insert("claude-3-haiku".into(), ModelPricing { input: 0.25, output: 1.25, cache_read: None, cache_creation: None });

    // Google
    m.insert("gemini-2.5-pro".into(), ModelPricing { input: 1.25, output: 10.00, cache_read: None, cache_creation: None });
    m.insert("gemini-2.5-flash".into(), ModelPricing { input: 0.15, output: 0.60, cache_read: None, cache_creation: None });
    m.insert("gemini-2.0-flash".into(), ModelPricing { input: 0.10, output: 0.40, cache_read: None, cache_creation: None });
    m.insert("gemini-1.5-pro".into(), ModelPricing { input: 1.25, output: 5.00, cache_read: None, cache_creation: None });
    m.insert("gemini-1.5-flash".into(), ModelPricing { input: 0.075, output: 0.30, cache_read: None, cache_creation: None });

    m
}

static CUSTOM_PRICING: RwLock<Option<HashMap<String, ModelPricing>>> = RwLock::new(None);

/// Set custom model pricing (merged with defaults, overrides take precedence).
pub fn set_model_pricing(pricing: HashMap<String, ModelPricing>) {
    let mut guard = CUSTOM_PRICING.write().unwrap();
    let existing = guard.get_or_insert_with(HashMap::new);
    existing.extend(pricing);
}

/// Look up pricing for a model. Matches by prefix (longest match wins).
pub fn get_model_pricing(model: &str) -> Option<ModelPricing> {
    let defaults = default_pricing();
    let custom = CUSTOM_PRICING.read().unwrap();
    let lower = model.to_lowercase();

    // Build merged map: defaults + custom overrides
    let check = |key: &str| -> Option<ModelPricing> {
        if let Some(ref c) = *custom {
            if let Some(p) = c.get(key) {
                return Some(p.clone());
            }
        }
        defaults.get(key).cloned()
    };

    // Exact match first
    if let Some(p) = check(&lower) {
        return Some(p);
    }

    // Prefix match (longest first)
    let mut best_match: Option<(usize, ModelPricing)> = None;

    let all_keys: Vec<String> = {
        let mut keys: Vec<String> = defaults.keys().cloned().collect();
        if let Some(ref c) = *custom {
            keys.extend(c.keys().cloned());
        }
        keys.sort();
        keys.dedup();
        keys
    };

    for key in &all_keys {
        if lower.starts_with(key.as_str()) {
            let len = key.len();
            if best_match.as_ref().map_or(true, |(best_len, _)| len > *best_len) {
                if let Some(p) = check(key) {
                    best_match = Some((len, p));
                }
            }
        }
    }

    best_match.map(|(_, p)| p)
}

/// Calculate estimated cost in USD from token usage and model.
pub fn estimate_cost(usage: &TokenUsage, model: &str) -> f64 {
    let pricing = match get_model_pricing(model) {
        Some(p) => p,
        None => return 0.0,
    };

    let per_m = 1_000_000.0;
    let mut cost = 0.0;

    // Input cost (subtract cached tokens from regular input)
    let cache_read = usage.cache_read_tokens.unwrap_or(0);
    let regular_input = usage.input_tokens.saturating_sub(cache_read);
    cost += (regular_input as f64 / per_m) * pricing.input;

    // Cache read cost
    if let (Some(cache_tokens), Some(cache_price)) = (usage.cache_read_tokens, pricing.cache_read) {
        if cache_tokens > 0 {
            cost += (cache_tokens as f64 / per_m) * cache_price;
        }
    }

    // Cache creation cost
    if let (Some(cache_tokens), Some(cache_price)) = (usage.cache_creation_tokens, pricing.cache_creation) {
        if cache_tokens > 0 {
            cost += (cache_tokens as f64 / per_m) * cache_price;
        }
    }

    // Output cost
    cost += (usage.output_tokens as f64 / per_m) * pricing.output;

    cost.max(0.0)
}

/// Convert raw LLM usage into TokenUsage.
pub fn to_token_usage(raw: &HashMap<String, u64>) -> TokenUsage {
    let input = raw.get("input_tokens").or(raw.get("prompt_tokens")).copied().unwrap_or(0);
    let output = raw.get("output_tokens").or(raw.get("completion_tokens")).copied().unwrap_or(0);
    let cache_read = raw.get("cache_read_input_tokens").copied().unwrap_or(0);
    let cache_creation = raw.get("cache_creation_input_tokens").copied().unwrap_or(0);

    TokenUsage {
        input_tokens: input,
        output_tokens: output,
        total_tokens: input + output + cache_creation,
        cache_read_tokens: if cache_read > 0 { Some(cache_read) } else { None },
        cache_creation_tokens: if cache_creation > 0 { Some(cache_creation) } else { None },
    }
}

/// Merge two TokenUsage objects (accumulate).
pub fn merge_token_usage(a: &TokenUsage, b: &TokenUsage) -> TokenUsage {
    let cache_read = match (a.cache_read_tokens, b.cache_read_tokens) {
        (Some(x), Some(y)) => Some(x + y),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };
    let cache_creation = match (a.cache_creation_tokens, b.cache_creation_tokens) {
        (Some(x), Some(y)) => Some(x + y),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };

    TokenUsage {
        input_tokens: a.input_tokens + b.input_tokens,
        output_tokens: a.output_tokens + b.output_tokens,
        total_tokens: a.total_tokens + b.total_tokens,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_model_pricing_exact() {
        let p = get_model_pricing("gpt-4o").unwrap();
        assert!((p.input - 2.50).abs() < 0.01);
    }

    #[test]
    fn test_get_model_pricing_prefix() {
        let p = get_model_pricing("gpt-4o-2024-05-13").unwrap();
        assert!((p.input - 2.50).abs() < 0.01);
    }

    #[test]
    fn test_get_model_pricing_unknown() {
        assert!(get_model_pricing("totally-unknown-model").is_none());
    }

    #[test]
    fn test_estimate_cost_basic() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            total_tokens: 2_000_000,
            ..Default::default()
        };
        let cost = estimate_cost(&usage, "gpt-4o");
        assert!((cost - 12.50).abs() < 0.01); // 2.50 + 10.00
    }

    #[test]
    fn test_merge_token_usage() {
        let a = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            cache_read_tokens: Some(10),
            cache_creation_tokens: None,
        };
        let b = TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
            total_tokens: 300,
            cache_read_tokens: Some(20),
            cache_creation_tokens: Some(5),
        };
        let merged = merge_token_usage(&a, &b);
        assert_eq!(merged.input_tokens, 300);
        assert_eq!(merged.output_tokens, 150);
        assert_eq!(merged.cache_read_tokens, Some(30));
        assert_eq!(merged.cache_creation_tokens, Some(5));
    }
}
