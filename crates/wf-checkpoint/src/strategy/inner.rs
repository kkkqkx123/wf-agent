use wf_types::checkpoint::{
    CheckpointContentConfig, CheckpointContext, CheckpointErrorHandlingConfig,
    CheckpointRetentionConfig, CheckpointTrigger, CompressionStrategy, UnifiedCheckpointPolicy,
};

pub trait CheckpointStrategy: Send + Sync {
    fn should_checkpoint(&self, trigger: &CheckpointTrigger, context: &CheckpointContext) -> bool;

    fn content_config(&self) -> &CheckpointContentConfig;

    fn retention_config(&self) -> Option<&CheckpointRetentionConfig>;

    /// Whether the retention policy is exceeded: more checkpoints than
    /// `max_checkpoints`, or the oldest checkpoint older than `max_age`
    /// (milliseconds). Aligned with TS `CheckpointStrategy.isRetentionExceeded`.
    fn is_retention_exceeded(&self, current_count: u32, oldest_timestamp: i64) -> bool {
        let Some(retention) = self.retention_config() else {
            return false;
        };
        if let Some(max) = retention.max_checkpoints {
            if current_count > max {
                return true;
            }
        }
        if let Some(max_age_ms) = retention.max_age {
            let now = chrono::Utc::now().timestamp_millis();
            if oldest_timestamp > 0 && now - oldest_timestamp > max_age_ms {
                return true;
            }
        }
        false
    }

    /// Effective compression strategy (default `auto`), aligned with TS
    /// `getCompressionStrategy`.
    fn compression_strategy(&self) -> CompressionStrategy {
        self.retention_config()
            .and_then(|r| r.compression)
            .unwrap_or(CompressionStrategy::Auto)
    }

    /// The error handling configuration, aligned with TS
    /// `getErrorHandlingConfig` (defaults: failOnCheckpointError false,
    /// retryOnFailure true, maxRetries 3).
    fn error_handling_config(&self) -> Option<&CheckpointErrorHandlingConfig> {
        None
    }

    fn is_disabled(&self) -> bool;

    /// Auto tags derived from the content config, aligned with TS
    /// `getMetadata` (`has-state`, `has-history`, `has-statistics`).
    fn auto_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        let content = self.content_config();
        if content.include_state.unwrap_or(true) {
            tags.push("has-state".to_string());
        }
        if content.include_history.unwrap_or(true) {
            tags.push("has-history".to_string());
        }
        if content.include_statistics.unwrap_or(false) {
            tags.push("has-statistics".to_string());
        }
        tags
    }
}

#[derive(Debug, Clone)]
pub struct StandardStrategy {
    enabled: bool,
    triggers: Vec<CheckpointTrigger>,
    content: CheckpointContentConfig,
    retention: Option<CheckpointRetentionConfig>,
    error_handling: Option<CheckpointErrorHandlingConfig>,
}

impl StandardStrategy {
    pub fn from_policy(policy: &UnifiedCheckpointPolicy) -> Self {
        Self {
            enabled: policy.enabled,
            triggers: policy.triggers.clone(),
            // TS default: include_state true, include_history true,
            // include_statistics false.
            content: policy.content.clone().unwrap_or(CheckpointContentConfig {
                include_state: Some(true),
                include_history: Some(true),
                include_statistics: Some(false),
                metadata: None,
                asynchronous: None,
            }),
            retention: policy.retention.clone(),
            error_handling: policy.error_handling.clone(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            triggers: vec![],
            content: CheckpointContentConfig {
                include_state: None,
                include_history: None,
                include_statistics: None,
                metadata: None,
                asynchronous: None,
            },
            retention: None,
            error_handling: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn triggers(&self) -> &[CheckpointTrigger] {
        &self.triggers
    }
}

impl CheckpointStrategy for StandardStrategy {
    fn should_checkpoint(&self, trigger: &CheckpointTrigger, _context: &CheckpointContext) -> bool {
        if !self.enabled {
            return false;
        }
        if self.triggers.is_empty() {
            return false;
        }
        // TS semantics: a policy containing NEVER never checkpoints.
        if self.triggers.contains(&CheckpointTrigger::Never) {
            return false;
        }
        self.triggers.contains(trigger)
    }

    fn content_config(&self) -> &CheckpointContentConfig {
        &self.content
    }

    fn retention_config(&self) -> Option<&CheckpointRetentionConfig> {
        self.retention.as_ref()
    }

    fn error_handling_config(&self) -> Option<&CheckpointErrorHandlingConfig> {
        self.error_handling.as_ref()
    }

    fn is_disabled(&self) -> bool {
        !self.enabled
    }
}

pub fn create_checkpoint_strategy(policy: &UnifiedCheckpointPolicy) -> StandardStrategy {
    if policy.enabled {
        StandardStrategy::from_policy(policy)
    } else {
        StandardStrategy::disabled()
    }
}

/// Named policy presets, aligned with the TS `CHECKPOINT_POLICIES`
/// (`MINIMAL` / `STANDARD` / `COMPREHENSIVE` / `NONE`).
pub fn policy_minimal() -> UnifiedCheckpointPolicy {
    UnifiedCheckpointPolicy {
        enabled: true,
        triggers: vec![
            CheckpointTrigger::OnError,
            CheckpointTrigger::OnPause,
            CheckpointTrigger::OnComplete,
        ],
        content: Some(CheckpointContentConfig {
            include_state: Some(true),
            include_history: Some(false),
            include_statistics: Some(false),
            metadata: None,
            asynchronous: None,
        }),
        retention: Some(CheckpointRetentionConfig {
            max_checkpoints: Some(100),
            max_age: Some(86_400_000),
            compression: Some(CompressionStrategy::Auto),
        }),
        error_handling: None,
    }
}

pub fn policy_standard() -> UnifiedCheckpointPolicy {
    UnifiedCheckpointPolicy {
        enabled: true,
        triggers: vec![
            CheckpointTrigger::BeforeExecute,
            CheckpointTrigger::AfterExecute,
            CheckpointTrigger::OnError,
            CheckpointTrigger::BeforeRetry,
            CheckpointTrigger::AfterRetrySuccess,
            CheckpointTrigger::OnFallback,
            CheckpointTrigger::ToolBefore,
            CheckpointTrigger::ToolAfter,
            CheckpointTrigger::OnPause,
            CheckpointTrigger::OnCancel,
            CheckpointTrigger::OnComplete,
            CheckpointTrigger::Manual,
        ],
        content: Some(CheckpointContentConfig {
            include_state: Some(true),
            include_history: Some(true),
            include_statistics: Some(false),
            metadata: None,
            asynchronous: None,
        }),
        retention: Some(CheckpointRetentionConfig {
            max_checkpoints: Some(1000),
            max_age: Some(604_800_000),
            compression: Some(CompressionStrategy::Auto),
        }),
        error_handling: None,
    }
}

pub fn policy_comprehensive() -> UnifiedCheckpointPolicy {
    UnifiedCheckpointPolicy {
        enabled: true,
        triggers: vec![
            CheckpointTrigger::BeforeExecute,
            CheckpointTrigger::AfterExecute,
            CheckpointTrigger::OnError,
            CheckpointTrigger::BeforeRetry,
            CheckpointTrigger::AfterRetrySuccess,
            CheckpointTrigger::OnFallback,
            CheckpointTrigger::IterationEnd,
            CheckpointTrigger::IterationFailed,
            CheckpointTrigger::ToolBefore,
            CheckpointTrigger::ToolAfter,
            CheckpointTrigger::OnPause,
            CheckpointTrigger::OnCancel,
            CheckpointTrigger::OnComplete,
            CheckpointTrigger::Manual,
        ],
        content: Some(CheckpointContentConfig {
            include_state: Some(true),
            include_history: Some(true),
            include_statistics: Some(true),
            metadata: None,
            asynchronous: None,
        }),
        retention: Some(CheckpointRetentionConfig {
            max_checkpoints: Some(10_000),
            max_age: Some(2_592_000_000),
            compression: Some(CompressionStrategy::Gzip),
        }),
        error_handling: None,
    }
}

pub fn policy_none() -> UnifiedCheckpointPolicy {
    UnifiedCheckpointPolicy {
        enabled: false,
        triggers: vec![],
        content: None,
        retention: None,
        error_handling: None,
    }
}

/// Create a strategy from a named preset (`"minimal" | "standard" |
/// "comprehensive" | "none"`, case-insensitive). Returns `None` for unknown
/// names.
pub fn create_checkpoint_strategy_by_name(name: &str) -> Option<StandardStrategy> {
    let policy = match name.to_ascii_lowercase().as_str() {
        "minimal" => policy_minimal(),
        "standard" => policy_standard(),
        "comprehensive" => policy_comprehensive(),
        "none" => policy_none(),
        _ => return None,
    };
    Some(create_checkpoint_strategy(&policy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_policy(triggers: Vec<CheckpointTrigger>) -> UnifiedCheckpointPolicy {
        UnifiedCheckpointPolicy {
            enabled: true,
            triggers,
            content: None,
            retention: None,
            error_handling: None,
        }
    }

    fn make_context() -> CheckpointContext {
        CheckpointContext {
            entity_type: "test".to_string(),
            entity_id: "test-1".to_string(),
            trigger: None,
            attempt: None,
            retry_count: None,
            error: None,
            fallback_used: None,
            metadata: None,
        }
    }

    #[test]
    fn standard_strategy_should_checkpoint() {
        let strategy = create_checkpoint_strategy(&make_policy(vec![
            CheckpointTrigger::BeforeExecute,
            CheckpointTrigger::AfterExecute,
        ]));
        let ctx = make_context();

        assert!(strategy.should_checkpoint(&CheckpointTrigger::BeforeExecute, &ctx));
        assert!(strategy.should_checkpoint(&CheckpointTrigger::AfterExecute, &ctx));
        assert!(!strategy.should_checkpoint(&CheckpointTrigger::OnError, &ctx));
    }

    #[test]
    fn disabled_strategy_never_checkpoints() {
        let strategy = create_checkpoint_strategy(&UnifiedCheckpointPolicy {
            enabled: false,
            triggers: vec![CheckpointTrigger::BeforeExecute],
            content: None,
            retention: None,
            error_handling: None,
        });
        let ctx = make_context();

        assert!(!strategy.should_checkpoint(&CheckpointTrigger::BeforeExecute, &ctx));
    }

    #[test]
    fn empty_triggers_never_checkpoints() {
        let strategy = create_checkpoint_strategy(&make_policy(vec![]));
        let ctx = make_context();

        assert!(!strategy.should_checkpoint(&CheckpointTrigger::BeforeExecute, &ctx));
    }

    #[test]
    fn content_config_defaults() {
        let strategy = create_checkpoint_strategy(&make_policy(vec![CheckpointTrigger::Manual]));
        let config = strategy.content_config();

        assert_eq!(config.include_state, Some(true));
        assert_eq!(config.include_history, Some(true));
        assert_eq!(
            config.include_statistics,
            Some(false),
            "TS default: statistics excluded"
        );
    }

    #[test]
    fn never_trigger_short_circuits_everything() {
        let strategy = create_checkpoint_strategy(&make_policy(vec![CheckpointTrigger::Never]));
        let ctx = make_context();
        assert!(!strategy.should_checkpoint(&CheckpointTrigger::Never, &ctx));
        assert!(!strategy.should_checkpoint(&CheckpointTrigger::OnError, &ctx));
        assert!(!strategy.should_checkpoint(&CheckpointTrigger::Manual, &ctx));
    }

    #[test]
    fn is_retention_exceeded_checks_count() {
        let strategy = create_checkpoint_strategy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTrigger::Manual],
            content: None,
            retention: Some(CheckpointRetentionConfig {
                max_checkpoints: Some(5),
                max_age: None,
                compression: None,
            }),
            error_handling: None,
        });
        assert!(!strategy.is_retention_exceeded(5, 0));
        assert!(strategy.is_retention_exceeded(6, 0));
    }

    #[test]
    fn is_retention_exceeded_checks_age() {
        let strategy = create_checkpoint_strategy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTrigger::Manual],
            content: None,
            retention: Some(CheckpointRetentionConfig {
                max_checkpoints: None,
                max_age: Some(1000),
                compression: None,
            }),
            error_handling: None,
        });
        let now = chrono::Utc::now().timestamp_millis();
        assert!(!strategy.is_retention_exceeded(0, now - 500));
        assert!(strategy.is_retention_exceeded(0, now - 2000));
    }

    #[test]
    fn auto_tags_reflect_content_config() {
        let strategy = create_checkpoint_strategy(&make_policy(vec![CheckpointTrigger::Manual]));
        let tags = strategy.auto_tags();
        assert!(tags.contains(&"has-state".to_string()));
        assert!(tags.contains(&"has-history".to_string()));
        assert!(!tags.contains(&"has-statistics".to_string()));
    }

    #[test]
    fn named_presets() {
        assert!(create_checkpoint_strategy_by_name("minimal").is_some());
        assert!(create_checkpoint_strategy_by_name("standard").is_some());
        assert!(create_checkpoint_strategy_by_name("COMPREHENSIVE").is_some());
        assert!(create_checkpoint_strategy_by_name("none").is_some());
        assert!(create_checkpoint_strategy_by_name("unknown").is_none());

        let none = create_checkpoint_strategy_by_name("none").unwrap();
        assert!(none.is_disabled());
        let standard = create_checkpoint_strategy_by_name("standard").unwrap();
        assert_eq!(
            standard.retention_config().unwrap().max_checkpoints,
            Some(1000)
        );
    }

    #[test]
    fn compression_strategy_defaults_to_auto() {
        let strategy = create_checkpoint_strategy(&make_policy(vec![CheckpointTrigger::Manual]));
        assert_eq!(strategy.compression_strategy(), CompressionStrategy::Auto);
    }
}
