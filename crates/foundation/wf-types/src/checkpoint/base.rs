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
#[serde(rename_all = "camelCase")]
pub struct SnapshotBase {
    pub _version: SnapshotVersion,
    pub _timestamp: super::super::Timestamp,
    pub _entity_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
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
    pub error_records: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interruption_records: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_records: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<super::super::execution::ExecutionHierarchy>,
}

/// Wire-compatible `"FULL"` / `"DELTA"` literal union. Legacy snake_case
/// values written by earlier Rust versions are still accepted when
/// deserializing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum CheckpointType {
    #[serde(alias = "full")]
    Full,
    #[serde(alias = "delta")]
    Delta,
}

/// A `{ from, to }` field change pair used for
/// status / current-node transitions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FullCheckpoint<TSnapshot> {
    pub r#type: CheckpointType,
    pub snapshot: TSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeltaCheckpoint<TDelta> {
    pub r#type: CheckpointType,
    pub base_checkpoint_id: super::super::Id,
    pub previous_checkpoint_id: super::super::Id,
    pub delta: TDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BaseCheckpointCore<TDelta, TSnapshot> {
    pub id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CheckpointType>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "base_checkpoint_id")]
    pub base_checkpoint_id: Option<super::super::Id>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "previous_checkpoint_id"
    )]
    pub previous_checkpoint_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<TDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<TSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<super::super::Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::super::Metadata>,
    /// Checkpoint format version of this blob. Absent for blobs written
    /// before version tracking was introduced (treated as the minimum
    /// compatible version by the VersionManager).
    #[serde(skip_serializing_if = "Option::is_none", alias = "format_version")]
    pub format_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckpointTiming {
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
    OnTimeout,
    OnFailure,
    OnStopped,
    OnComplete,
    Interval,
    Manual,
    Never,
}

/// Compression strategy for checkpoint blobs:
/// `'none' | 'gzip' | 'auto'` union. `Auto` compresses above a size
/// threshold (512 bytes).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompressionStrategy {
    None,
    Gzip,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointContentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_state: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_history: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_statistics: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<CheckpointMetadata>,
    /// Defer post-persist side effects (event publish, file snapshot) to a
    /// background persistence queue; the
    /// checkpoint id is returned before they complete. `wait_for_persistence`
    /// drains the queue.
    #[serde(skip_serializing_if = "Option::is_none", rename = "async")]
    pub asynchronous: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointRetentionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_checkpoints: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<CompressionStrategy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointErrorHandlingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_on_checkpoint_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_on_failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedCheckpointPolicy {
    pub enabled: bool,
    pub triggers: Vec<CheckpointTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<CheckpointContentConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention: Option<CheckpointRetentionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_handling: Option<CheckpointErrorHandlingConfig>,
}

/// Node-level checkpoint timing variants, aligned with the runtime
/// `BeforeNode` / `AfterNode` / `OnNodeError` decision points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeCheckpointTiming {
    Before,
    After,
    OnError,
}

/// Per-node checkpoint configuration. Overrides the workflow-level
/// checkpoint policy where explicitly set; unspecified aspects fall back to
/// the workflow policy (node config wins, workflow policy as default).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeCheckpointConfig {
    /// Master switch for node-level checkpointing. `None` defers to the
    /// workflow policy; `Some(false)` disables node checkpoints entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Explicit timing set (Before / After / OnError). When present it
    /// replaces the workflow policy's node-level timings; when absent each
    /// timing falls back to the workflow policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<NodeCheckpointTiming>>,
    /// Checkpoint description override (surfaced on published events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Throttle node checkpoints to every N nodes (Before/After only).
    /// Default: every node (1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub every_n_nodes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointContext {
    pub entity_type: String,
    pub entity_id: String,
    /// The trigger that caused this checkpoint decision, populated by the
    /// coordinator `prepare` step and used for metadata/tag generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<CheckpointTiming>,
    /// The file-checkpoint actor id (`{kind}:{hierarchy}`) the entity's
    /// file edits are attributed to. Populated by the coordinator `prepare`
    /// step when a file checkpoint manager is attached; `None` keeps the
    /// entity-id-derived actor mapping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
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
