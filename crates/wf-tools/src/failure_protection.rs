use dashmap::DashMap;
use std::time::{Duration, Instant};

use crate::error::{ToolError, ToolResult};

pub struct ToolFailureProtectionConfig {
    pub max_consecutive_failures: u32,
    pub cooldown_period: Duration,
}

impl Default for ToolFailureProtectionConfig {
    fn default() -> Self {
        Self {
            max_consecutive_failures: 3,
            cooldown_period: Duration::from_secs(60),
        }
    }
}

pub struct ToolFailureProtectionState {
    failures: DashMap<String, Vec<Instant>>,
    config: ToolFailureProtectionConfig,
}

impl ToolFailureProtectionState {
    pub fn new(config: ToolFailureProtectionConfig) -> Self {
        Self {
            failures: DashMap::new(),
            config,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(ToolFailureProtectionConfig::default())
    }

    pub fn can_execute(&self, tool_id: &str) -> bool {
        self.failures
            .get(tool_id)
            .map(|timestamps| {
                let recent_count = timestamps
                    .iter()
                    .filter(|t| t.elapsed() < self.config.cooldown_period)
                    .count();
                recent_count < self.config.max_consecutive_failures as usize
            })
            .unwrap_or(true)
    }

    pub fn record_failure(&self, tool_id: &str) {
        let now = Instant::now();
        self.failures
            .entry(tool_id.to_string())
            .or_default()
            .push(now);
    }

    pub fn record_success(&self, tool_id: &str) {
        self.failures.remove(tool_id);
    }

    pub fn get_failure_count(&self, tool_id: &str) -> usize {
        self.failures
            .get(tool_id)
            .map(|timestamps| {
                timestamps
                    .iter()
                    .filter(|t| t.elapsed() < self.config.cooldown_period)
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn reset(&self, tool_id: &str) {
        self.failures.remove(tool_id);
    }

    pub fn reset_all(&self) {
        self.failures.clear();
    }

    pub fn is_blocked(&self, tool_id: &str) -> bool {
        !self.can_execute(tool_id)
    }

    pub fn check_and_fail(&self, tool_id: &str) -> ToolResult<()> {
        if !self.can_execute(tool_id) {
            return Err(ToolError::ExecutionFailed {
                tool_id: tool_id.to_string(),
                reason: format!(
                    "Tool blocked after {} consecutive failures within {:?}",
                    self.config.max_consecutive_failures, self.config.cooldown_period
                ),
            });
        }
        Ok(())
    }
}

impl Default for ToolFailureProtectionState {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_can_execute_with_no_failures() {
        let protection = ToolFailureProtectionState::with_defaults();
        assert!(protection.can_execute("tool1"));
    }

    #[test]
    fn test_blocks_after_max_failures() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 3,
            cooldown_period: Duration::from_secs(60),
        };
        let protection = ToolFailureProtectionState::new(config);

        protection.record_failure("tool1");
        assert!(protection.can_execute("tool1"));

        protection.record_failure("tool1");
        assert!(protection.can_execute("tool1"));

        protection.record_failure("tool1");
        assert!(!protection.can_execute("tool1"));
        assert!(protection.is_blocked("tool1"));
    }

    #[test]
    fn test_success_resets_failures() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 2,
            cooldown_period: Duration::from_secs(60),
        };
        let protection = ToolFailureProtectionState::new(config);

        protection.record_failure("tool1");
        protection.record_failure("tool1");
        assert!(protection.is_blocked("tool1"));

        protection.record_success("tool1");
        assert!(protection.can_execute("tool1"));
    }

    #[test]
    fn test_cooldown_expires() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 1,
            cooldown_period: Duration::from_millis(50),
        };
        let protection = ToolFailureProtectionState::new(config);

        protection.record_failure("tool1");
        assert!(protection.is_blocked("tool1"));

        thread::sleep(Duration::from_millis(100));
        assert!(protection.can_execute("tool1"));
    }

    #[test]
    fn test_check_and_fails() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 1,
            cooldown_period: Duration::from_secs(60),
        };
        let protection = ToolFailureProtectionState::new(config);

        assert!(protection.check_and_fail("tool1").is_ok());

        protection.record_failure("tool1");
        let result = protection.check_and_fail("tool1");
        assert!(result.is_err());
    }

    #[test]
    fn test_reset() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 1,
            cooldown_period: Duration::from_secs(60),
        };
        let protection = ToolFailureProtectionState::new(config);

        protection.record_failure("tool1");
        protection.record_failure("tool2");
        assert!(protection.is_blocked("tool1"));
        assert!(protection.is_blocked("tool2"));

        protection.reset("tool1");
        assert!(protection.can_execute("tool1"));
        assert!(protection.is_blocked("tool2"));

        protection.reset_all();
        assert!(protection.can_execute("tool2"));
    }

    #[test]
    fn test_get_failure_count() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 5,
            cooldown_period: Duration::from_secs(60),
        };
        let protection = ToolFailureProtectionState::new(config);

        assert_eq!(protection.get_failure_count("tool1"), 0);

        protection.record_failure("tool1");
        protection.record_failure("tool1");
        assert_eq!(protection.get_failure_count("tool1"), 2);
    }
}
