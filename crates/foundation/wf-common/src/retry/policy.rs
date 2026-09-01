use std::time::Duration;

use tokio_util::sync::CancellationToken;

/// Retry policy: maximum retry count, base delay and whether the delay grows
/// exponentially (`base * 2^(attempt-1)`).
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub exponential_backoff: bool,
}

/// Execute `operation` with retries governed by `policy`. `should_retry`
/// decides whether the latest result warrants another attempt. When `policy`
/// is `None` the operation runs exactly once (fail fast). An optional cancel
/// token interrupts the retry delay, returning the paired error value.
pub async fn execute_with_retry<F, Fut, T, E>(
    policy: Option<&RetryPolicy>,
    should_retry: impl Fn(&Result<T, E>) -> bool,
    cancel: Option<(&CancellationToken, E)>,
    operation: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let Some(policy) = policy else {
        return operation().await;
    };
    let (mut cancel_err, cancel_token) = match cancel {
        Some((token, err)) => (Some(err), Some(token)),
        None => (None, None),
    };
    let mut attempt = 0u32;
    loop {
        let result = operation().await;
        if !should_retry(&result) || attempt >= policy.max_retries {
            return result;
        }
        attempt += 1;
        let delay_ms = if policy.exponential_backoff {
            policy.base_delay_ms * 2u64.pow(attempt - 1)
        } else {
            policy.base_delay_ms
        };
        if let Some(token) = &cancel_token {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                _ = token.cancelled() => {
                    return Err(cancel_err.take().expect("cancel token implies error"));
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn execute_with_retry_stops_when_policy_exhausted() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let policy = RetryPolicy {
            max_retries: 2,
            base_delay_ms: 1,
            exponential_backoff: false,
        };
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_cb = attempts.clone();
        let result: Result<(), &str> = execute_with_retry(
            Some(&policy),
            |r| r.is_err(),
            None,
            move || {
                let attempts = attempts_cb.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err("boom")
                }
            },
        )
        .await;
        assert_eq!(result, Err("boom"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn execute_with_retry_without_policy_fails_fast() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_cb = attempts.clone();
        let result: Result<(), &str> = execute_with_retry(
            None,
            |_| true,
            None,
            move || {
                let attempts = attempts_cb.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err("boom")
                }
            },
        )
        .await;
        assert_eq!(result, Err("boom"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
