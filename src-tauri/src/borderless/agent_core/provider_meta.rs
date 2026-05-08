//! Provider metadata: names and context window lookup.

use serde::{Deserialize, Serialize};

/// Supported LLM provider names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderName {
    OpenAI,
    Anthropic,
    Google,
}

impl std::fmt::Display for ProviderName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenAI => write!(f, "openai"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::Google => write!(f, "google"),
        }
    }
}

/// Context window sizes by model regex pattern.
static CONTEXT_WINDOWS: &[(&str, usize)] = &[
    // OpenAI
    ("gpt-4o", 128_000),
    ("gpt-4-turbo", 128_000),
    ("gpt-4-0125", 128_000),
    ("gpt-4-1106", 128_000),
    ("gpt-3.5-turbo-16k", 16_384),
    ("gpt-3.5-turbo", 16_384),
    ("o1-mini", 128_000),
    ("o1-preview", 128_000),
    ("o1", 200_000),
    ("o3", 200_000),
    ("o4-mini", 200_000),
    // Anthropic
    ("claude-opus-4", 200_000),
    ("claude-sonnet-4", 200_000),
    ("claude-3-7-sonnet", 200_000),
    ("claude-3-5-sonnet", 200_000),
    ("claude-3-5-haiku", 200_000),
    ("claude-3-opus", 200_000),
    ("claude-3-sonnet", 200_000),
    ("claude-3-haiku", 200_000),
    // Google
    ("gemini-2.5", 1_000_000),
    ("gemini-2.0-flash", 1_000_000),
    ("gemini-1.5-pro", 2_000_000),
    ("gemini-1.5-flash", 1_000_000),
    ("gemini-pro", 32_000),
];

/// Look up context window size for a model string.
/// Falls back to `default_size` (128k) if no match.
pub fn get_context_window_for_model(model: &str, default_size: Option<usize>) -> usize {
    let default = default_size.unwrap_or(128_000);
    let lower = model.to_lowercase();

    // Special tag for 1M context
    if lower.contains("[1m]") {
        return 1_000_000;
    }

    for &(pattern, size) in CONTEXT_WINDOWS {
        if lower.contains(pattern) {
            return size;
        }
    }

    default
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_window_gpt4o() {
        assert_eq!(get_context_window_for_model("gpt-4o", None), 128_000);
    }

    #[test]
    fn test_context_window_claude() {
        assert_eq!(get_context_window_for_model("claude-3-5-sonnet-20241022", None), 200_000);
    }

    #[test]
    fn test_context_window_1m_tag() {
        assert_eq!(get_context_window_for_model("claude-opus-4-6[1m]", None), 1_000_000);
    }

    #[test]
    fn test_context_window_unknown() {
        assert_eq!(get_context_window_for_model("unknown-model", None), 128_000);
    }

    #[test]
    fn test_context_window_custom_default() {
        assert_eq!(get_context_window_for_model("unknown-model", Some(32_000)), 32_000);
    }
}
