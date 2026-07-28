use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

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
    inner: Mutex<ProtectionInner>,
}

struct ProtectionInner {
    failure_map: HashMap<String, ToolFailureInfo>,
    config: ToolFailureProtectionConfig,
}

impl ToolFailureProtectionState {
    pub fn new(config: ToolFailureProtectionConfig) -> Self {
        Self {
            inner: Mutex::new(ProtectionInner {
                failure_map: HashMap::new(),
                config,
            }),
        }
    }

    pub fn can_execute(&self, tool_name: &str) -> ToolExecutionCheckResult {
        let inner = self.inner.lock().unwrap();

        if !inner.config.enabled {
            return ToolExecutionCheckResult {
                allowed: true,
                failure_count: 0,
                reason: None,
                remaining_cooldown_ms: 0,
                last_error: None,
            };
        }

        let Some(info) = inner.failure_map.get(tool_name) else {
            return ToolExecutionCheckResult {
                allowed: true,
                failure_count: 0,
                reason: None,
                remaining_cooldown_ms: 0,
                last_error: None,
            };
        };

        if info.failure_count < inner.config.max_consecutive_failures {
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
        let cooldown_ms = inner.config.cooldown_period.as_millis() as i64;

        if elapsed < cooldown_ms {
            let remaining = (cooldown_ms - elapsed) as u64;
            ToolExecutionCheckResult {
                allowed: false,
                failure_count: info.failure_count,
                reason: Some(format!(
                    "Tool '{}' is blocked due to {} consecutive failures",
                    tool_name, info.failure_count
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

    pub fn record_success(&self, tool_name: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.failure_map.remove(tool_name);
    }

    pub fn record_failure(&self, tool_name: &str, error_message: String) {
        let mut inner = self.inner.lock().unwrap();
        let now = wf_common::time::now();

        let entry = inner.failure_map.entry(tool_name.to_string()).or_insert(
            ToolFailureInfo {
                failure_count: 0,
                last_failure_timestamp: now,
                last_error: String::new(),
            },
        );

        entry.failure_count += 1;
        entry.last_failure_timestamp = now;
        entry.last_error = error_message;
    }

    pub fn reset_tool(&self, tool_name: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.failure_map.remove(tool_name);
    }

    pub fn get_failure_count(&self, tool_name: &str) -> u32 {
        let inner = self.inner.lock().unwrap();
        inner
            .failure_map
            .get(tool_name)
            .map(|i| i.failure_count)
            .unwrap_or(0)
    }

    pub fn config(&self) -> ToolFailureProtectionConfig {
        let inner = self.inner.lock().unwrap();
        inner.config.clone()
    }

    pub fn update_config(&self, config: ToolFailureProtectionConfig) {
        let mut inner = self.inner.lock().unwrap();
        inner.config = config;
    }
}

impl ToolFailureProtectionState {
    pub fn size(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.failure_map.len()
    }

    pub fn is_empty(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.failure_map.is_empty()
    }

    pub async fn cleanup(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.failure_map.clear();
    }

    pub async fn create_snapshot(&self) -> ToolFailureProtectionSnapshot {
        let inner = self.inner.lock().unwrap();
        ToolFailureProtectionSnapshot {
            failure_map: inner.failure_map.clone(),
            config: inner.config.clone(),
        }
    }

    pub async fn restore_from_snapshot(&self, snapshot: ToolFailureProtectionSnapshot) {
        let mut inner = self.inner.lock().unwrap();
        inner.failure_map = snapshot.failure_map;
        inner.config = snapshot.config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_state() -> ToolFailureProtectionState {
        ToolFailureProtectionState::new(ToolFailureProtectionConfig::default())
    }

    #[test]
    fn test_can_execute_no_history() {
        let state = default_state();
        let result = state.can_execute("my_tool");
        assert!(result.allowed);
        assert_eq!(result.failure_count, 0);
    }

    #[test]
    fn test_record_failure_and_block() {
        let state = default_state();

        for i in 0..3 {
            state.record_failure("tool_a", format!("error {}", i));
        }

        let result = state.can_execute("tool_a");
        assert!(!result.allowed);
        assert_eq!(result.failure_count, 3);
        assert!(result.remaining_cooldown_ms > 0);
    }

    #[test]
    fn test_record_success_resets() {
        let state = default_state();

        state.record_failure("tool_a", "error".to_string());
        state.record_failure("tool_a", "error".to_string());
        state.record_success("tool_a");

        let result = state.can_execute("tool_a");
        assert!(result.allowed);
        assert_eq!(result.failure_count, 0);
    }

    #[test]
    fn test_reset_tool() {
        let state = default_state();

        for _ in 0..5 {
            state.record_failure("tool_a", "error".to_string());
        }

        state.reset_tool("tool_a");
        let result = state.can_execute("tool_a");
        assert!(result.allowed);
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
    fn test_get_failure_count() {
        let state = default_state();
        assert_eq!(state.get_failure_count("tool_a"), 0);

        state.record_failure("tool_a", "e1".to_string());
        assert_eq!(state.get_failure_count("tool_a"), 1);

        state.record_failure("tool_a", "e2".to_string());
        assert_eq!(state.get_failure_count("tool_a"), 2);
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
    fn test_cooldown_expires() {
        let state = ToolFailureProtectionState::new(ToolFailureProtectionConfig {
            max_consecutive_failures: 1,
            cooldown_period: Duration::from_millis(10),
            enabled: true,
        });

        state.record_failure("tool_a", "error".to_string());
        assert!(!state.can_execute("tool_a").allowed);

        std::thread::sleep(Duration::from_millis(20));
        assert!(state.can_execute("tool_a").allowed);
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
