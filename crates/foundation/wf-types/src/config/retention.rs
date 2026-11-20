use serde::{Deserialize, Serialize};

/// Server-owned retention policy for execution history, tasks and checkpoints.
///
/// All durations are expressed with explicit units. The defaults are
/// intentionally conservative: automatic deletion stays off and the first
/// sweep only reports its candidates until an operator enables it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct RetentionConfig {
    pub enabled: bool,
    pub scan_interval_secs: u64,
    pub batch_size: usize,
    pub dry_run: bool,
    pub checkpoints: CheckpointRetentionSettings,
    pub file_checkpoints: FileCheckpointRetentionSettings,
    pub executions: ExecutionRetentionSettings,
    pub tasks: TaskRetentionSettings,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scan_interval_secs: 3600,
            batch_size: 200,
            dry_run: true,
            checkpoints: CheckpointRetentionSettings::default(),
            file_checkpoints: FileCheckpointRetentionSettings::default(),
            executions: ExecutionRetentionSettings::default(),
            tasks: TaskRetentionSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct CheckpointRetentionSettings {
    pub max_age_days: u64,
    pub max_checkpoints: u32,
    pub protect_favorites: bool,
}

impl Default for CheckpointRetentionSettings {
    fn default() -> Self {
        Self {
            max_age_days: 30,
            max_checkpoints: 50,
            protect_favorites: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct FileCheckpointRetentionSettings {
    pub history_max_age_days: u64,
    pub keep_recent_heads: usize,
}

impl Default for FileCheckpointRetentionSettings {
    fn default() -> Self {
        Self {
            history_max_age_days: 30,
            keep_recent_heads: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct ExecutionRetentionSettings {
    pub history_retention_days: u64,
    pub keep_recent: usize,
    pub protect_running: bool,
}

impl Default for ExecutionRetentionSettings {
    fn default() -> Self {
        Self {
            history_retention_days: 180,
            keep_recent: 20,
            protect_running: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct TaskRetentionSettings {
    pub retention_days: u64,
    pub trigger_retention_days: u64,
}

impl Default for TaskRetentionSettings {
    fn default() -> Self {
        Self {
            retention_days: 30,
            trigger_retention_days: 30,
        }
    }
}

impl RetentionConfig {
    pub fn max_age_ms(days: u64) -> i64 {
        days.saturating_mul(86_400_000).min(i64::MAX as u64) as i64
    }
}
