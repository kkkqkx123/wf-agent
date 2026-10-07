//! Exponential-backoff retry around fallible chat calls.
//!
//! A lean port of the code-context-engine retry policy: retryable failures
//! (rate limits, 5xx, network errors, timeouts) back off exponentially up to
//! a cap, while client errors return immediately. Observability hooks and
//! jittered staggering from the source project are intentionally left out.

use std::future::Future;
use std::time::Duration;

use crate::error::ChatError;

/// Retry policy with exponential backoff.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    max_retries: u32,
    initial_delay_ms: u64,
    max_delay_ms: u64,
    multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 1000,
            max_delay_ms: 30_000,
            multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Creates a policy with a retry budget and initial delay.
    pub fn new(max_retries: u32, initial_delay_ms: u64) -> Self {
        Self {
            max_retries,
            initial_delay_ms,
            ..Self::default()
        }
    }

    /// Caps the delay between attempts.
    pub fn with_max_delay(mut self, max_delay_ms: u64) -> Self {
        self.max_delay_ms = max_delay_ms;
        self
    }

    /// Sets the exponential backoff multiplier.
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier.max(1.0);
        self
    }

    /// Returns the configured retry budget.
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// Runs `operation` until it succeeds, fails permanently, or exhausts
    /// the retry budget.
    pub async fn execute<F, Fut, T>(&self, mut operation: F) -> Result<T, ChatError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, ChatError>>,
    {
        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    if !err.is_retryable() || attempt == self.max_retries {
                        return Err(err);
                    }
                    tokio::time::sleep(self.delay_for(attempt)).await;
                }
            }
        }
        Err(ChatError::Transport(
            "retry loop exited without a terminal result".into(),
        ))
    }

    /// Computes the capped exponential delay for a zero-based attempt index.
    fn delay_for(&self, attempt: u32) -> Duration {
        let delay_ms = (self.initial_delay_ms as f64 * self.multiplier.powi(attempt as i32))
            .min(self.max_delay_ms as f64) as u64;
        Duration::from_millis(delay_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn succeeds_without_retry() {
        let policy = RetryPolicy::default();
        let calls = AtomicU32::new(0);
        let result = policy
            .execute(|| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ChatError>("ok")
            })
            .await;
        assert_eq!(result.expect("ok"), "ok");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_transient_failures_then_succeeds() {
        let policy = RetryPolicy::new(3, 1);
        let calls = AtomicU32::new(0);
        let result = policy
            .execute(|| async {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                if call < 2 {
                    Err(ChatError::Timeout)
                } else {
                    Ok("recovered")
                }
            })
            .await;
        assert_eq!(result.expect("recovered"), "recovered");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn permanent_errors_return_immediately() {
        let policy = RetryPolicy::new(3, 1);
        let calls = AtomicU32::new(0);
        let result = policy
            .execute(|| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(ChatError::InvalidRequest("bad".into()))
            })
            .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn delay_grows_exponentially_with_cap() {
        let policy = RetryPolicy::new(5, 100).with_max_delay(250);
        assert_eq!(policy.delay_for(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for(1), Duration::from_millis(200));
        assert_eq!(policy.delay_for(2), Duration::from_millis(250));
    }
}
