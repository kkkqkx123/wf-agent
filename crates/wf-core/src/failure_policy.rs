use std::time::Duration;

use serde_json::Value;
use wf_types::execution::{FailureAction, FailurePolicyConfig, FallbackPolicy, RetryPolicy};

use wf_types::ErrorSeverity;

#[derive(Debug, Clone)]
pub struct FailurePolicyManager {
    config: FailurePolicyConfig,
}

impl FailurePolicyManager {
    pub fn new(config: FailurePolicyConfig) -> Self {
        Self { config }
    }

    pub fn should_retry(&self, error: &ExecutionSharedErrorProxy, attempt: u32) -> bool {
        let retry = match &self.config.retry_policy {
            Some(p) => p,
            None => return false,
        };

        if !retry.enabled {
            return false;
        }
        if attempt >= retry.max_retries {
            return false;
        }
        if is_non_retryable(error, &self.config.non_retryable_errors) {
            return false;
        }

        true
    }

    pub fn next_delay(&self, attempt: u32) -> Duration {
        let retry = match &self.config.retry_policy {
            Some(p) => p,
            None => return Duration::from_secs(1),
        };

        let base = retry.base_delay_ms;
        let multiplier = retry.backoff_multiplier.unwrap_or(2.0);
        let raw_delay = (base as f64) * multiplier.powi(attempt as i32);
        let capped = retry
            .max_delay_ms
            .map(|max| std::cmp::min(raw_delay as u64, max))
            .unwrap_or(raw_delay as u64);

        if retry.jitter.unwrap_or(true) {
            apply_jitter(capped)
        } else {
            Duration::from_millis(capped)
        }
    }

    pub fn fallback_value(&self) -> Option<&Value> {
        self.config
            .fallback_policy
            .as_ref()
            .and_then(|f| f.fallback_value.as_ref())
    }

    pub fn should_continue_after_fallback(&self) -> bool {
        self.config
            .fallback_policy
            .as_ref()
            .map(|f| f.continue_after_fallback)
            .unwrap_or(true)
    }

    pub fn failure_action(
        &self,
        severity: ErrorSeverity,
        attempt: u32,
        error: &ExecutionSharedErrorProxy,
    ) -> FailureAction {
        match severity {
            ErrorSeverity::Critical => FailureAction::Fail,
            ErrorSeverity::Error => {
                if self.should_retry(error, attempt) {
                    FailureAction::Retry
                } else if self.fallback_value().is_some() {
                    FailureAction::Fallback
                } else {
                    FailureAction::Fail
                }
            }
            ErrorSeverity::Warning => {
                if self.should_retry(error, attempt) {
                    FailureAction::Retry
                } else {
                    FailureAction::Continue
                }
            }
            ErrorSeverity::Info => FailureAction::Continue,
        }
    }

    pub fn config(&self) -> &FailurePolicyConfig {
        &self.config
    }
}

fn apply_jitter(delay_ms: u64) -> Duration {
    let jitter_factor = rand::random::<f64>() * 0.2 + 0.9;
    Duration::from_millis((delay_ms as f64 * jitter_factor) as u64)
}

fn is_non_retryable(error: &ExecutionSharedErrorProxy, patterns: &Option<Vec<String>>) -> bool {
    let patterns = match patterns {
        Some(p) if !p.is_empty() => p,
        _ => return false,
    };

    let msg = error.message.to_lowercase();
    patterns.iter().any(|p| msg.contains(&p.to_lowercase()))
}

pub struct ExecutionSharedErrorProxy {
    pub message: String,
    pub severity: Option<ErrorSeverity>,
}

impl ExecutionSharedErrorProxy {
    pub fn from_message(message: String) -> Self {
        let severity = infer_severity(&message);
        Self {
            message,
            severity: Some(severity),
        }
    }
}

fn infer_severity(message: &str) -> ErrorSeverity {
    let lower = message.to_lowercase();
    if lower.contains("critical") {
        ErrorSeverity::Critical
    } else if lower.contains("timeout") || lower.contains("abort") {
        ErrorSeverity::Error
    } else if lower.contains("validation") || lower.contains("invalid") {
        ErrorSeverity::Warning
    } else {
        ErrorSeverity::Info
    }
}

pub fn default_retry_policy() -> RetryPolicy {
    RetryPolicy {
        enabled: true,
        max_retries: 3,
        base_delay_ms: 1000,
        max_delay_ms: Some(30000),
        backoff_multiplier: Some(2.0),
        jitter: Some(true),
    }
}

pub fn default_fallback_policy() -> FallbackPolicy {
    FallbackPolicy {
        fallback_value: None,
        log_fallback: true,
        continue_after_fallback: true,
    }
}

pub fn default_failure_policy_config() -> FailurePolicyConfig {
    FailurePolicyConfig {
        retry_policy: Some(default_retry_policy()),
        fallback_policy: Some(default_fallback_policy()),
        non_retryable_errors: Some(vec!["abort".to_string(), "cancelled".to_string()]),
        log_level: Some("info".to_string()),
        metrics_enabled: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(config: FailurePolicyConfig) -> FailurePolicyManager {
        FailurePolicyManager::new(config)
    }

    #[test]
    fn test_should_retry_within_limit() {
        let m = manager(FailurePolicyConfig {
            retry_policy: Some(RetryPolicy {
                enabled: true,
                max_retries: 3,
                base_delay_ms: 100,
                max_delay_ms: None,
                backoff_multiplier: None,
                jitter: Some(false),
            }),
            fallback_policy: None,
            non_retryable_errors: None,
            log_level: None,
            metrics_enabled: None,
        });

        let err = ExecutionSharedErrorProxy::from_message("transient error".to_string());
        assert!(m.should_retry(&err, 0));
        assert!(m.should_retry(&err, 2));
        assert!(!m.should_retry(&err, 3));
    }

    #[test]
    fn test_should_retry_disabled() {
        let m = manager(FailurePolicyConfig {
            retry_policy: Some(RetryPolicy {
                enabled: false,
                max_retries: 3,
                base_delay_ms: 100,
                max_delay_ms: None,
                backoff_multiplier: None,
                jitter: Some(false),
            }),
            fallback_policy: None,
            non_retryable_errors: None,
            log_level: None,
            metrics_enabled: None,
        });

        let err = ExecutionSharedErrorProxy::from_message("error".to_string());
        assert!(!m.should_retry(&err, 0));
    }

    #[test]
    fn test_non_retryable() {
        let m = manager(FailurePolicyConfig {
            retry_policy: Some(RetryPolicy {
                enabled: true,
                max_retries: 3,
                base_delay_ms: 100,
                max_delay_ms: None,
                backoff_multiplier: None,
                jitter: Some(false),
            }),
            fallback_policy: None,
            non_retryable_errors: Some(vec!["abort".to_string()]),
            log_level: None,
            metrics_enabled: None,
        });

        let err = ExecutionSharedErrorProxy::from_message("operation aborted".to_string());
        assert!(!m.should_retry(&err, 0));
    }

    #[test]
    fn test_next_delay_exponential() {
        let m = manager(FailurePolicyConfig {
            retry_policy: Some(RetryPolicy {
                enabled: true,
                max_retries: 5,
                base_delay_ms: 1000,
                max_delay_ms: Some(30000),
                backoff_multiplier: Some(2.0),
                jitter: Some(false),
            }),
            fallback_policy: None,
            non_retryable_errors: None,
            log_level: None,
            metrics_enabled: None,
        });

        let d0 = m.next_delay(0);
        assert_eq!(d0, Duration::from_millis(1000));

        let d1 = m.next_delay(1);
        assert_eq!(d1, Duration::from_millis(2000));

        let d2 = m.next_delay(2);
        assert_eq!(d2, Duration::from_millis(4000));
    }

    #[test]
    fn test_next_delay_capped() {
        let m = manager(FailurePolicyConfig {
            retry_policy: Some(RetryPolicy {
                enabled: true,
                max_retries: 10,
                base_delay_ms: 1000,
                max_delay_ms: Some(5000),
                backoff_multiplier: Some(2.0),
                jitter: Some(false),
            }),
            fallback_policy: None,
            non_retryable_errors: None,
            log_level: None,
            metrics_enabled: None,
        });

        let d5 = m.next_delay(5);
        assert_eq!(d5, Duration::from_millis(5000));
    }

    #[test]
    fn test_failure_action_critical() {
        let m = manager(default_failure_policy_config());
        let err = ExecutionSharedErrorProxy::from_message("critical".to_string());
        assert_eq!(
            m.failure_action(ErrorSeverity::Critical, 0, &err),
            FailureAction::Fail
        );
    }

    #[test]
    fn test_failure_action_retry_then_fallback() {
        let m = manager(FailurePolicyConfig {
            retry_policy: Some(RetryPolicy {
                enabled: true,
                max_retries: 2,
                base_delay_ms: 100,
                max_delay_ms: None,
                backoff_multiplier: None,
                jitter: Some(false),
            }),
            fallback_policy: Some(FallbackPolicy {
                fallback_value: Some(Value::String("default".to_string())),
                log_fallback: true,
                continue_after_fallback: true,
            }),
            non_retryable_errors: None,
            log_level: None,
            metrics_enabled: None,
        });

        let err = ExecutionSharedErrorProxy::from_message("timeout".to_string());
        assert_eq!(
            m.failure_action(ErrorSeverity::Error, 0, &err),
            FailureAction::Retry
        );
        assert_eq!(
            m.failure_action(ErrorSeverity::Error, 5, &err),
            FailureAction::Fallback
        );
    }

    #[test]
    fn test_infer_severity() {
        assert_eq!(infer_severity("critical failure"), ErrorSeverity::Critical);
        assert_eq!(infer_severity("timeout after 30s"), ErrorSeverity::Error);
        assert_eq!(
            infer_severity("validation error"),
            ErrorSeverity::Warning
        );
        assert_eq!(infer_severity("something went wrong"), ErrorSeverity::Info);
    }

    #[test]
    fn test_jitter_range() {
        let m = manager(FailurePolicyConfig {
            retry_policy: Some(RetryPolicy {
                enabled: true,
                max_retries: 5,
                base_delay_ms: 1000,
                max_delay_ms: None,
                backoff_multiplier: None,
                jitter: Some(true),
            }),
            fallback_policy: None,
            non_retryable_errors: None,
            log_level: None,
            metrics_enabled: None,
        });

        for _ in 0..100 {
            let d = m.next_delay(0);
            let ms = d.as_millis();
            assert!((900..=1100).contains(&ms), "jitter out of range: {}", ms);
        }
    }

    #[test]
    fn test_fallback_value_none() {
        let m = manager(default_failure_policy_config());
        assert!(m.fallback_value().is_none());
    }

    #[test]
    fn test_continue_after_fallback_default() {
        let m = manager(default_failure_policy_config());
        assert!(m.should_continue_after_fallback());
    }
}
