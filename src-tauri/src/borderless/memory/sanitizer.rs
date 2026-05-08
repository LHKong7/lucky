//! Credential sanitization for memory items.

use regex::Regex;

/// Patterns that match sensitive credentials.
static CREDENTIAL_PATTERNS: &[(&str, &str)] = &[
    (r"(?i)(sk-[a-zA-Z0-9]{20,})", "[REDACTED_API_KEY]"),
    (r"(?i)(key-[a-zA-Z0-9]{20,})", "[REDACTED_API_KEY]"),
    (r"(?i)api[_-]?key\s*[=:]\s*\S+", "api_key=[REDACTED]"),
    (r"(?i)password\s*[=:]\s*\S+", "password=[REDACTED]"),
    (r"(?i)(AKIA[0-9A-Z]{16})", "[REDACTED_AWS_KEY]"),
    (r"eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}", "[REDACTED_JWT]"),
    (r"(?i)bearer\s+[a-zA-Z0-9_\-\.]{20,}", "bearer [REDACTED]"),
];

/// Sanitize sensitive credentials from text.
pub fn sanitize_credentials(text: &str) -> String {
    let mut result = text.to_string();

    for &(pattern, replacement) in CREDENTIAL_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            result = re.replace_all(&result, replacement).to_string();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_api_key() {
        let input = "my key is sk-abcdefghijklmnopqrstuvwxyz123";
        let result = sanitize_credentials(input);
        assert!(!result.contains("sk-abcdefghijklmnopqrstuvwxyz123"));
        assert!(result.contains("[REDACTED_API_KEY]"));
    }

    #[test]
    fn test_sanitize_jwt() {
        let input = "token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let result = sanitize_credentials(input);
        assert!(result.contains("[REDACTED_JWT]"));
    }

    #[test]
    fn test_no_false_positive() {
        let input = "the function returns a short string";
        let result = sanitize_credentials(input);
        assert_eq!(result, input);
    }
}
