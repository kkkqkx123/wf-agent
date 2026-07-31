use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::error::{ToolError, ToolResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFailureProtectionConfig {
    pub max_consecutive_failures: u32,
    pub cooldown_period: Duration,
    pub enabled: bool,
}

impl Default for ToolFailureProtectionConfig {
    fn default() -> Self {
        Self {
            max_consecutive_failures: 3,
            cooldown_period: Duration::from_secs(60),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFailureInfo {
    pub failure_count: u32,
    pub last_failure_timestamp: i64,
    pub last_error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionCheckResult {
    pub allowed: bool,
    pub failure_count: u32,
    pub reason: Option<String>,
    pub remaining_cooldown_ms: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFailureProtectionSnapshot {
    pub failure_map: HashMap<String, ToolFailureInfo>,
    pub config: ToolFailureProtectionConfig,
}

pub struct ToolFailureProtectionState {
    failures: DashMap<String, ToolFailureInfo>,
    config: RwLock<ToolFailureProtectionConfig>,
}

impl ToolFailureProtectionState {
    pub fn new(config: ToolFailureProtectionConfig) -> Self {
        Self {
            failures: DashMap::new(),
            config: RwLock::new(config),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(ToolFailureProtectionConfig::default())
    }

    pub fn can_execute(&self, tool_id: &str) -> ToolExecutionCheckResult {
        let config = self.config.read().unwrap();

        if !config.enabled {
            return ToolExecutionCheckResult {
                allowed: true,
                failure_count: 0,
                reason: None,
                remaining_cooldown_ms: 0,
                last_error: None,
            };
        }

        let Some(info) = self.failures.get(tool_id) else {
            return ToolExecutionCheckResult {
                allowed: true,
                failure_count: 0,
                reason: None,
                remaining_cooldown_ms: 0,
                last_error: None,
            };
        };

        if info.failure_count < config.max_consecutive_failures {
            return ToolExecutionCheckResult {
                allowed: true,
                failure_count: info.failure_count,
                reason: None,
                remaining_cooldown_ms: 0,
                last_error: Some(info.last_error.clone()),
            };
        }

        let now = wf_common::time::now();
        let elapsed = now - info.last_failure_timestamp;
        let cooldown_ms = config.cooldown_period.as_millis() as i64;

        if elapsed < cooldown_ms {
            let remaining = (cooldown_ms - elapsed) as u64;
            ToolExecutionCheckResult {
                allowed: false,
                failure_count: info.failure_count,
                reason: Some(format!(
                    "Tool '{}' is blocked due to {} consecutive failures",
                    tool_id, info.failure_count
                )),
                remaining_cooldown_ms: remaining,
                last_error: Some(info.last_error.clone()),
            }
        } else {
            ToolExecutionCheckResult {
                allowed: true,
                failure_count: info.failure_count,
                reason: None,
                remaining_cooldown_ms: 0,
                last_error: Some(info.last_error.clone()),
            }
        }
    }

    pub fn record_failure(&self, tool_id: &str, error_message: String) {
        let now = wf_common::time::now();
        let mut entry =
            self.failures
                .entry(tool_id.to_string())
                .or_insert_with(|| ToolFailureInfo {
                    failure_count: 0,
                    last_failure_timestamp: now,
                    last_error: String::new(),
                });
        entry.failure_count += 1;
        entry.last_failure_timestamp = now;
        entry.last_error = error_message;
    }

    pub fn record_success(&self, tool_id: &str) {
        self.failures.remove(tool_id);
    }

    pub fn get_failure_count(&self, tool_id: &str) -> u32 {
        self.failures
            .get(tool_id)
            .map(|info| info.failure_count)
            .unwrap_or(0)
    }

    pub fn reset(&self, tool_id: &str) {
        self.failures.remove(tool_id);
    }

    pub fn reset_tool(&self, tool_id: &str) {
        self.failures.remove(tool_id);
    }

    pub fn reset_all(&self) {
        self.failures.clear();
    }

    pub fn is_blocked(&self, tool_id: &str) -> bool {
        !self.can_execute(tool_id).allowed
    }

    pub fn check_and_fail(&self, tool_id: &str) -> ToolResult<()> {
        let result = self.can_execute(tool_id);
        if !result.allowed {
            return Err(ToolError::ExecutionFailed {
                tool_id: tool_id.to_string(),
                reason: result.reason.unwrap_or_else(|| {
                    format!(
                        "Tool blocked after {} consecutive failures",
                        result.failure_count
                    )
                }),
            });
        }
        Ok(())
    }

    pub fn config(&self) -> ToolFailureProtectionConfig {
        self.config.read().unwrap().clone()
    }

    pub fn update_config(&self, config: ToolFailureProtectionConfig) {
        *self.config.write().unwrap() = config;
    }

    pub fn size(&self) -> usize {
        self.failures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.failures.is_empty()
    }

    pub async fn cleanup(&self) {
        self.failures.clear();
    }

    pub async fn create_snapshot(&self) -> ToolFailureProtectionSnapshot {
        let failure_map: HashMap<String, ToolFailureInfo> = self
            .failures
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();
        let config = self.config.read().unwrap().clone();
        ToolFailureProtectionSnapshot {
            failure_map,
            config,
        }
    }

    pub async fn restore_from_snapshot(&self, snapshot: ToolFailureProtectionSnapshot) {
        self.failures.clear();
        for (key, info) in snapshot.failure_map {
            self.failures.insert(key, info);
        }
        *self.config.write().unwrap() = snapshot.config;
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

    fn default_state() -> ToolFailureProtectionState {
        ToolFailureProtectionState::new(ToolFailureProtectionConfig::default())
    }

    #[test]
    fn test_can_execute_with_no_failures() {
        let protection = ToolFailureProtectionState::with_defaults();
        assert!(protection.can_execute("tool1").allowed);
    }

    #[test]
    fn test_blocks_after_max_failures() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 3,
            cooldown_period: Duration::from_secs(60),
            enabled: true,
        };
        let protection = ToolFailureProtectionState::new(config);

        protection.record_failure("tool1", "err".to_string());
        assert!(protection.can_execute("tool1").allowed);

        protection.record_failure("tool1", "err".to_string());
        assert!(protection.can_execute("tool1").allowed);

        protection.record_failure("tool1", "err".to_string());
        assert!(!protection.can_execute("tool1").allowed);
        assert!(protection.is_blocked("tool1"));
    }

    #[test]
    fn test_success_resets_failures() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 2,
            cooldown_period: Duration::from_secs(60),
            enabled: true,
        };
        let protection = ToolFailureProtectionState::new(config);

        protection.record_failure("tool1", "err".to_string());
        protection.record_failure("tool1", "err".to_string());
        assert!(protection.is_blocked("tool1"));

        protection.record_success("tool1");
        assert!(protection.can_execute("tool1").allowed);
    }

    #[test]
    fn test_cooldown_expires() {
        let state = ToolFailureProtectionState::new(ToolFailureProtectionConfig {
            max_consecutive_failures: 1,
            cooldown_period: Duration::from_millis(10),
            enabled: true,
        });

        state.record_failure("tool1", "error".to_string());
        assert!(!state.can_execute("tool1").allowed);

        std::thread::sleep(Duration::from_millis(20));
        assert!(state.can_execute("tool1").allowed);
    }

    #[test]
    fn test_check_and_fails() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 1,
            cooldown_period: Duration::from_secs(60),
            enabled: true,
        };
        let protection = ToolFailureProtectionState::new(config);

        assert!(protection.check_and_fail("tool1").is_ok());

        protection.record_failure("tool1", "err".to_string());
        let result = protection.check_and_fail("tool1");
        assert!(result.is_err());
    }

    #[test]
    fn test_reset() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 1,
            cooldown_period: Duration::from_secs(60),
            enabled: true,
        };
        let protection = ToolFailureProtectionState::new(config);

        protection.record_failure("tool1", "err".to_string());
        protection.record_failure("tool2", "err".to_string());
        assert!(protection.is_blocked("tool1"));
        assert!(protection.is_blocked("tool2"));

        protection.reset("tool1");
        assert!(protection.can_execute("tool1").allowed);
        assert!(protection.is_blocked("tool2"));

        protection.reset_all();
        assert!(protection.can_execute("tool2").allowed);
    }

    #[test]
    fn test_get_failure_count() {
        let config = ToolFailureProtectionConfig {
            max_consecutive_failures: 5,
            cooldown_period: Duration::from_secs(60),
            enabled: true,
        };
        let protection = ToolFailureProtectionState::new(config);

        assert_eq!(protection.get_failure_count("tool1"), 0);

        protection.record_failure("tool1", "err".to_string());
        protection.record_failure("tool1", "err".to_string());
        assert_eq!(protection.get_failure_count("tool1"), 2);
    }

    #[test]
    fn test_disabled_always_allows() {
        let state = ToolFailureProtectionState::new(ToolFailureProtectionConfig {
            enabled: false,
            ..Default::default()
        });

        for _ in 0..10 {
            state.record_failure("tool_a", "error".to_string());
        }

        let result = state.can_execute("tool_a");
        assert!(result.allowed);
    }

    #[test]
    fn test_snapshot_roundtrip() {
        let state = default_state();
        state.record_failure("tool_a", "err_a".to_string());
        state.record_failure("tool_b", "err_b".to_string());

        let snapshot = futures::executor::block_on(state.create_snapshot());
        assert_eq!(snapshot.failure_map.len(), 2);

        let new_state = ToolFailureProtectionState::new(ToolFailureProtectionConfig::default());
        futures::executor::block_on(new_state.restore_from_snapshot(snapshot));

        assert_eq!(new_state.get_failure_count("tool_a"), 1);
        assert_eq!(new_state.get_failure_count("tool_b"), 1);
    }

    #[test]
    fn test_size_and_is_empty() {
        let state = default_state();
        assert!(state.is_empty());
        assert_eq!(state.size(), 0);

        state.record_failure("tool_a", "e".to_string());
        assert!(!state.is_empty());
        assert_eq!(state.size(), 1);
    }
}
