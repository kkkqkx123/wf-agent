use wf_checkpoint::file::session::EditGroup;
use wf_checkpoint::file::FileCheckpointOptions;
use wf_checkpoint::gc::{GcRetention, GcStats};
use wf_checkpoint::provenance::{
    DeltaSummary, FileDiffView, PartitionCommitView, PartitionView, WorkspaceFile,
};

use super::file::FileCheckpointSummary;
use crate::infra::context::ApiContext;
use crate::ApiError;
use crate::ApiResult;

/// The attached file checkpoint manager, or an error when file
/// checkpointing is disabled.
fn manager(ctx: &ApiContext) -> ApiResult<&wf_checkpoint::FileCheckpointManager> {
    ctx.file_checkpoint_manager().ok_or_else(|| {
        ApiError::execution("file checkpointing is not enabled; set file_checkpoint.enabled=true")
    })
}

/// Workspace root bound to the attached manager, or an error when the
/// manager has no workspace bound (direct workspace writes need one).
fn bound_workspace_root(
    manager: &wf_checkpoint::FileCheckpointManager,
) -> ApiResult<std::path::PathBuf> {
    manager
        .workspace_root()
        .map(|root| root.to_path_buf())
        .ok_or_else(|| ApiError::execution("file checkpoint workspace root is not configured"))
}

/// Create a full workspace checkpoint for an actor against the bound
/// workspace root. Used by the HTTP create endpoint and CLI troubleshooting
/// commands; execution-driven snapshots keep using the coordinator path.
pub fn create_workspace_checkpoint(
    ctx: &ApiContext,
    actor: &str,
) -> ApiResult<FileCheckpointSummary> {
    if actor.trim().is_empty() {
        return Err(ApiError::Validation("actor must not be empty".to_string()));
    }
    let manager = manager(ctx)?;
    let root = bound_workspace_root(manager)?;
    let opts = FileCheckpointOptions::default();
    let checkpoint = manager
        .create_workspace_checkpoint(actor, &root, &opts)
        .map_err(ApiError::execution_with_source)?;
    Ok(FileCheckpointSummary {
        id: checkpoint.id,
        timestamp: checkpoint.timestamp,
        file_count: checkpoint.files.len(),
    })
}

/// Restore the bound workspace from a file checkpoint. Returns the number
/// of restored files.
pub fn restore_workspace(
    ctx: &ApiContext,
    entity_id: &str,
    checkpoint_id: &str,
) -> ApiResult<usize> {
    if entity_id.trim().is_empty() {
        return Err(ApiError::Validation(
            "entity id must not be empty".to_string(),
        ));
    }
    if checkpoint_id.trim().is_empty() {
        return Err(ApiError::Validation(
            "checkpoint id must not be empty".to_string(),
        ));
    }
    let manager = manager(ctx)?;
    let root = bound_workspace_root(manager)?;
    let opts = FileCheckpointOptions::default();
    let result = manager
        .restore_workspace(entity_id, checkpoint_id, &root, &opts)
        .map_err(ApiError::execution_with_source)?;
    Ok(result.restored)
}

/// All partitions of the file-checkpoint store (actor partitions, approval,
/// integrated features, staged).
pub fn list_partitions(ctx: &ApiContext) -> ApiResult<Vec<PartitionView>> {
    manager(ctx)?
        .list_partitions()
        .map_err(ApiError::execution_with_source)
}

/// Commit history of one partition ref, newest first in walk order. The
/// ref must match a `refs/wf/` ref exactly (percent-encoded slashes from
/// the route arrive decoded here).
pub fn partition_history(
    ctx: &ApiContext,
    partition: &str,
    limit: usize,
    offset: usize,
) -> ApiResult<Vec<PartitionCommitView>> {
    if partition.trim().is_empty() {
        return Err(ApiError::Validation(
            "partition must not be empty".to_string(),
        ));
    }
    manager(ctx)?
        .partition_history(partition, limit, offset)
        .map_err(ApiError::execution_with_source)
}

/// Changes recorded in an actor partition, in chronological order, with
/// optional path substring filter and inclusive time window.
pub fn list_changes_by_actor(
    ctx: &ApiContext,
    actor: &str,
    path_filter: Option<&str>,
    time_range: Option<(i64, i64)>,
) -> ApiResult<Vec<DeltaSummary>> {
    manager(ctx)?
        .list_changes_by_actor(actor, path_filter, time_range)
        .map_err(ApiError::execution_with_source)
}

/// Changes touching a path across every partition, with an optional
/// inclusive time window.
pub fn list_changes_by_path(
    ctx: &ApiContext,
    path: &str,
    time_range: Option<(i64, i64)>,
) -> ApiResult<Vec<DeltaSummary>> {
    manager(ctx)?
        .list_changes_by_path(path, time_range)
        .map_err(ApiError::execution_with_source)
}

/// Reconstructed file set of an actor partition (current state).
pub fn get_actor_workspace(ctx: &ApiContext, actor: &str) -> ApiResult<Vec<WorkspaceFile>> {
    manager(ctx)?
        .get_actor_workspace(actor)
        .map_err(ApiError::execution_with_source)
}

/// Per-file diff between two actor workspaces.
pub fn diff_actors(ctx: &ApiContext, actor_a: &str, actor_b: &str) -> ApiResult<Vec<FileDiffView>> {
    manager(ctx)?
        .diff_actors(actor_a, actor_b)
        .map_err(ApiError::execution_with_source)
}

/// Per-file diff between an actor workspace and the main line.
pub fn diff_against_main(ctx: &ApiContext, actor: &str) -> ApiResult<Vec<FileDiffView>> {
    manager(ctx)?
        .diff_against_main(actor)
        .map_err(ApiError::execution_with_source)
}

/// Complete version timeline for a file path, including rename/move history.
pub fn file_timeline(
    ctx: &ApiContext,
    path: &str,
) -> ApiResult<wf_checkpoint::provenance::FileTimeline> {
    manager(ctx)?
        .file_timeline(path)
        .map_err(ApiError::execution_with_source)
}

/// Maximum entries returned by a file timeline view.
pub const MAX_FILE_TIMELINE_ENTRIES: usize = 5000;

/// Capped file timeline view with total before truncation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileTimelineView {
    pub original_path: String,
    pub entries: Vec<wf_checkpoint::provenance::FileTimelineEntry>,
    pub truncated: bool,
    pub total: usize,
}

/// File timeline with cap and truncation flag. Entries are chronological, so
/// the earliest page is stable.
pub fn file_timeline_capped(ctx: &ApiContext, path: &str) -> ApiResult<FileTimelineView> {
    let timeline = file_timeline(ctx, path)?;
    let total = timeline.entries.len();
    let truncated = total > MAX_FILE_TIMELINE_ENTRIES;
    let mut entries = timeline.entries;
    entries.truncate(MAX_FILE_TIMELINE_ENTRIES);
    Ok(FileTimelineView {
        original_path: timeline.original_path,
        entries,
        truncated,
        total,
    })
}

/// Explicit rename: record the move linkage and apply it as delete-old +
/// edit-new for the actor's partition. Returns the new snapshot id (hex).
pub fn rename_file(
    ctx: &ApiContext,
    actor: &str,
    from_path: &str,
    to_path: &str,
    content: &[u8],
) -> ApiResult<String> {
    let manager = manager(ctx)?;
    let actor_id = manager.actor_id_for(actor);
    manager
        .rename_file(&actor_id, from_path, to_path, content)
        .map_err(ApiError::execution_with_source)
}

/// Begin a new edit group for grouping a multi-file operation.
pub fn begin_edit_group(ctx: &ApiContext, label: Option<String>) -> ApiResult<String> {
    manager(ctx)?
        .begin_edit_group(label)
        .map(|id| id.to_string())
        .map_err(ApiError::execution_with_source)
}

/// List all staged edit groups (newest first).
pub fn list_edit_groups(ctx: &ApiContext) -> ApiResult<Vec<EditGroup>> {
    manager(ctx)?
        .list_edit_groups()
        .map_err(ApiError::execution_with_source)
}

/// Roll back an entire edit group on an actor's partition.
pub fn rollback_edit_group(ctx: &ApiContext, actor: &str, group_id: &str) -> ApiResult<String> {
    let group_id = group_id
        .parse()
        .map_err(|_| ApiError::execution(format!("invalid edit group id '{group_id}'")))?;
    manager(ctx)?
        .rollback_edit_group(actor, &group_id)
        .map_err(ApiError::execution_with_source)
}

/// Undo the last edit on an actor's partition.
pub fn undo_edit(ctx: &ApiContext, actor: &str) -> ApiResult<String> {
    manager(ctx)?
        .undo_edit(actor)
        .map_err(ApiError::execution_with_source)
}

/// Redo the most recently undone edit on an actor's partition.
pub fn redo_edit(ctx: &ApiContext, actor: &str) -> ApiResult<String> {
    manager(ctx)?
        .redo_edit(actor)
        .map_err(ApiError::execution_with_source)
}

/// Trigger a manual GC run on the file-checkpoint store. `keep_recent_heads`
/// controls how many recent partition heads are kept protected beyond the
/// built-in protected set (branch heads + ancestors + git anchors).
pub fn run_gc(ctx: &ApiContext, keep_recent_heads: usize) -> ApiResult<GcStats> {
    let retention = GcRetention { keep_recent_heads };
    manager(ctx)?
        .run_gc(retention)
        .map_err(ApiError::execution_with_source)
}

/// Maximum bytes returned by a single file content read; larger files are
/// truncated with `truncated: true`.
pub const MAX_FILE_CONTENT_BYTES: usize = 1024 * 1024;
/// Maximum entries returned by a directory tree listing.
pub const MAX_TREE_ENTRIES: usize = 2000;

/// Read-only view of one workspace file. `version` is `None` for workspace
/// current state; snapshot-versioned reads are distinguished by callers via
/// the file timeline (snapshot ids + hashes) and remain timeline-anchored.
/// Direct workspace mutations (rename, sessions, undo/redo) are explicit file
/// operations; the approval channel covers human-in-the-loop tool approvals.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileContentView {
    pub path: String,
    pub actor: String,
    pub hash: String,
    pub size: usize,
    pub is_binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default)]
    pub truncated: bool,
    pub timestamp: i64,
}

/// One directory tree entry (metadata only, no content bytes).
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileTreeEntry {
    pub path: String,
    pub hash: String,
    pub size: usize,
    pub timestamp: i64,
}

/// Validate a workspace-relative path against sandbox escape.
pub fn validate_workspace_path(path: &str) -> ApiResult<()> {
    validate_workspace_path_for_actor(None, path)
}

/// Validate a workspace-relative path with structured audit on rejection.
/// `actor` identifies the caller for the audit record; `None` logs as
/// `unknown`. Every rejection emits a structured `tracing::warn!` with
/// actor, path and reason so out-of-bound writes stay observable.
pub fn validate_workspace_path_for_actor(actor: Option<&str>, path: &str) -> ApiResult<()> {
    let who = actor.unwrap_or("unknown");
    let reject = |reason: &str| {
        tracing::warn!(
            actor = %who,
            path = %path,
            reason = %reason,
            "file checkpoint path rejected"
        );
        ApiError::Validation(reason.to_string())
    };
    if path.is_empty() || path.len() > 1024 {
        return Err(reject("path must be 1..=1024 chars"));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(reject("absolute paths are rejected"));
    }
    for component in path.split('/') {
        if component == ".." {
            return Err(reject("path escapes the workspace"));
        }
    }
    if path.contains('\\') || path.contains('\0') {
        return Err(reject("path contains illegal characters"));
    }
    Ok(())
}

/// Structured audit record for silently ignored or rejected file writes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScopeAuditEntry {
    pub actor: String,
    pub path: String,
    pub reason: String,
    pub timestamp: i64,
}

/// Emit a structured audit for an ignored write (ignore-rule hit or
/// out-of-scope path). Callers in the watcher and poll paths use this so
/// every silent skip carries actor, path and reason.
pub fn audit_ignored_write(actor: &str, path: &str, reason: &str) -> ScopeAuditEntry {
    tracing::warn!(
        actor = %actor,
        path = %path,
        reason = %reason,
        "file checkpoint write ignored"
    );
    ScopeAuditEntry {
        actor: actor.to_string(),
        path: path.to_string(),
        reason: reason.to_string(),
        timestamp: wf_common::now(),
    }
}

fn is_binary_bytes(content: &[u8]) -> bool {
    content.iter().take(8000).any(|b| *b == 0)
}

/// Shape raw file bytes into a capped content view with shared truncation
/// and binary rules for current-state and version-anchored reads.
fn capped_content_view(
    path: &str,
    actor: &str,
    content: Vec<u8>,
    hash: String,
    timestamp: i64,
) -> FileContentView {
    let binary = is_binary_bytes(&content);
    let truncated = content.len() > MAX_FILE_CONTENT_BYTES;
    let capped = content
        .get(..MAX_FILE_CONTENT_BYTES.min(content.len()))
        .unwrap_or_default();
    let text = if binary {
        None
    } else {
        Some(String::from_utf8_lossy(capped).into_owned())
    };
    FileContentView {
        path: path.to_string(),
        actor: actor.to_string(),
        hash,
        size: content.len(),
        is_binary: binary,
        content: text,
        truncated,
        timestamp,
    }
}

/// Files with unresolved merge conflicts across every partition.
pub fn list_conflicts(ctx: &ApiContext) -> ApiResult<Vec<wf_checkpoint::provenance::ConflictFile>> {
    manager(ctx)?
        .list_conflicts()
        .map_err(ApiError::execution_with_source)
}

/// Read one file's bytes at a historical commit without touching the
/// workspace. The commit id comes from the file timeline; the hash is the
/// content hash of the bytes at that commit and the timestamp is the commit
/// time. Truncation and binary rules match current-state reads.
pub fn read_file_content_at(
    ctx: &ApiContext,
    actor: &str,
    path: &str,
    version: &str,
) -> ApiResult<FileContentView> {
    validate_workspace_path_for_actor(Some(actor), path)?;
    if version.trim().is_empty() {
        return Err(ApiError::Validation(
            "version must not be empty".to_string(),
        ));
    }
    let manager = manager(ctx)?;
    let content = manager
        .read_file_at(version, path)
        .map_err(ApiError::execution_with_source)?
        .ok_or_else(|| ApiError::not_found("file", path))?;
    let timestamp = manager
        .commit_timestamp(version)
        .map_err(ApiError::execution_with_source)?
        .unwrap_or(0);
    let hash = wf_checkpoint::sha256_hex(&content);
    Ok(capped_content_view(path, actor, content, hash, timestamp))
}

/// Read the current workspace content of one file with sandbox checks.
pub fn read_file_content(ctx: &ApiContext, actor: &str, path: &str) -> ApiResult<FileContentView> {
    validate_workspace_path_for_actor(Some(actor), path)?;
    let files = get_actor_workspace(ctx, actor)?;
    let Some(file) = files.into_iter().find(|f| f.path == path) else {
        return Err(ApiError::not_found("file", path));
    };
    Ok(capped_content_view(
        &file.path,
        actor,
        file.content,
        file.hash,
        file.timestamp,
    ))
}

/// Capped directory tree view with total before truncation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileTreeView {
    pub entries: Vec<FileTreeEntry>,
    pub truncated: bool,
    pub total: usize,
}

/// List workspace tree entries with an optional path prefix filter.
pub fn list_tree(
    ctx: &ApiContext,
    actor: &str,
    prefix: Option<&str>,
) -> ApiResult<Vec<FileTreeEntry>> {
    Ok(list_tree_capped(ctx, actor, prefix)?.entries)
}

/// List workspace tree entries with cap and truncation flag. Entries are
/// sorted by path before truncation so the first page is stable.
pub fn list_tree_capped(
    ctx: &ApiContext,
    actor: &str,
    prefix: Option<&str>,
) -> ApiResult<FileTreeView> {
    if let Some(prefix) = prefix {
        if !prefix.is_empty() {
            if let Err(err) = validate_workspace_path_for_actor(Some(actor), prefix) {
                audit_ignored_write(actor, prefix, &err.to_string());
                return Err(err);
            }
        }
    }
    let files = get_actor_workspace(ctx, actor)?;
    let mut out = Vec::new();
    for file in files {
        if let Some(prefix) = prefix {
            if !prefix.is_empty() && !file.path.starts_with(prefix) {
                continue;
            }
        }
        out.push(FileTreeEntry {
            path: file.path,
            hash: file.hash,
            size: file.content.len(),
            timestamp: file.timestamp,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    let total = out.len();
    let truncated = total > MAX_TREE_ENTRIES;
    out.truncate(MAX_TREE_ENTRIES);
    Ok(FileTreeView {
        entries: out,
        truncated,
        total,
    })
}

/// Rebuild the source index from the commit graph (ops channel).
/// Used after index loss; queries fall back to bounded graph scans until
/// then. Returns the number of indexed commits.
pub fn rebuild_source_index(ctx: &ApiContext) -> ApiResult<usize> {
    manager(ctx)?
        .rebuild_source_index()
        .map_err(ApiError::execution_with_source)
}

/// Source-index health for the ops channel (indexed rows versus
/// reachable commits, plus a rebuild hint).
pub fn index_status(ctx: &ApiContext) -> ApiResult<wf_checkpoint::provenance::IndexStatus> {
    manager(ctx)?
        .index_status()
        .map_err(ApiError::execution_with_source)
}

fn require_attribution(actor: &str, session: Option<&str>) -> ApiResult<()> {
    if actor.trim().is_empty() {
        return Err(ApiError::Validation("actor must not be empty".to_string()));
    }
    if let Some(session) = session {
        if session.trim().is_empty() {
            return Err(ApiError::Validation(
                "session must not be empty when provided".to_string(),
            ));
        }
    }
    Ok(())
}

/// Submit an actor edit line for review (edit ref copy, no merge).
/// Requires actor attribution; session is optional but recorded when given.
pub fn submit_for_review(
    ctx: &ApiContext,
    actor: &str,
    session: Option<&str>,
) -> ApiResult<String> {
    require_attribution(actor, session)?;
    manager(ctx)?
        .move_agent_to_approval(actor)
        .map_err(ApiError::execution_with_source)
}

/// Merge an actor submission into a feature ref via three-way merge.
/// Requires actor attribution.
pub fn merge_to_feature(
    ctx: &ApiContext,
    actor: &str,
    feature: &str,
    session: Option<&str>,
) -> ApiResult<wf_checkpoint::file::git_merge::GitMergeOutcome> {
    require_attribution(actor, session)?;
    if feature.trim().is_empty() {
        return Err(ApiError::Validation(
            "feature must not be empty".to_string(),
        ));
    }
    manager(ctx)?
        .merge_agent_to_feature(actor, feature)
        .map_err(ApiError::execution_with_source)
}

/// Merge named features into main in input order, one merge commit per
/// feature. Requires actor attribution; a conflicted feature aborts the
/// sequence while earlier merges stay individually revertable.
pub fn merge_to_main(
    ctx: &ApiContext,
    actor: &str,
    features: &[String],
    session: Option<&str>,
) -> ApiResult<wf_checkpoint::MergeCommitResult> {
    require_attribution(actor, session)?;
    if features.is_empty() {
        return Err(ApiError::Validation(
            "features must not be empty".to_string(),
        ));
    }
    let refs: Vec<&str> = features.iter().map(String::as_str).collect();
    manager(ctx)?
        .merge_features_to_main(&refs)
        .map_err(ApiError::execution_with_source)
}

/// Roll back one merged feature from main by appending a revert commit.
/// Other merged features are untouched. Requires actor attribution.
pub fn rollback_feature(
    ctx: &ApiContext,
    actor: &str,
    feature_head: &str,
    session: Option<&str>,
) -> ApiResult<String> {
    require_attribution(actor, session)?;
    if feature_head.trim().is_empty() {
        return Err(ApiError::Validation(
            "feature_head must not be empty".to_string(),
        ));
    }
    manager(ctx)?
        .rollback_feature_from_main(feature_head, actor)
        .map_err(ApiError::execution_with_source)
}

/// Resolve conflicts on a feature ref by committing resolved content.
/// Returns remaining unresolved files (0 means fully resolved).
/// Requires actor attribution.
pub fn resolve_feature_conflicts(
    ctx: &ApiContext,
    actor: &str,
    feature: &str,
    resolutions: Vec<(String, Vec<u8>)>,
    session: Option<&str>,
) -> ApiResult<usize> {
    require_attribution(actor, session)?;
    if feature.trim().is_empty() {
        return Err(ApiError::Validation(
            "feature must not be empty".to_string(),
        ));
    }
    if resolutions.is_empty() {
        return Err(ApiError::Validation(
            "resolutions must not be empty".to_string(),
        ));
    }
    for (path, _) in &resolutions {
        validate_workspace_path_for_actor(Some(actor), path)?;
    }
    let owned: Vec<(String, Vec<u8>)> = resolutions;
    let refs: Vec<(String, Vec<u8>)> = owned;
    manager(ctx)?
        .resolve_conflicts(actor, feature, &refs)
        .map_err(ApiError::execution_with_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_path_rejects_escape_and_absolute() {
        assert!(validate_workspace_path("src/main.rs").is_ok());
        assert!(validate_workspace_path("").is_err());
        assert!(validate_workspace_path("/etc/passwd").is_err());
        assert!(validate_workspace_path("../secret").is_err());
        assert!(validate_workspace_path("a/../../b").is_err());
        assert!(validate_workspace_path("a\\b").is_err());
    }

    #[test]
    fn attributed_path_validation_rejects_anonymous_and_bad_paths() {
        assert!(validate_workspace_path_for_actor(Some("exec-1"), "src/main.rs").is_ok());
        assert!(validate_workspace_path_for_actor(Some("exec-1"), "../secret").is_err());
        assert!(validate_workspace_path_for_actor(Some("exec-1"), "/etc/passwd").is_err());
        assert!(validate_workspace_path_for_actor(None, "src/main.rs").is_ok());
    }

    #[test]
    fn ignored_write_audit_carries_actor_path_and_reason() {
        let entry = audit_ignored_write("exec-1", "dist/bundle.js", "ignore-rule hit");
        assert_eq!(entry.actor, "exec-1");
        assert_eq!(entry.path, "dist/bundle.js");
        assert_eq!(entry.reason, "ignore-rule hit");
        assert!(entry.timestamp > 0);
    }
}
