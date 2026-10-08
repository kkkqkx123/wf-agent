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
        let timestamp = self.creation_timestamp().unwrap_or_else(|_| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        });
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

    /// Carry the union of the parents' empty-dir manifests onto a new
    /// commit. Only worktree scans observe the true empty-directory set,
    /// so non-scan commits preserve rather than invent it.
    fn inherit_empty_dirs_manifest(&self, commit_id: &str) -> Result<(), CheckpointError> {
        let git = self.git_ref()?;
        let commit = git.read_commit(commit_id).map_err(map_git_error)?;
        if commit.parents.is_empty() {
            return Ok(());
        }
        let storage = self.storage_ref()?;
        let mut merged = std::collections::BTreeSet::new();
        for parent in &commit.parents {
            for dir in storage.load_empty_dirs(parent)? {
                merged.insert(dir);
            }
        }
        if !merged.is_empty() {
            let dirs: Vec<String> = merged.into_iter().collect();
            storage.store_empty_dirs(commit_id, &dirs)?;
        }
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
    /// agent content are agent-owned and skipped; the human ref is never
    /// merged automatically. Returns the new commit id, if any.
    pub fn poll_human_edits(
        &self,
        base_dir: &std::path::Path,
    ) -> Result<Option<String>, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: self.policy.scan_config.custom_ignore_patterns.clone(),
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
        let mut owned: HashMap<String, Vec<u8>> = HashMap::new();
        let mut ref_names = vec![crate::git_store::REF_MAIN.to_string()];
        for (name, _) in git
            .list_refs(crate::git_store::REF_EDIT_PREFIX)
            .map_err(map_git_error)?
        {
            ref_names.push(name);
        }
        for name in ref_names {
            let Some(head) = git.read_ref(&name).map_err(map_git_error)? else {
                continue;
            };
            let Ok(commit) = git.read_commit(&head) else {
                continue;
            };
            if let Ok(files) = git.tree_to_bytes(&commit.tree) {
                for (path, bytes) in files {
                    owned.entry(path).or_insert(bytes);
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
        let mut changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
        for (path, bytes) in &worktree {
            if owned.get(path).is_some_and(|known| known == bytes) {
                continue;
            }
            if human_tree.get(path).is_some_and(|known| known == bytes) {
                continue;
            }
            changes.insert(path.clone(), Some((MODE_FILE.to_string(), bytes.clone())));
        }
        // Human deletions: present in the human-tracked view but gone from
        // the worktree, and not explained by an agent deletion.
        let mut tracked: HashMap<String, Vec<u8>> = human_tree.clone();
        for (path, bytes) in &owned {
            tracked.entry(path.clone()).or_insert_with(|| bytes.clone());
        }
        for path in tracked.keys() {
            if !worktree.contains_key(path) && !owned.contains_key(path) {
                changes.insert(path.clone(), None);
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
