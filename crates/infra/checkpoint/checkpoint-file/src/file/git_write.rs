//! Git-backed write path: every new file content lands in the object
//! store, never in the legacy delta/snapshot rows.
//!
//! Rules enforced here:
//! - tool reports are the primary path and carry in-memory bytes (no disk
//!   re-read);
//! - one tool call is one commit; multi-file operations commit atomically;
//! - script runs diff before/after inside the declared writable scope and
//!   produce exactly one atomic commit;
//! - human edits land on the human ref via periodic polling and are never
//!   auto-merged;
//! - session grouping is a commit trailer, not an association table;
//! - empty commits create nothing (the existing commit id is returned).

use std::collections::HashMap;

use crate::event::CheckpointEventBus;
use crate::file::util::{compute_full_hash, sha256_hex, validate_workspace_relative_path};
use crate::file::{FileCheckpoint, FileState};
use crate::git_store::{
    commit_message, edit_ref_for_actor, CommitOnRefOutcome, GitStoreError, MODE_FILE, REF_HUMAN,
};
use crate::provenance::DeltaSummary;
use crate::scan::{ScanConfig, WorkspaceScanner};
use checkpoint_base::actor::id::ActorId;
use checkpoint_base::error::CheckpointError;

use super::FileCheckpointManager;

/// One staged multi-file operation awaiting its single atomic commit.
#[derive(Debug, Clone, Default)]
pub(crate) struct PendingEditBatch {
    pub(crate) label: Option<String>,
    pub(crate) actor: String,
    pub(crate) created_at: i64,
    pub(crate) files: HashMap<String, Option<Vec<u8>>>,
}

/// Outcome of a Git write: the commit id plus whether a commit was created.
#[derive(Debug, Clone)]
pub struct GitWriteOutcome {
    pub id: String,
    pub created: bool,
}

pub(crate) fn map_git_error(e: GitStoreError) -> CheckpointError {
    match e {
        GitStoreError::Uninitialized(reason) => CheckpointError::Coordinator(reason),
        GitStoreError::RefNotFound(id) => CheckpointError::NotFound { id },
        GitStoreError::ObjectNotFound(id) => CheckpointError::NotFound { id },
        GitStoreError::RefConflict(name) => {
            CheckpointError::Branch(format!("concurrent ref update conflict on '{name}'"))
        }
        GitStoreError::Corrupt { id, reason } => CheckpointError::Corrupted { id, reason },
        GitStoreError::InvalidInput(reason) => CheckpointError::Validation { reason },
        GitStoreError::Io(reason) => CheckpointError::Internal(format!("git store io: {reason}")),
    }
}

impl FileCheckpointManager {
    /// Core write: commit `files` (`None` = deletion) on the actor's edit
    /// ref in a single atomic commit. `session`/`tool` become trailers.
    pub(crate) fn commit_tool_files(
        &self,
        actor: &ActorId,
        files: &HashMap<String, Option<Vec<u8>>>,
        session: Option<&str>,
        tool: Option<&str>,
        intent: &str,
    ) -> Result<GitWriteOutcome, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut changes: HashMap<String, Option<(String, Vec<u8>)>> =
            HashMap::with_capacity(files.len());
        let mut paths: Vec<String> = Vec::with_capacity(files.len());
        for (path, content) in files {
            let validated = validate_workspace_relative_path(path)?;
            paths.push(validated.clone());
            changes.insert(
                validated,
                content.clone().map(|bytes| (MODE_FILE.to_string(), bytes)),
            );
        }
        paths.sort();
        let refname = edit_ref_for_actor(actor.as_str());
        let message = commit_message(intent, Some(actor.as_str()), session, tool, &[]);
        let CommitOnRefOutcome { id, created } = git
            .commit_on_ref(&refname, &changes, actor.as_str(), &message)
            .map_err(map_git_error)?;
        if created {
            self.index_commit(
                storage,
                &id,
                actor.as_str(),
                session.unwrap_or(""),
                tool.unwrap_or(""),
                &paths,
            )?;
            self.store
                .latest_checkpoints
                .insert(actor.as_str().to_string(), id.clone());
            self.redo_stacks.remove(actor.as_str());
        }
        for path in &paths {
            let bytes = files.get(path).and_then(|c| c.as_deref());
            if created {
                self.publish_file_event(&id, path, actor.as_str(), bytes);
            }
        }
        Ok(GitWriteOutcome { id, created })
    }

    /// Record the source-index entry plus the empty-dir manifest hook for a
    /// new commit. The index is acceleration-only and rebuildable. The
    /// manifest is inherited from the commit's parents so linear history
    /// keeps the last workspace-observed empty-directory set; worktree
    /// scans overwrite it with a fresh measurement afterwards.
    pub(crate) fn index_commit(
        &self,
        storage: &crate::storage::SqliteStorage,
        commit_id: &str,
        actor: &str,
        session: &str,
        tool: &str,
        paths: &[String],
    ) -> Result<(), CheckpointError> {
        let timestamp = self
            .git_ref()
            .ok()
            .and_then(|git| git.read_commit(commit_id).ok())
            .map(|commit| commit.committer_ts)
            .unwrap_or_else(|| self.creation_timestamp().unwrap_or(0));
        storage.record_source_index(&crate::storage::SourceIndexEntry {
            commit_id: commit_id.to_string(),
            actor: actor.to_string(),
            session: session.to_string(),
            tool: tool.to_string(),
            paths: paths.to_vec(),
            timestamp,
        })?;
        self.inherit_empty_dirs_manifest(commit_id)?;
        Ok(())
    }

    /// Non-scan commits do not inherit empty-dir manifests. Only worktree
    /// scans observe the true empty set, so inheriting parent manifests here
    /// would resurrect dirs deleted on disk. Scan paths store the observed
    /// set explicitly.
    fn inherit_empty_dirs_manifest(&self, _commit_id: &str) -> Result<(), CheckpointError> {
        Ok(())
    }

    pub(crate) fn publish_file_event(
        &self,
        commit_id: &str,
        path: &str,
        source: &str,
        bytes: Option<&[u8]>,
    ) {
        let Some(bus) = self.event_bus.as_ref() else {
            return;
        };
        let hash = bytes.map(sha256_hex).unwrap_or_else(|| sha256_hex(b""));
        bus.publish(crate::event::CheckpointEventBus::file_changed_with_summary(
            commit_id.to_string(),
            path,
            source,
            Some(DeltaSummary {
                file: path.to_string(),
                source: source.to_string(),
                timestamp: wf_common::now(),
                snapshot_id: commit_id.to_string(),
                hash,
                message: None,
            }),
        ));
    }

    /// Project a commit into the stable `FileCheckpoint` read shape.
    pub(crate) fn project_commit(
        &self,
        commit_id: &str,
    ) -> Result<FileCheckpoint, CheckpointError> {
        if !crate::git_store::is_hex_id(commit_id) {
            return Err(CheckpointError::NotFound {
                id: commit_id.to_string(),
            });
        }
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let commit = git.read_commit(commit_id).map_err(map_git_error)?;
        let files_map = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
        let mut files: Vec<FileState> = files_map
            .iter()
            .map(|(path, bytes)| FileState {
                path: path.clone(),
                hash: sha256_hex(bytes),
                size: bytes.len() as u64,
                last_modified: commit.committer_ts,
                deleted: false,
            })
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let full_hash = compute_full_hash(&files);
        let empty_dirs = storage.load_empty_dirs(commit_id)?;
        Ok(FileCheckpoint {
            id: commit_id.to_string(),
            timestamp: commit.committer_ts,
            full_hash,
            files,
            empty_dirs: if empty_dirs.is_empty() {
                None
            } else {
                Some(empty_dirs)
            },
        })
    }

    /// Periodic human-edit poll: diff the worktree against every tracked
    /// ref and commit the remainder on the human ref. Files matching any
    /// agent content are agent-owned and skipped, with one exception: a
    /// revert to main content syncs the human ref back to main so it never
    /// goes stale. The human ref is never merged automatically. Returns the
    /// new commit id, if any.
    ///
    /// Deletion attribution is three-state: gone from the worktree, present
    /// in the human view and unknown to every agent ref commits a human
    /// delete; gone but still carried by an agent ref is ambiguous (a human
    /// delete of agent content cannot be told apart from agent ownership)
    /// and is skipped loudly with a warning plus a skip event, never
    /// committed; record those through an explicit delete instead.
    pub fn poll_human_edits(
        &self,
        base_dir: &std::path::Path,
    ) -> Result<Option<String>, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut ignore_patterns = self
            .policy
            .scan_config
            .custom_ignore_patterns
            .clone();
        ignore_patterns.extend(crate::scan::storage_exclude_patterns(
            base_dir,
            storage.db_path().as_deref(),
        ));
        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: ignore_patterns,
            failure_behavior: self.policy.scan_config.failure_behavior,
        });
        let scan = scanner.scan(base_dir)?;
        let mut worktree: HashMap<String, Vec<u8>> = HashMap::with_capacity(scan.files.len());
        for file in &scan.files {
            let path = base_dir.join(&file.path);
            match std::fs::read(&path) {
                Ok(bytes) => {
                    worktree.insert(file.path.clone(), bytes);
                }
                Err(err) => {
                    crate::file::util::handle_restore_failure(
                        self.policy.scan_config.failure_behavior,
                        &file.path,
                        &err,
                    )?;
                }
            }
        }
        // Agent-owned content: union of main plus every edit-ref head tree.
        // Deterministic rule: a worktree file matching any tracked agent
        // content is agent-owned and skipped; otherwise it is human.
        // All known bytes per path are retained so main versus edit
        // divergence never misclassifies an agent file as human.
        let mut owned: HashMap<String, std::collections::HashSet<Vec<u8>>> = HashMap::new();
        let mut ref_names = vec![crate::git_store::REF_MAIN.to_string()];
        for (name, _) in git
            .list_refs(crate::git_store::REF_EDIT_PREFIX)
            .map_err(map_git_error)?
        {
            ref_names.push(name);
        }
        ref_names.sort();
        for name in ref_names {
            let Some(head) = git.read_ref(&name).map_err(map_git_error)? else {
                continue;
            };
            let Ok(commit) = git.read_commit(&head) else {
                continue;
            };
            if let Ok(files) = git.tree_to_bytes(&commit.tree) {
                for (path, bytes) in files {
                    owned.entry(path).or_default().insert(bytes);
                }
            }
        }
        let human_head = git.read_ref(REF_HUMAN).map_err(map_git_error)?;
        let human_tree: HashMap<String, Vec<u8>> = match &human_head {
            Some(id) => git
                .read_commit(id)
                .map_err(map_git_error)
                .and_then(|c| git.tree_to_bytes(&c.tree).map_err(map_git_error))
                .unwrap_or_default(),
            None => HashMap::new(),
        };
        let main_tree: HashMap<String, Vec<u8>> = match git
            .read_ref(crate::git_store::REF_MAIN)
            .map_err(map_git_error)?
        {
            Some(id) => git
                .read_commit(&id)
                .map_err(map_git_error)
                .and_then(|c| git.tree_to_bytes(&c.tree).map_err(map_git_error))
                .unwrap_or_default(),
            None => HashMap::new(),
        };
        let mut changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
        for (path, bytes) in &worktree {
            if owned.get(path).is_some_and(|known| known.contains(bytes)) {
                // Revert sync: the worktree matches main exactly while the
                // human ref still carries an older edit. Record the sync so
                // the human ref follows the revert instead of going stale.
                let matches_main = main_tree.get(path).is_some_and(|m| m == bytes);
                let human_differs = human_tree.get(path).is_some_and(|h| h != bytes);
                if matches_main && human_differs {
                    changes.insert(path.clone(), Some((MODE_FILE.to_string(), bytes.clone())));
                }
                continue;
            }
            if human_tree.get(path).is_some_and(|known| known == bytes) {
                continue;
            }
            changes.insert(path.clone(), Some((MODE_FILE.to_string(), bytes.clone())));
        }
        // Human deletions: present in the human-tracked view but gone from
        // the worktree, and not explained by any agent content. Paths the
        // agents still carry are ambiguous and stay uncommitted (see the
        // method docs): they are reported loudly so the gap stays
        // observable instead of silently diverging.
        let mut tracked: HashMap<String, Vec<u8>> = human_tree.clone();
        for path in owned.keys() {
            tracked.entry(path.clone()).or_default();
        }
        let mut ambiguous_deletes: Vec<String> = Vec::new();
        for path in tracked.keys() {
            if worktree.contains_key(path) {
                continue;
            }
            if !owned.contains_key(path) {
                changes.insert(path.clone(), None);
            } else if human_tree.contains_key(path) {
                ambiguous_deletes.push(path.clone());
            }
        }
        if !ambiguous_deletes.is_empty() {
            ambiguous_deletes.sort();
            tracing::warn!(
                paths = ?ambiguous_deletes,
                "human poll skipped ambiguous deletes of agent-owned paths; use an explicit delete to record them"
            );
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::skipped(
                    "human-attribution",
                    format!(
                        "skipped ambiguous deletes of agent-owned paths: {}",
                        ambiguous_deletes.join(", ")
                    ),
                    None,
                ));
            }
        }
        if changes.is_empty() {
            return Ok(None);
        }
        let message = commit_message("human edit poll", Some("human"), None, Some("watcher"), &[]);
        let outcome = git
            .commit_on_ref(REF_HUMAN, &changes, "human", &message)
            .map_err(map_git_error)?;
        if !outcome.created {
            return Ok(None);
        }
        let mut paths: Vec<String> = changes.keys().cloned().collect();
        paths.sort();
        self.index_commit(storage, &outcome.id, "human", "", "watcher", &paths)?;
        // The poll just scanned the worktree: its empty-directory set is a
        // fresh measurement and replaces the inherited manifest.
        storage.store_empty_dirs(&outcome.id, &scan.empty_dirs)?;
        for (path, change) in &changes {
            let bytes = change.as_ref().map(|(_, content)| content.as_slice());
            self.publish_file_event(&outcome.id, path, "human", bytes);
        }
        Ok(Some(outcome.id))
    }
}
