use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileCheckpointStorageType {
    Sqlite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileCheckpointStorageConfig {
    #[serde(rename = "type")]
    pub storage_type: FileCheckpointStorageType,
    pub db_path: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FailureBehavior {
    #[default]
    Warn,
    Error,
    Ignore,
}

/// Layered approval policy applied when an agent execution ends.
/// The policy decides how the agent partition flows through the approval
/// layer before being merged into a feature.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// Skip the approval layer entirely: agent ends and is merged straight
    /// into the feature (current default, behavior unchanged).
    #[default]
    None,
    /// Move agent changes to approval and immediately merge into the feature.
    Auto,
    /// Approval is granted by an in-workflow approval tool (`approve_changes`).
    Llm,
    /// Approval is suspended and resolved by the host (`list_pending_approvals`
    /// / `approve` / `reject`), possibly across executions.
    Manual,
}

/// Three-way merge conflict behavior when agent changes are merged.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictBehavior {
    /// Write conflict markers into the affected file and report via the
    /// merge result / event; execution continues (default).
    #[default]
    Marker,
    /// Abort the merge with an error as soon as a conflict is detected.
    Fail,
    /// Route conflicted changes to the approval layer for resolution.
    Approval,
}

/// GC retention policy for file-checkpoint physical garbage collection.
/// Mirrors `layertwine::git_sync::gc::GcRetention` at the config layer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FileCheckpointGcRetention {
    /// Keep the N most recently created partition head checkpoints
    /// protected even when no branch points at them. `0` = only the
    /// built-in protected set (branch heads + ancestors + git anchors).
    #[serde(default)]
    pub keep_recent_heads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileCheckpointConfig {
    #[serde(default)]
    pub enabled: bool,
    pub workspace_root: Option<String>,
    #[serde(default = "default_max_delta_chain")]
    pub max_delta_chain_length: u32,
    pub custom_ignore_patterns: Option<Vec<String>>,
    pub storage: Option<FileCheckpointStorageConfig>,
    #[serde(default)]
    pub failure_behavior: FailureBehavior,
    /// Layered approval policy (auto / llm / manual / none).
    /// `none` keeps today's behavior.
    #[serde(default)]
    pub approval_policy: ApprovalPolicy,
    /// Three-way merge conflict strategy (marker / fail / approval).
    /// `marker` is the default.
    #[serde(default)]
    pub conflict_behavior: ConflictBehavior,
    /// Whether the manual watcher is enabled (`true` when both
    /// `FileCheckpointConfig.enabled` and `workspace_root` are set).
    #[serde(default)]
    pub manual_watch: bool,
    /// Physical GC auto-run interval in seconds; `None` = never run
    /// automatically (explicit `run_gc` / API only).
    #[serde(default)]
    pub gc_interval_secs: Option<u64>,
    /// GC retention policy; `None` = default (only the built-in protected
    /// set).
    #[serde(default)]
    pub gc_retention: Option<FileCheckpointGcRetention>,
}

fn default_max_delta_chain() -> u32 {
    20
}

impl Default for FileCheckpointConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            workspace_root: None,
            max_delta_chain_length: default_max_delta_chain(),
            custom_ignore_patterns: None,
            storage: None,
            failure_behavior: FailureBehavior::default(),
            approval_policy: ApprovalPolicy::default(),
            conflict_behavior: ConflictBehavior::default(),
            manual_watch: false,
            gc_interval_secs: None,
            gc_retention: None,
        }
    }
}
