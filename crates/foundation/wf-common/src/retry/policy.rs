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

/// Retry interception order (budget, hook, approval).
///
/// A retryable failure passes these stages in order; no fourth mechanism
/// is introduced, existing stages are reused:
/// 1. budget: `RetryBudget::can_retry` / policy cap decides capacity;
/// 2. hook observation: the `on_retry` callback fires synchronously with a
///    [`RetryAttemptDescriptor`] (integrators bridge it to a sync hook
///    handler for observation);
/// 3. approval gate: denying integrations return an error from the gate
///    instead of sleeping, which ends the loop without an attempt;
/// 4. attempt: the delay elapses, the operation runs again;
/// 5. async post-processing: the `LLM_RETRY_SCHEDULED` event (same payload
///    keys as the descriptor) is matched by trigger templates after the
///    fact; budget consumption (`RetryConsumed` / `RetryDenied`) is
///    reported through the shared `RetryBudget` event callback.
pub const RETRY_INTERCEPTION_ORDER: &[&str] = &[
    "budget",
    "hook-observation",
    "approval-gate",
    "attempt",
    "async-trigger",
];

/// One scheduled retry attempt: the emission payload shared by the sync
/// `on_retry` callback and the async `LLM_RETRY_SCHEDULED` event metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryAttemptDescriptor {
    /// 1-based attempt number about to run (first retry is 1).
    pub attempt: u32,
    /// Policy cap the attempt counts against.
    pub max_retries: u32,
    /// Delay before the attempt in ms.
    pub delay_ms: u64,
    /// Why the previous attempt is retryable (caller-provided label).
    pub reason: String,
}

impl RetryAttemptDescriptor {
    /// Metadata payload for the async retry event (same keys every
    /// producer emits so trigger conditions match uniformly).
    pub fn event_metadata(&self) -> std::collections::HashMap<String, serde_json::Value> {
        std::collections::HashMap::from([
            ("attempt".to_string(), serde_json::json!(self.attempt)),
            (
                "max_retries".to_string(),
                serde_json::json!(self.max_retries),
            ),
            ("delay_ms".to_string(), serde_json::json!(self.delay_ms)),
            ("reason".to_string(), serde_json::json!(self.reason)),
        ])
    }
}

/// Execute `operation` with retries governed by `policy`. `should_retry`
/// decides whether the latest result warrants another attempt. When `policy`
/// is `None` the operation runs exactly once (fail fast). An optional cancel
/// token interrupts the retry delay, returning the paired error value.
/// `reason` labels the failure for the `on_retry` observation callback.
pub async fn execute_with_retry_observed<F, Fut, T, E>(
    policy: Option<&RetryPolicy>,
    should_retry: impl Fn(&Result<T, E>) -> bool,
    reason: impl Fn(&Result<T, E>) -> String,
    on_retry: Option<&(dyn Fn(&RetryAttemptDescriptor) + Send + Sync)>,
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
        let delay_ms = policy.delay_for_attempt(attempt);
        if let Some(callback) = on_retry {
            callback(&RetryAttemptDescriptor {
                attempt,
                max_retries: policy.max_retries,
                delay_ms,
                reason: reason(&result),
            });
        }
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
impl RetryPolicy {
    /// Delay before `attempt` (1-based) in ms.
    pub fn delay_for_attempt(&self, attempt: u32) -> u64 {
        if self.exponential_backoff {
            self.base_delay_ms * 2u64.pow(attempt.saturating_sub(1))
        } else {
            self.base_delay_ms
        }
    }
}

/// Execute `operation` with retries governed by `policy` (no observation
/// callback; see [`execute_with_retry_observed`] for the interceptable
/// form). Behavior is unchanged: same attempts, delays and cancel rules.
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
    execute_with_retry_observed(
        policy,
        should_retry,
        |_| String::new(),
        None,
        cancel,
        operation,
    )
    .await
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
        use std::sync::atomic::{AtomicU32, Ordering};
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

    #[tokio::test]
    async fn observed_retry_emits_descriptor_before_delay() {
        use std::sync::Mutex;
        let policy = RetryPolicy {
            max_retries: 2,
            base_delay_ms: 10,
            exponential_backoff: true,
        };
        let seen: Arc<Mutex<Vec<RetryAttemptDescriptor>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_cb = seen.clone();
        let result: Result<(), &str> = execute_with_retry_observed(
            Some(&policy),
            |r| r.is_err(),
            |_| "retryable".to_string(),
            Some(&|descriptor| seen_cb.lock().unwrap().push(descriptor.clone())),
            None,
            || async { Err("boom") },
        )
        .await;
        assert_eq!(result, Err("boom"));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].attempt, 1);
        assert_eq!(seen[0].delay_ms, 10);
        assert_eq!(seen[1].attempt, 2);
        assert_eq!(seen[1].delay_ms, 20);
        assert_eq!(seen[0].reason, "retryable");
        let metadata = seen[0].event_metadata();
        assert_eq!(metadata["attempt"], serde_json::json!(1));
        assert_eq!(metadata["max_retries"], serde_json::json!(2));
    }

    #[test]
    fn interception_order_names_five_stages() {
        assert_eq!(
            RETRY_INTERCEPTION_ORDER,
            &[
                "budget",
                "hook-observation",
                "approval-gate",
                "attempt",
                "async-trigger"
            ]
        );
    }
}
