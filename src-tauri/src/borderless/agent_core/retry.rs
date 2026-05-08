//! Generic async retry with exponential backoff.

use std::future::Future;
use std::time::Duration;

use super::LlmError;

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryOptions {
    /// Total number of attempts (initial + retries). Default: 3.
    pub max_attempts: u32,
    /// Base delay between retries. Default: 1 second.
    pub base_delay: Duration,
    /// HTTP status codes considered retryable. Default: [429, 500, 502, 503].
    pub retryable_statuses: Vec<u16>,
}

impl Default for RetryOptions {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_secs(1),
            retryable_statuses: vec![429, 500, 502, 503],
        }
    }
}

/// Error context extracted from a failed HTTP request.
pub struct HttpErrorInfo {
    pub status: Option<u16>,
    pub message: String,
    pub retry_after: Option<Duration>,
}

/// Wrap an async operation with retry + exponential backoff.
///
/// The `extract_error` closure converts the raw error into `HttpErrorInfo`
/// so the retry logic can inspect status codes.
pub async fn with_retry<T, E, F, Fut, Extract>(
    mut operation: F,
    extract_error: Extract,
    opts: Option<&RetryOptions>,
) -> Result<T, LlmError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    Extract: Fn(&E) -> HttpErrorInfo,
{
    let defaults = RetryOptions::default();
    let opts = opts.unwrap_or(&defaults);

    for attempt in 1..=opts.max_attempts {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                let info = extract_error(&e);
                let status = info.status;

                // Authentication errors are never retryable
                if status == Some(401) || status == Some(403) {
                    return Err(LlmError::Authentication(info.message));
                }

                let retryable = status
                    .map(|s| opts.retryable_statuses.contains(&s))
                    .unwrap_or(false);
                let has_more = attempt < opts.max_attempts;

                if has_more && retryable {
                    let delay = if status == Some(429) {
                        info.retry_after
                            .unwrap_or_else(|| opts.base_delay * 2u32.pow(attempt - 1))
                    } else {
                        opts.base_delay * 2u32.pow(attempt - 1)
                    };
                    tokio::time::sleep(delay).await;
                    continue;
                }

                if status == Some(429) {
                    return Err(LlmError::RateLimit {
                        retry_after: info.retry_after,
                    });
                }

                if retryable {
                    return Err(LlmError::RetryExhausted {
                        attempts: opts.max_attempts,
                        message: info.message,
                    });
                }

                return Err(LlmError::Request(info.message));
            }
        }
    }

    unreachable!("loop always returns or throws")
}
