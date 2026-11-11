//! Checkpoint and approval subcommand arguments.

use clap::Subcommand;

/// Explicit execution domain override for ambiguous execution ids.
///
/// Only needed when one id exists as both an agent loop and a workflow
/// execution; otherwise the domain is resolved automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CheckpointDomain {
    /// Workflow execution.
    Workflow,
    /// Agent loop.
    Agent,
}

/// Checkpoint subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum CheckpointSub {
    /// Create a workflow checkpoint for an execution.
    Create {
        /// Workflow execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Create an agent-loop checkpoint.
    CreateAgent {
        /// Agent loop id.
        #[arg(value_name = "ID")]
        id: String,
        /// Checkpoint name.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    /// Create a file checkpoint for the bound workspace.
    FileCreate {
        /// Actor or entity id the file edits are attributed to.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// List file-checkpoint partitions of the bound workspace.
    FilePartitions,
    /// Show the commit history of one file-checkpoint partition, newest first.
    FilePartitionHistory {
        /// Partition ref name.
        #[arg(value_name = "PARTITION")]
        partition: String,
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Offset into results.
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
    },
    /// Rename or move a file within an actor workspace.
    FileRename {
        /// Actor partition the rename is attributed to.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Current workspace-relative path.
        #[arg(value_name = "FROM")]
        from: String,
        /// New workspace-relative path.
        #[arg(value_name = "TO")]
        to: String,
        /// Optional file supplying the destination content; empty when omitted.
        #[arg(long, value_name = "FILE")]
        content_file: Option<String>,
    },
    /// Report state-to-file association health for state checkpoints.
    FileStateLinks {
        /// Comma-separated state checkpoint ids.
        #[arg(value_name = "IDS", value_delimiter = ',')]
        ids: Vec<String>,
    },
    /// List file changes by actor and/or path.
    FileChanges {
        /// Actor partition to query.
        #[arg(long, value_name = "ACTOR")]
        actor: Option<String>,
        /// Path substring filter (or the full path query when no actor given).
        #[arg(long, value_name = "PATH")]
        path: Option<String>,
        /// Start of the time window (epoch seconds).
        #[arg(long, value_name = "TIMESTAMP")]
        start: Option<i64>,
        /// End of the time window (epoch seconds).
        #[arg(long, value_name = "TIMESTAMP")]
        end: Option<i64>,
    },
    /// Show the per-file diff between an actor workspace and the main line.
    FileDiffMain {
        /// Actor partition to compare.
        #[arg(value_name = "ACTOR")]
        actor: String,
    },
    /// Show the version timeline of a file path, following renames.
    FileTimeline {
        /// Workspace-relative file path.
        #[arg(value_name = "PATH")]
        path: String,
    },
    /// List staged file edit sessions (edit groups), newest first.
    FileSessionList {
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Offset into results.
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
    },
    /// Begin a file edit session (edit group) and print its id.
    FileSessionBegin {
        /// Optional human-readable session label.
        #[arg(long, value_name = "LABEL")]
        label: Option<String>,
    },
    /// Roll back an entire edit session on an actor partition.
    FileSessionRollback {
        /// Edit session id to roll back.
        #[arg(value_name = "SESSION")]
        session: String,
        /// Actor partition the session applies to.
        #[arg(value_name = "ACTOR")]
        actor: String,
    },
    /// Undo the last edit on an actor partition.
    FileUndo {
        /// Actor partition to undo.
        #[arg(value_name = "ACTOR")]
        actor: String,
    },
    /// Redo the last undone edit on an actor partition.
    FileRedo {
        /// Actor partition to redo.
        #[arg(value_name = "ACTOR")]
        actor: String,
    },
    /// Show the reconstructed file set of an actor workspace.
    FileWorkspace {
        /// Actor partition to inspect.
        #[arg(value_name = "ACTOR")]
        actor: String,
    },
    /// List the file tree of an actor workspace.
    FileTree {
        /// Actor partition to inspect.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Optional path prefix filter.
        #[arg(long, value_name = "PREFIX")]
        prefix: Option<String>,
    },
    /// List files with unresolved merge conflicts.
    FileConflicts,
    /// Resolve merge conflicts on a feature ref by writing full-file contents.
    FileConflictResolve {
        /// Actor the resolution is attributed to.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Feature name for `refs/wf/feat/<name>`.
        #[arg(value_name = "FEATURE")]
        feature: String,
        /// Repeatable `PATH=FILE` pair; the file supplies the resolved bytes.
        #[arg(long = "resolve", value_name = "PATH=FILE")]
        resolutions: Vec<String>,
        /// Optional session attribution recorded alongside the actor.
        #[arg(long, value_name = "SESSION")]
        session: Option<String>,
    },
    /// Read one file's content from an actor workspace, optionally at a commit.
    FileContent {
        /// Actor partition to read.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Workspace-relative file path.
        #[arg(value_name = "PATH")]
        path: String,
        /// Historical commit id to read the version at.
        #[arg(long, value_name = "COMMIT")]
        commit: Option<String>,
    },
    /// Show the per-file diff between two actor workspaces.
    FileDiffActors {
        /// Left-hand actor partition.
        #[arg(value_name = "ACTOR_A")]
        actor_a: String,
        /// Right-hand actor partition.
        #[arg(value_name = "ACTOR_B")]
        actor_b: String,
    },
    /// Show the object-store footprint of the file-checkpoint store.
    FileStats,
    /// Preview undo of the last input boundary for an execution.
    FileUndoLastInputPreview {
        /// Execution id owning the input boundary.
        #[arg(value_name = "EXECUTION")]
        execution: String,
        /// Optional scope prefix override (repeatable).
        #[arg(long = "prefix", value_name = "PREFIX")]
        checkpoint_prefixes: Vec<String>,
    },
    /// Apply undo of the last input boundary for an execution.
    FileUndoLastInput {
        /// Execution id owning the input boundary.
        #[arg(value_name = "EXECUTION")]
        execution: String,
        /// Optional scope prefix override (repeatable).
        #[arg(long = "prefix", value_name = "PREFIX")]
        checkpoint_prefixes: Vec<String>,
    },
    /// Restore the bound workspace from a file checkpoint as new content
    /// writes; commit history is not rewritten.
    FileRestore {
        /// Execution or entity id owning the checkpoint.
        #[arg(value_name = "ENTITY")]
        entity: String,
        /// File checkpoint (commit) id to restore from.
        #[arg(long, value_name = "CHECKPOINT")]
        checkpoint: String,
        /// Preview what would be restored without executing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run object-store cleanup on the file-checkpoint store.
    FileGc {
        /// Keep this many recent partition heads protected beyond the
        /// built-in protected set.
        #[arg(long, value_name = "N", default_value_t = 0)]
        keep_recent_heads: usize,
    },
    /// Submit an actor edit line for review (edit ref copy, no merge).
    FileSubmit {
        /// Actor the edit line is attributed to (required, no anonymous writes).
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Optional session attribution recorded alongside the actor.
        #[arg(long, value_name = "SESSION")]
        session: Option<String>,
    },
    /// Merge an actor submission into a feature ref via three-way merge.
    FileMergeToFeature {
        /// Actor the submission is attributed to.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Feature name for `refs/wf/feat/<name>`.
        #[arg(value_name = "FEATURE")]
        feature: String,
        /// Optional session attribution recorded alongside the actor.
        #[arg(long, value_name = "SESSION")]
        session: Option<String>,
    },
    /// Merge named features into main in input order, one merge commit
    /// per feature. A conflicted feature aborts the sequence while
    /// earlier merges stay individually revertable.
    FileMergeToMain {
        /// Actor the merge is attributed to.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Feature names merged in input order.
        #[arg(value_name = "FEATURE")]
        features: Vec<String>,
        /// Optional session attribution recorded alongside the actor.
        #[arg(long, value_name = "SESSION")]
        session: Option<String>,
    },
    /// Roll back one merged feature from main by appending a revert
    /// commit. Other merged features are untouched.
    FileRollbackFeature {
        /// Actor the rollback is attributed to.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Merged feature head commit to revert.
        #[arg(value_name = "HEAD")]
        feature_head: String,
        /// Optional session attribution recorded alongside the actor.
        #[arg(long, value_name = "SESSION")]
        session: Option<String>,
    },
    /// Rebuild the source index from the commit graph (ops channel).
    FileIndexRebuild,
    /// Show source-index health: indexed rows versus reachable commits.
    FileIndexStatus,
    /// Export the main tree into the user workspace and record a reconcile point.
    FileSyncExport {
        /// Confirm overwriting worktree changes that differ from main.
        #[arg(long)]
        confirm_overwrite: bool,
        /// Override the configured auto-commit default.
        #[arg(long, value_name = "BOOL")]
        auto_commit: Option<bool>,
        /// Optional commit message; otherwise one is suggested.
        #[arg(long, value_name = "MESSAGE")]
        message: Option<String>,
    },
    /// Import the external head as human history and advance the reconcile point.
    FileSyncImport,
    /// Move the baseline to a newly resolved commit, transplanting the old main tree.
    FileSyncRebase {
        /// User-repository branch the new baseline is resolved from.
        #[arg(long, value_name = "BRANCH")]
        branch: Option<String>,
        /// Commit hash prefix pinned inside the user repository.
        #[arg(long, value_name = "PREFIX")]
        commit_prefix: Option<String>,
        /// Nth ancestor back from the branch tip along the first-parent chain.
        #[arg(long, value_name = "N")]
        behind: Option<u64>,
        /// Baseline mode: borrow (default) or copy.
        #[arg(long, value_name = "MODE", default_value = "borrow")]
        mode: String,
    },
    /// Show the external update hint when the external head moved past reconcile.
    FileSyncHint,
    /// Show the current reconcile point, if an export has recorded one.
    FileSyncReconcile,
    /// Show the configured sync defaults.
    FileSyncConfig,
    /// List checkpoints of an execution (domain auto-resolved).
    List {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Offset into results.
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
        /// Explicit domain override, only needed for ambiguous ids.
        #[arg(long, value_enum)]
        domain: Option<CheckpointDomain>,
    },
    /// Show a checkpoint.
    Show {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Set tags on a checkpoint (replaces existing tags).
    Tag {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
        /// Comma-separated tags.
        #[arg(value_name = "TAGS", value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// Remove all tags from a checkpoint.
    Untag {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Compare two checkpoints and show their metadata differences.
    Diff {
        /// First checkpoint id.
        #[arg(value_name = "ID_A")]
        a: String,
        /// Second checkpoint id.
        #[arg(value_name = "ID_B")]
        b: String,
    },
    /// Restore a workflow checkpoint as a new execution branch; history
    /// is not rewritten and other branches are untouched.
    Restore {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
        /// Also resume the restored execution.
        #[arg(long)]
        resume: bool,
        /// Preview what would be restored without executing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Restore an agent-loop checkpoint as a new execution branch;
    /// history is not rewritten.
    RestoreAgent {
        /// Agent loop id.
        #[arg(value_name = "ID")]
        id: String,
        /// Checkpoint id.
        #[arg(long, value_name = "CHECKPOINT")]
        checkpoint: String,
        /// Preview what would be restored without executing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete a checkpoint.
    Delete {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Show the checkpoint chain of an execution (chronological with transitions).
    Chain {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Explicit domain override; agent loops must pass `agent`, workflow
        /// executions default to the workflow chain endpoint.
        #[arg(long, value_enum)]
        domain: Option<CheckpointDomain>,
    },
    /// GC checkpoints of an execution (optionally before a timestamp).
    Gc {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Only delete checkpoints before this epoch millis (when omitted delete all of the execution).
        #[arg(long, value_name = "TIMESTAMP")]
        before: Option<i64>,
        /// Explicit domain override, only needed for ambiguous ids.
        #[arg(long, value_enum)]
        domain: Option<CheckpointDomain>,
    },
    /// Show the latest checkpoint of an entity.
    Latest {
        /// Execution or entity id.
        #[arg(value_name = "ENTITY")]
        entity: String,
        /// Explicit domain override, only needed for ambiguous ids.
        #[arg(long, value_enum)]
        domain: Option<CheckpointDomain>,
    },
    /// List checkpoints of several entities in one call.
    Entities {
        /// Execution or entity ids (comma-separated).
        #[arg(value_name = "IDS", value_delimiter = ',')]
        ids: Vec<String>,
        /// Explicit domain override applied to every id.
        #[arg(long, value_enum)]
        domain: Option<CheckpointDomain>,
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Offset into results.
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
    },
    /// List checkpoints of a workflow's executions within a time range.
    TimeRange {
        /// Workflow id owning the executions.
        #[arg(value_name = "WORKFLOW")]
        workflow: String,
        /// Start of the window (epoch millis).
        #[arg(long, value_name = "MILLIS")]
        start: i64,
        /// End of the window (epoch millis).
        #[arg(long, value_name = "MILLIS")]
        end: i64,
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Offset into results.
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
    },
    /// Show entity-level checkpoint metadata.
    MetadataGet {
        /// Execution or entity id.
        #[arg(value_name = "ENTITY")]
        entity: String,
    },
    /// Replace entity-level checkpoint metadata with a JSON object.
    MetadataSet {
        /// Execution or entity id.
        #[arg(value_name = "ENTITY")]
        entity: String,
        /// JSON object holding the metadata to store.
        #[arg(long, value_name = "JSON")]
        json: String,
    },
    /// Delete all checkpoints of an entity.
    DeleteByEntity {
        /// Execution or entity id.
        #[arg(value_name = "ENTITY")]
        entity: String,
        /// Explicit domain override, only needed for ambiguous ids.
        #[arg(long, value_enum)]
        domain: Option<CheckpointDomain>,
    },
}

/// Approval subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ApprovalSub {
    /// List pending approvals.
    List,
    /// Approve a pending approval.
    Approve {
        /// Agent instance id.
        #[arg(value_name = "INSTANCE")]
        instance: String,
        /// Feature name (auto-derived when omitted).
        #[arg(long, value_name = "FEATURE")]
        feature: Option<String>,
        /// Specific file paths (comma-separated).
        #[arg(long, value_name = "PATHS")]
        paths: Option<String>,
    },
    /// Reject a pending approval.
    Reject {
        /// Agent instance id.
        #[arg(value_name = "INSTANCE")]
        instance: String,
        /// Optional human-readable rejection reason (logging only).
        #[arg(long, value_name = "REASON")]
        reason: Option<String>,
    },
}

#[cfg(test)]
mod test;
