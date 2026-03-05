//! Retry-aware Provider wrapper
//!
//! Wraps any `Provider` with automatic retry on transient failures.
//! Uses exponential backoff and classifies errors as retryable/permanent.
//!
//! # Usage
//! ```no_run
//! use providers::{openai::OpenAI, retry::RetryProvider};
//!
//! let provider = OpenAI::from_env().unwrap();
//! let resilient = RetryProvider::new(provider)
//!     .max_retries(3)
//!     .base_delay_ms(500);
//! // Use `resilient` as your provider — retries transparently on 429/5xx
//! ```

use async_trait::async_trait;
use std::time::Duration;
use tracing::{debug, warn};

use crate::{Error, Result, Provider, StreamingResponse};
use brain::agent::provider::ChatRequest;

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (excluding the first attempt).
    pub max_retries: u32,
    /// Base delay in milliseconds for exponential backoff.
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds.
    pub max_delay_ms: u64,
    /// Jitter factor (0.0 to 1.0). Adds randomness to delays.
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 30_000,
            jitter_factor: 0.2,
        }
    }
}

/// Wraps any Provider with automatic retry on transient failures.
pub struct RetryProvider<P: Provider> {
    inner: P,
    config: RetryConfig,
}

impl<P: Provider> RetryProvider<P> {
    /// Wrap a provider with default retry settings.
    pub fn new(provider: P) -> Self {
        Self {
            inner: provider,
            config: RetryConfig::default(),
        }
    }

    /// Set maximum retries.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.config.max_retries = n;
        self
    }

    /// Set base delay in milliseconds.
    pub fn base_delay_ms(mut self, ms: u64) -> Self {
        self.config.base_delay_ms = ms;
        self
    }

    /// Set maximum delay cap.
    pub fn max_delay_ms(mut self, ms: u64) -> Self {
        self.config.max_delay_ms = ms;
        self
    }

    /// Calculate delay for a given attempt (with exponential backoff + jitter).
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base = self.config.base_delay_ms as f64 * 2.0_f64.powi(attempt as i32);
        let capped = base.min(self.config.max_delay_ms as f64);

        // Add jitter
        let jitter = capped * self.config.jitter_factor;
        let actual = capped + (rand_simple(attempt) * jitter);

        Duration::from_millis(actual as u64)
    }
}

/// Cheap pseudo-random based on attempt number (no external dependency).
fn rand_simple(seed: u32) -> f64 {
    let x = seed.wrapping_mul(2654435761);
    (x as f64 % 1000.0) / 1000.0
}

/// Classify whether an error is retryable.
fn is_retryable(error: &Error) -> bool {
    match error {
        // HTTP transport errors (network, timeout)
        Error::Http(_) => true,

        // Provider API errors — check for known retryable status codes
        Error::ProviderApi(msg) => {
            let lower = msg.to_lowercase();
            // 429 rate limit
            lower.contains("429")
                // 500 internal server error
                || lower.contains("500")
                // 502 bad gateway
                || lower.contains("502")
                // 503 service unavailable
                || lower.contains("503")
                // 529 API overloaded (Anthropic)
                || lower.contains("529")
                // Generic transient patterns
                || lower.contains("rate limit")
                || lower.contains("overloaded")
                || lower.contains("timeout")
                || lower.contains("temporarily")
        }

        // Auth errors are permanent — never retry
        Error::ProviderAuth(_) => false,

        // Stream interruptions are retryable
        Error::StreamInterrupted(_) => true,

        // Everything else: don't retry
        _ => false,
    }
}

#[async_trait]
impl<P: Provider> Provider for RetryProvider<P> {
    async fn stream_completion(&self, request: ChatRequest) -> Result<StreamingResponse> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = self.delay_for_attempt(attempt - 1);
                debug!(
                    provider = self.inner.name(),
                    attempt = attempt,
                    delay_ms = delay.as_millis() as u64,
                    "Retrying provider request"
                );
                tokio::time::sleep(delay).await;
            }

            match self.inner.stream_completion(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if is_retryable(&e) && attempt < self.config.max_retries {
                        warn!(
                            provider = self.inner.name(),
                            attempt = attempt + 1,
                            max_retries = self.config.max_retries,
                            error = %e,
                            "Retryable error, will retry"
                        );
                        last_error = Some(e);
                        continue;
                    } else {
                        // Non-retryable or exhausted retries
                        return Err(e);
                    }
                }
            }
        }

        // Should not reach here, but just in case
        Err(last_error.unwrap_or_else(|| {
            Error::Internal("All retry attempts exhausted".to_string())
        }))
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_classification() {
        assert!(is_retryable(&Error::ProviderApi("429 rate limit".into())));
        assert!(is_retryable(&Error::ProviderApi("500 internal".into())));
        assert!(is_retryable(&Error::ProviderApi("502 bad gateway".into())));
        assert!(is_retryable(&Error::ProviderApi("503 unavailable".into())));
        assert!(is_retryable(&Error::ProviderApi("529 overloaded".into())));
        assert!(is_retryable(&Error::StreamInterrupted("broken".into())));

        assert!(!is_retryable(&Error::ProviderAuth("invalid key".into())));
        assert!(!is_retryable(&Error::ProviderApi("401 unauthorized".into())));
        assert!(!is_retryable(&Error::ProviderApi("400 bad request".into())));
    }

    #[test]
    fn test_delay_backoff() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            jitter_factor: 0.0, // No jitter for deterministic testing
            ..Default::default()
        };

        let provider = RetryProvider {
            inner: crate::mock::MockProvider::new(),
            config,
        };

        // Attempt 0: 1000ms * 2^0 = 1000ms
        let d0 = provider.delay_for_attempt(0);
        assert_eq!(d0.as_millis(), 1000);

        // Attempt 1: 1000ms * 2^1 = 2000ms
        let d1 = provider.delay_for_attempt(1);
        assert_eq!(d1.as_millis(), 2000);

        // Attempt 2: 1000ms * 2^2 = 4000ms
        let d2 = provider.delay_for_attempt(2);
        assert_eq!(d2.as_millis(), 4000);
    }
}
