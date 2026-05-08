//! Token estimation utilities.

/// Estimate the number of tokens in a text using the fast 1/3 character heuristic.
///
/// This is intentionally approximate (1 token ~ 3 chars). For more accurate
/// counting, use a dedicated tokenizer like tiktoken-rs.
pub fn estimate_tokens(text: &str) -> usize {
    // Ceiling division to err on the side of overestimating
    (text.len() + 2) / 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_empty() {
        // Empty string should be ~0 tokens (with rounding)
        assert!(estimate_tokens("") <= 1);
    }

    #[test]
    fn test_estimate_tokens_short() {
        // "hello" = 5 chars -> ~2 tokens
        let tokens = estimate_tokens("hello");
        assert!(tokens >= 1 && tokens <= 3);
    }

    #[test]
    fn test_estimate_tokens_longer() {
        // 300 chars -> ~100 tokens
        let text = "a".repeat(300);
        let tokens = estimate_tokens(&text);
        assert!((tokens as i64 - 100).abs() < 5);
    }
}
