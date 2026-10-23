use serde::{Deserialize, Serialize};

use super::file_scope::ExecutionFileScope;

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
    /// No-op: the actor edit line keeps its history and stays queryable.
    /// Nothing is submitted for review and nothing is merged.
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
/// Mirrors `checkpoint_file::gc::GcRetention` at the config layer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FileCheckpointGcRetention {
    /// Keep the N most recently created checkpoints
    /// protected even when no branch points at them. `0` = only the
    /// built-in protected set (branch heads + ancestors).
    #[serde(default)]
    pub keep_recent_heads: usize,
}

/// Scope-window serialization for shell sampling.
///
/// Shell tools capture before/after file hashes around the command run. Two
/// overlapping sampling windows interleave on the shared filesystem and
/// misattribute each other's writes, so shell windows hold a process-wide
/// exclusive permit across the whole capture-execute-commit span. `None`
/// (missing section) keeps the default: enabled with the built-in timeout.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeSerializationConfig {
    /// Master switch. `false` disables the exclusive permit and restores
    /// fully parallel sampling (with the interleaving risk).
    #[serde(default = "default_scope_serialization_enabled")]
    pub enabled: bool,
    /// Maximum time to wait for the exclusive permit in milliseconds.
    /// `None` uses the built-in default; expiry proceeds without the
    /// permit and marks sampling degraded instead of blocking the batch.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

fn default_scope_serialization_enabled() -> bool {
    true
}

impl Default for ScopeSerializationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            lock_timeout_ms: None,
        }
    }
}

impl ScopeSerializationConfig {
    /// Built-in wait budget used when `lock_timeout_ms` is unset.
    pub const DEFAULT_LOCK_TIMEOUT_MS: u64 = 300_000;

    /// Resolve the effective (enabled, timeout) pair for a possibly
    /// missing section: missing means enabled with the default timeout.
    pub fn resolve(config: Option<&ScopeSerializationConfig>) -> (bool, u64) {
        match config {
            None => (true, Self::DEFAULT_LOCK_TIMEOUT_MS),
            Some(section) => (
                section.enabled,
                section
                    .lock_timeout_ms
                    .unwrap_or(Self::DEFAULT_LOCK_TIMEOUT_MS),
            ),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FileCheckpointConfig {
    #[serde(default)]
    pub enabled: bool,
    pub workspace_root: Option<String>,
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
    /// Manual watcher poll interval in milliseconds; `None` uses the
    /// built-in default. Larger values cut background full-scan cost on
    /// large workspaces.
    #[serde(default)]
    pub manual_poll_ms: Option<u64>,
    /// Physical GC auto-run interval in seconds; `None` = never run
    /// automatically (explicit `run_gc` / API only).
    #[serde(default)]
    pub gc_interval_secs: Option<u64>,
    /// GC retention policy; `None` = default (only the built-in protected
    /// set).
    #[serde(default)]
    pub gc_retention: Option<FileCheckpointGcRetention>,
    /// Default execution scope applied when an execution carries no explicit
    /// scope. Missing means deny everything at the scope core; the checkpoint
    /// manager materializes an explicit workspace range when a workspace root
    /// is configured. Every field inside follows the same missing-inherits
    /// versus explicit-empty-denies convention.
    #[serde(default)]
    pub default_scope: Option<ExecutionFileScope>,
    /// Keep the checkpoint object store inside the workspace
    /// (`<workspace>/.wf-checkpoint-git`) instead of the global data
    /// directory. Default (`false` / missing) stores outside the workspace.
    #[serde(default)]
    pub store_in_workspace: Option<bool>,
    /// Per-file content cap in bytes. Files above it stay on disk untouched:
    /// never captured, never restored, never deleted. `None` uses the
    /// built-in 10 MiB default; `0` disables the cap.
    #[serde(default)]
    pub max_file_size: Option<u64>,
    /// Shell sampling-window serialization. `None` keeps the default
    /// (enabled with the built-in wait budget).
    #[serde(default)]
    pub scope_serialization: Option<ScopeSerializationConfig>,
}
