use serde::{Deserialize, Serialize};

pub type SnapshotVersion = u32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    Active,
    Completed,
    Expired,
    Corrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotBase {
    pub _version: SnapshotVersion,
    pub _timestamp: super::super::Timestamp,
    pub _entity_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointStateBase {
    pub id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<super::super::Id>,
    pub timestamp: super::super::Timestamp,
    pub format_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<super::super::Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<super::super::Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<super::super::execution::ExecutionHierarchy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointType {
    Full,
    Delta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FullCheckpoint<TSnapshot> {
    pub r#type: CheckpointType,
    pub snapshot: TSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeltaCheckpoint<TDelta> {
    pub r#type: CheckpointType,
    pub base_checkpoint_id: super::super::Id,
    pub previous_checkpoint_id: super::super::Id,
    pub delta: TDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaseCheckpointCore<TDelta, TSnapshot> {
    pub id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CheckpointType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_checkpoint_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_checkpoint_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<TDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<TSnapshot>,
    pub timestamp: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::super::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeltaStorageConfig {
    pub enabled: bool,
    pub baseline_interval: u32,
    pub max_delta_chain_length: u32,
}

impl Default for DeltaStorageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            baseline_interval: 10,
            max_delta_chain_length: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckpointTrigger {
    BeforeExecute,
    AfterExecute,
    OnError,
    BeforeRetry,
    AfterRetrySuccess,
    OnFallback,
    IterationEnd,
    IterationFailed,
    ToolBefore,
    ToolAfter,
    OnPause,
    OnCancel,
    OnComplete,
    Interval,
    Manual,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointContentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_state: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_history: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_statistics: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<CheckpointMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointRetentionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_checkpoints: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointErrorHandlingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_on_checkpoint_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_on_failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedCheckpointPolicy {
    pub enabled: bool,
    pub triggers: Vec<CheckpointTrigger>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<CheckpointContentConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention: Option<CheckpointRetentionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_handling: Option<CheckpointErrorHandlingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointContext {
    pub entity_type: String,
    pub entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_used: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}
