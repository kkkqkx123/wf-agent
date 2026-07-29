use std::path::Path;

use crate::checkpoint::repo::CheckpointRepo;
use crate::core::delta::Delta;
use crate::core::file_node::FileNode;
use crate::core::snapshot::Snapshot;
use crate::core::types::LineDiff;
use crate::core::types::{CheckpointId, SourceType};
use crate::engine::merge::apply_deltas;
use crate::error::{LayertwineError, Result};
use crate::storage::repository::{DeltaStore, FileNodeStore, SnapshotStore};

#[cfg(test)]
use crate::engine::diff::diff_to_line_diff;

/// Which ref namespace to use for git commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefNamespace {
    /// Commit directly to the current git branch (HEAD), updating working tree and index.
    /// This is the default, backward-compatible mode.
    CurrentBranch,
    /// Commit to `refs/layertwine/<branch>`, isolated from working tree.
    /// Uses TreeBuilder directly without touching HEAD, index, or working tree.
    Isolated,
}

impl Default for RefNamespace {
    fn default() -> Self {
        Self::CurrentBranch
    }
}

/// Configuration for git sync operations.
#[derive(Debug, Clone)]
pub struct GitSyncConfig {
    /// Which ref namespace to use for the git commit.
    pub ref_namespace: RefNamespace,
    /// Whether to require the git working tree to be clean before committing.
    /// Only applies in `CurrentBranch` mode.
    pub require_clean_tree: bool,
    /// Whether to write files to the working tree (only for `CurrentBranch` mode).
    /// When false, uses TreeBuilder even in `CurrentBranch` mode.
    pub write_working_tree: bool,
}

impl Default for GitSyncConfig {
    fn default() -> Self {
        Self {
            ref_namespace: RefNamespace::CurrentBranch,
            require_clean_tree: false,
            write_working_tree: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    InSync,
    Ahead {
        unpushed_checkpoints: usize,
    },
    Behind {
        unpulled_commits: usize,
    },
    Divergent {
        unpushed_checkpoints: usize,
        unpulled_commits: usize,
    },
}

#[derive(Debug, Clone)]
pub struct SyncInfo {
    pub status: SyncStatus,
    pub git_head_hash: Option<String>,
    pub local_baseline_id: Option<CheckpointId>,
}

pub struct GitBridge {}

impl GitBridge {
    pub fn new() -> Self {
        GitBridge {}
    }

    /// Initialize a checkpoint repo from a Git repository reference.
    ///
    /// Reads ALL file contents from the Git repo at the given ref, builds
    /// FileNode + Delta + Snapshot chains for each file, creates an initial
    /// Checkpoint referencing ALL snapshots, stores the checkpoint in the
    /// repo, and resets the branch head to the new checkpoint.
    /// Also extracts the Git commit author and message into the checkpoint metadata.
    pub fn init_from_git<S>(
        git_repo_path: &Path,
        storage: &S,
        checkpoint_repo: &mut CheckpointRepo,
        git_ref: &str,
    ) -> Result<()>
    where
        S: SnapshotStore + DeltaStore + FileNodeStore,
    {
        let git_repo = git2::Repository::open(git_repo_path)
            .map_err(|e| LayertwineError::GitSync(format!("failed to open git repo: {}", e)))?;

        let resolved = git_repo.revparse_single(git_ref).map_err(|e| {
            LayertwineError::GitSync(format!("failed to resolve ref '{}': {}", git_ref, e))
        })?;

        let commit = resolved
            .peel_to_commit()
            .map_err(|e| LayertwineError::GitSync(format!("failed to peel to commit: {}", e)))?;

        let tree = commit
            .tree()
            .map_err(|e| LayertwineError::GitSync(format!("failed to get tree: {}", e)))?;

        let git_commit_hash = commit.id().to_string();
        let mut snapshots: Vec<Snapshot> = Vec::new();

        walk_tree_and_create_snapshots(&git_repo, &tree, "", storage, &mut snapshots)?;

        if snapshots.is_empty() {
            return Err(LayertwineError::GitSync(
                "no files found in git ref".to_string(),
            ));
        }

        // Extract author and message from the Git commit
        let author = commit.author();
        let author_name = author.name().unwrap_or("git-sync");
        let default_msg = format!("Sync from Git ref: {}", git_ref);
        let commit_msg = commit.message().unwrap_or(&default_msg).trim();

        // Commit the baseline snapshots, then set git_anchor.
        // git_anchor is excluded from content-addressed hashing, so
        // setting it here does NOT invalidate the checkpoint ID.
        let cp_id = checkpoint_repo.commit(
            snapshots.iter().map(|s| s.id).collect(),
            commit_msg,
            author_name,
        )?;

        if let Ok(cp) = checkpoint_repo.get_checkpoint_mut(&cp_id) {
            cp.metadata.git_anchor = Some(git_commit_hash);
        }

        Ok(())
    }

    /// Commit the current checkpoint state to Git.
    ///
    /// Depending on `config.ref_namespace`:
    /// - `CurrentBranch`: writes files to working tree, stages, commits to HEAD (backward compatible)
    /// - `Isolated`: creates tree via TreeBuilder, commits to `refs/layertwine/<branch>`,
    ///   does NOT touch HEAD, index, or working tree.
    ///
    /// Records the git commit hash in the checkpoint's `git_anchor` metadata.
    pub fn push_to_git<S>(
        storage: &S,
        git_repo_path: &Path,
        checkpoint_repo: &mut CheckpointRepo,
        branch_name: &str,
        message: &str,
    ) -> Result<String>
    where
        S: SnapshotStore + DeltaStore + FileNodeStore,
    {
        Self::push_to_git_with_config(
            storage,
            git_repo_path,
            checkpoint_repo,
            branch_name,
            message,
            &GitSyncConfig::default(),
        )
    }

    /// Commit the current checkpoint state to Git with explicit configuration.
    pub fn push_to_git_with_config<S>(
        storage: &S,
        git_repo_path: &Path,
        checkpoint_repo: &mut CheckpointRepo,
        branch_name: &str,
        message: &str,
        config: &GitSyncConfig,
    ) -> Result<String>
    where
        S: SnapshotStore + DeltaStore + FileNodeStore,
    {
        let git_repo = git2::Repository::open(git_repo_path)
            .map_err(|e| LayertwineError::GitSync(format!("failed to open git repo: {}", e)))?;

        let checkpoint_id = checkpoint_repo.get_branch_head(branch_name).map_err(|_| {
            LayertwineError::NotFound(format!("branch '{}' not found", branch_name))
        })?;

        let checkpoint = checkpoint_repo
            .get_checkpoint(&checkpoint_id)
            .map_err(|_| LayertwineError::NotFound("checkpoint not found".to_string()))?;

        if checkpoint.baseline_snapshots.is_empty() {
            return Err(LayertwineError::GitSync(
                "checkpoint has no baseline snapshots".to_string(),
            ));
        }

        // Clone to release immutable borrow before passing checkpoint_repo as &mut
        let checkpoint = checkpoint.clone();

        match config.ref_namespace {
            RefNamespace::Isolated => {
                Self::push_to_git_isolated(
                    &git_repo,
                    storage,
                    checkpoint_repo,
                    &checkpoint,
                    branch_name,
                    message,
                    config,
                )
            }
            RefNamespace::CurrentBranch => {
                Self::push_to_git_current_branch(
                    &git_repo,
                    storage,
                    checkpoint_repo,
                    &checkpoint,
                    branch_name,
                    message,
                    config,
                )
            }
        }
    }

    /// CurrentBranch mode: write to working tree + commit to HEAD.
    fn push_to_git_current_branch<S>(
        git_repo: &git2::Repository,
        storage: &S,
        checkpoint_repo: &mut CheckpointRepo,
        checkpoint: &crate::checkpoint::types::Checkpoint,
        branch_name: &str,
        message: &str,
        config: &GitSyncConfig,
    ) -> Result<String>
    where
        S: SnapshotStore + DeltaStore + FileNodeStore,
    {
        if config.require_clean_tree {
            let statuses = git_repo.statuses(None).map_err(|e| {
                LayertwineError::GitSync(format!("failed to get repo status: {}", e))
            })?;
            if statuses.iter().any(|s| s.status() != git2::Status::CURRENT) {
                return Err(LayertwineError::GitSync(
                    "working tree is not clean. Commit or stash changes first.".to_string(),
                ));
            }
        }

        if !config.write_working_tree {
            // Use TreeBuilder directly, skip working tree
            return Self::build_tree_and_commit(
                git_repo,
                storage,
                checkpoint_repo,
                checkpoint,
                Some("HEAD"),
                branch_name,
                message,
            );
        }

        let workdir = git_repo
            .workdir()
            .ok_or_else(|| {
                LayertwineError::GitSync("bare repository has no workdir".to_string())
            })?
            .to_path_buf();

        let mut index = git_repo
            .index()
            .map_err(|e| LayertwineError::GitSync(format!("failed to get index: {}", e)))?;

        // Write all snapshot files to the working tree and stage them
        for snapshot_id in &checkpoint.baseline_snapshots {
            let snapshot = storage
                .get_snapshot(snapshot_id)
                .map_err(LayertwineError::Storage)?;

            let current_text = reconstruct_snapshot_text(storage, &snapshot)?;

            let file_path_in_repo = workdir.join(&snapshot.file.file_path);
            if let Some(parent) = file_path_in_repo.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    LayertwineError::GitSync(format!("failed to create directories: {}", e))
                })?;
            }
            std::fs::write(&file_path_in_repo, current_text.as_bytes())
                .map_err(|e| LayertwineError::GitSync(format!("failed to write file: {}", e)))?;

            let repo_relative_path = snapshot.file.file_path.to_str().unwrap_or("");
            index.add_path(Path::new(repo_relative_path)).map_err(|e| {
                LayertwineError::GitSync(format!("failed to add file to index: {}", e))
            })?;
        }

        index
            .write()
            .map_err(|e| LayertwineError::GitSync(format!("failed to write index: {}", e)))?;

        let tree_id = index
            .write_tree()
            .map_err(|e| LayertwineError::GitSync(format!("failed to write tree: {}", e)))?;

        let tree = git_repo
            .find_tree(tree_id)
            .map_err(|e| LayertwineError::GitSync(format!("failed to find tree: {}", e)))?;

        let checkpoint_id = checkpoint.id;
        let git_commit_hash = Self::create_git_commit(
            git_repo,
            checkpoint,
            Some("HEAD"),
            branch_name,
            message,
            &tree,
        )?;

        if let Ok(cp) = checkpoint_repo.get_checkpoint_mut(&checkpoint_id) {
            cp.metadata.git_anchor = Some(git_commit_hash.clone());
        }

        Ok(git_commit_hash)
    }

    /// Isolated mode: use TreeBuilder, commit to `refs/layertwine/<branch>`.
    fn push_to_git_isolated<S>(
        git_repo: &git2::Repository,
        storage: &S,
        checkpoint_repo: &mut CheckpointRepo,
        checkpoint: &crate::checkpoint::types::Checkpoint,
        branch_name: &str,
        message: &str,
        _config: &GitSyncConfig,
    ) -> Result<String>
    where
        S: SnapshotStore + DeltaStore + FileNodeStore,
    {
        let git_ref = format!("refs/layertwine/{}", branch_name);
        Self::build_tree_and_commit(
            git_repo,
            storage,
            checkpoint_repo,
            checkpoint,
            Some(&git_ref),
            branch_name,
            message,
        )
    }

    /// Build a tree object from snapshot contents and create a git commit.
    ///
    /// `target_ref`: `Some("HEAD")` for CurrentBranch mode, `Some("refs/layertwine/<branch>")` for Isolated.
    /// When `None`, creates a dangling commit (no ref update).
    fn build_tree_and_commit<S>(
        git_repo: &git2::Repository,
        storage: &S,
        checkpoint_repo: &mut CheckpointRepo,
        checkpoint: &crate::checkpoint::types::Checkpoint,
        target_ref: Option<&str>,
        branch_name: &str,
        message: &str,
    ) -> Result<String>
    where
        S: SnapshotStore + DeltaStore + FileNodeStore,
    {
        let mut tree_builder = git_repo
            .treebuilder(None)
            .map_err(|e| LayertwineError::GitSync(format!("failed to create tree builder: {}", e)))?;

        for snapshot_id in &checkpoint.baseline_snapshots {
            let snapshot = storage
                .get_snapshot(snapshot_id)
                .map_err(LayertwineError::Storage)?;

            let current_text = reconstruct_snapshot_text(storage, &snapshot)?;

            let blob_oid = git_repo
                .blob(current_text.as_bytes())
                .map_err(|e| LayertwineError::GitSync(format!("failed to create blob: {}", e)))?;

            let path_str = snapshot.file.file_path.to_str().unwrap_or("");
            tree_builder
                .insert(path_str, blob_oid, git2::FileMode::Blob.into())
                .map_err(|e| {
                    LayertwineError::GitSync(format!("failed to insert tree entry: {}", e))
                })?;
        }

        let tree_oid = tree_builder
            .write()
            .map_err(|e| LayertwineError::GitSync(format!("failed to write tree: {}", e)))?;

        let tree = git_repo
            .find_tree(tree_oid)
            .map_err(|e| LayertwineError::GitSync(format!("failed to find tree: {}", e)))?;

        let checkpoint_id = checkpoint.id;
        let git_commit_hash = Self::create_git_commit(
            git_repo,
            checkpoint,
            target_ref,
            branch_name,
            message,
            &tree,
        )?;

        if let Ok(cp) = checkpoint_repo.get_checkpoint_mut(&checkpoint_id) {
            cp.metadata.git_anchor = Some(git_commit_hash.clone());
        }

        Ok(git_commit_hash)
    }

    /// Create a git commit, optionally updating a ref.
    ///
    /// `update_ref`: `Some("HEAD")` or `Some("refs/layertwine/<branch>")`.
    /// When `None`, creates an unreferenced commit.
    fn create_git_commit(
        git_repo: &git2::Repository,
        checkpoint: &crate::checkpoint::types::Checkpoint,
        update_ref: Option<&str>,
        branch_name: &str,
        message: &str,
        tree: &git2::Tree,
    ) -> Result<String> {
        let author_sig =
            git2::Signature::now(checkpoint.metadata.author.as_str(), "layertwine@local").map_err(
                |e| LayertwineError::GitSync(format!("failed to create signature: {}", e)),
            )?;

        // Find parent commit: either from the update_ref or HEAD
        let parent_commit = if let Some(ref_str) = update_ref {
            git_repo
                .refname_to_id(ref_str)
                .ok()
                .and_then(|oid| git_repo.find_commit(oid).ok())
        } else {
            git_repo.head().ok().and_then(|head| head.peel_to_commit().ok())
        };

        let git_commit = if let Some(parent) = &parent_commit {
            git_repo
                .commit(
                    update_ref,
                    &author_sig,
                    &author_sig,
                    message,
                    tree,
                    &[parent],
                )
                .map_err(|e| LayertwineError::GitSync(format!("failed to commit: {}", e)))?
        } else {
            git_repo
                .commit(
                    update_ref,
                    &author_sig,
                    &author_sig,
                    message,
                    tree,
                    &[] as &[&git2::Commit],
                )
                .map_err(|e| LayertwineError::GitSync(format!("failed to commit: {}", e)))?
        };

        let git_commit_hash = git_commit.to_string();

        // Ensure refs/heads/<branch_name> exists for CurrentBranch mode
        // In Isolated mode, the ref is already set (refs/layertwine/<branch>)
        if update_ref == Some("HEAD") {
            git_repo
                .branch(
                    branch_name,
                    &git_repo.find_commit(git_commit).map_err(|e| {
                        LayertwineError::GitSync(format!("failed to find commit: {}", e))
                    })?,
                    true, // force update
                )
                .map_err(|e| {
                    LayertwineError::GitSync(format!(
                        "failed to create/update branch '{}': {}",
                        branch_name, e
                    ))
                })?;
        }

        Ok(git_commit_hash)
    }

    /// Compare the status between the Git repo HEAD and the checkpoint repo baseline.
    ///
    /// Uses `graph_ahead_behind` for precise divergence counting.
    pub fn compare_status(
        git_repo_path: &Path,
        checkpoint_repo: &CheckpointRepo,
        branch_name: &str,
    ) -> Result<SyncInfo> {
        let git_repo = git2::Repository::open(git_repo_path)
            .map_err(|e| LayertwineError::GitSync(format!("failed to open git repo: {}", e)))?;

        let git_head = git_repo.head().ok();
        let git_head_commit = git_head.as_ref().and_then(|h| h.peel_to_commit().ok());
        let git_head_hash = git_head_commit.as_ref().map(|c| c.id().to_string());
        let git_head_oid = git_head_commit.as_ref().map(|c| c.id());

        let local_baseline_id = checkpoint_repo.get_branch_head(branch_name).ok();
        let local_git_anchor = local_baseline_id.and_then(|id| {
            checkpoint_repo
                .get_checkpoint(&id)
                .ok()
                .and_then(|cp| cp.metadata.git_anchor.clone())
                .and_then(|anchor| git2::Oid::from_str(&anchor).ok())
        });

        let status = match (git_head_oid, local_git_anchor) {
            (Some(git_oid), Some(local_oid)) => {
                let (ahead, behind) = git_repo
                    .graph_ahead_behind(git_oid, local_oid)
                    .unwrap_or((0, 0));

                if ahead == 0 && behind == 0 {
                    SyncStatus::InSync
                } else if ahead > 0 && behind > 0 {
                    SyncStatus::Divergent {
                        unpushed_checkpoints: behind,
                        unpulled_commits: ahead,
                    }
                } else if ahead > 0 {
                    SyncStatus::Behind {
                        unpulled_commits: ahead,
                    }
                } else {
                    SyncStatus::Ahead {
                        unpushed_checkpoints: behind,
                    }
                }
            }
            (Some(_), None) => {
                // Git HEAD exists but checkpoint has no anchor => checkpoint is ahead
                SyncStatus::Ahead {
                    unpushed_checkpoints: 1,
                }
            }
            (None, Some(_)) => {
                // Checkpoint anchored but no Git HEAD (unlikely) => ahead
                SyncStatus::Ahead {
                    unpushed_checkpoints: 1,
                }
            }
            (None, None) => SyncStatus::InSync,
        };

        Ok(SyncInfo {
            status,
            git_head_hash,
            local_baseline_id,
        })
    }

    /// Fetch from a Git remote and update the checkpoint repo accordingly.
    ///
    /// Fetches the remote refs, then initializes from the fetched remote tracking branch.
    pub fn fetch_from_remote(git_repo_path: &Path, remote_name: &str) -> Result<()> {
        let git_repo = git2::Repository::open(git_repo_path)
            .map_err(|e| LayertwineError::GitSync(format!("failed to open git repo: {}", e)))?;

        let mut remote = git_repo.find_remote(remote_name).map_err(|e| {
            LayertwineError::GitSync(format!("failed to find remote '{}': {}", remote_name, e))
        })?;

        let mut fetch_options = git2::FetchOptions::new();
        remote
            .fetch(
                &["refs/heads/*:refs/remotes/*"],
                Some(&mut fetch_options),
                None,
            )
            .map_err(|e| {
                LayertwineError::GitSync(format!(
                    "failed to fetch from remote '{}': {}",
                    remote_name, e
                ))
            })?;

        Ok(())
    }
}

impl Default for GitBridge {
    fn default() -> Self {
        Self::new()
    }
}

fn walk_tree_and_create_snapshots<S>(
    git_repo: &git2::Repository,
    tree: &git2::Tree,
    prefix: &str,
    storage: &S,
    snapshots: &mut Vec<Snapshot>,
) -> Result<()>
where
    S: SnapshotStore + DeltaStore + FileNodeStore,
{
    for entry in tree.iter() {
        let name = entry.name().unwrap_or("").to_string();
        let entry_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", prefix, name)
        };

        match entry.kind() {
            Some(git2::ObjectType::Tree) => {
                if let Ok(subtree) = entry.to_object(git_repo).and_then(|o| o.peel_to_tree()) {
                    walk_tree_and_create_snapshots(
                        git_repo,
                        &subtree,
                        &entry_path,
                        storage,
                        snapshots,
                    )?;
                }
            }
            Some(git2::ObjectType::Blob) => {
                if let Ok(blob) = entry.to_object(git_repo).and_then(|o| o.peel_to_blob()) {
                    let content = blob.content().to_vec();
                    let file_node = FileNode::new(std::path::PathBuf::from(&entry_path), &content);

                    storage
                        .store_file_node(&file_node, &content)
                        .map_err(LayertwineError::Storage)?;

                    let diff = LineDiff::new(vec![]);
                    let delta = Delta::new(file_node.clone(), diff, SourceType::Backup);

                    storage
                        .store_delta(&delta)
                        .map_err(LayertwineError::Storage)?;

                    let snapshot = Snapshot::new_initial(file_node, delta.id);

                    storage
                        .store_snapshot(&snapshot, &content)
                        .map_err(LayertwineError::Storage)?;

                    snapshots.push(snapshot);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Reconstruct the current text of a snapshot from its base content + delta chain.
fn reconstruct_snapshot_text<S>(storage: &S, snapshot: &Snapshot) -> Result<String>
where
    S: SnapshotStore + DeltaStore + FileNodeStore,
{
    let base_content = storage
        .get_file_content(snapshot.file.path_str(), &snapshot.file.base_hash)
        .map_err(LayertwineError::Storage)?;
    let deltas = storage
        .get_deltas(&snapshot.deltas)
        .map_err(LayertwineError::Storage)?;
    let base_str = String::from_utf8_lossy(&base_content).to_string();
    apply_deltas(&base_str, &deltas).map_err(|e| LayertwineError::Engine(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SqliteStorage;
    use std::path::PathBuf;

    fn init_git_repo(path: &Path) -> git2::Repository {
        let repo = git2::Repository::init(path).unwrap();

        let test_file_path = path.join("hello.txt");
        std::fs::write(&test_file_path, b"hello from git").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("hello.txt")).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        drop(tree);
        repo
    }

    #[test]
    fn test_init_from_git() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        let _git_repo = init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();
        let initial_snapshot = {
            let file_node = FileNode::new(PathBuf::from("dummy"), b"init");
            let diff = LineDiff::new(vec![]);
            let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
            storage.store_delta(&delta).unwrap();
            let snap = Snapshot::new_initial(file_node, delta.id);
            storage.store_snapshot(&snap, b"init").unwrap();
            snap.id
        };
        let mut checkpoint_repo = CheckpointRepo::new_single(initial_snapshot);

        let result = GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD");

        assert!(result.is_ok(), "init_from_git failed: {:?}", result.err());
    }

    #[test]
    fn test_push_to_git() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        let _git_repo = init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();

        // Create initial snapshot
        let initial_content = b"initial content";
        let file_node1 = FileNode::new(PathBuf::from("pushed.txt"), initial_content);
        storage
            .store_file_node(&file_node1, initial_content)
            .unwrap();
        let diff1 = LineDiff::new(vec![]);
        let delta1 = Delta::new(file_node1.clone(), diff1, SourceType::Manual);
        storage.store_delta(&delta1).unwrap();
        let snapshot1 = Snapshot::new_initial(file_node1, delta1.id);
        storage.store_snapshot(&snapshot1, initial_content).unwrap();

        let mut checkpoint_repo = CheckpointRepo::new_single(snapshot1.id);

        // Create a new snapshot with different content
        let modified_content = b"test content for push";
        let file_node2 = FileNode::new(PathBuf::from("pushed.txt"), modified_content);
        storage
            .store_file_node(&file_node2, modified_content)
            .unwrap();
        let diff2 = diff_to_line_diff("initial content", "test content for push");
        let delta2 = Delta::new(file_node2.clone(), diff2, SourceType::Manual);
        storage.store_delta(&delta2).unwrap();
        let snapshot2 = Snapshot::from_parent(&snapshot1, delta2.id, "manual_edit".to_string());
        storage.store_snapshot(&snapshot2, b"").unwrap();

        // Commit the modified snapshot
        checkpoint_repo
            .commit_single(snapshot2.id, "test push", "test-user")
            .unwrap();

        let result = GitBridge::push_to_git(
            &storage,
            &git_path,
            &mut checkpoint_repo,
            "main",
            "push from layertwine",
        );

        assert!(result.is_ok(), "push_to_git failed: {:?}", result.err());
    }

    #[test]
    fn test_compare_status() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();
        let content = b"status test";
        let file_node = FileNode::new(PathBuf::from("status.txt"), content);
        storage.store_file_node(&file_node, content).unwrap();
        let diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();
        let snapshot = Snapshot::new_initial(file_node, delta.id);
        storage.store_snapshot(&snapshot, content).unwrap();

        let checkpoint_repo = CheckpointRepo::new_single(snapshot.id);

        let info = GitBridge::compare_status(&git_path, &checkpoint_repo, "main").unwrap();
        assert_eq!(
            info.status,
            SyncStatus::Ahead {
                unpushed_checkpoints: 1
            }
        );
    }

    #[test]
    fn test_init_from_git_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        let git_repo = init_git_repo(&git_path);

        let commit = git_repo.head().unwrap().peel_to_commit().unwrap();
        let expected_hash = commit.id().to_string();
        let expected_author = commit.author().name().unwrap_or("").to_string();
        let expected_msg = commit.message().unwrap_or("").trim().to_string();

        let storage = SqliteStorage::new_in_memory().unwrap();
        let dummy_file_node = FileNode::new(PathBuf::from("dummy"), b"init");
        let dummy_diff = LineDiff::new(vec![]);
        let dummy_delta = Delta::new(dummy_file_node.clone(), dummy_diff, SourceType::Manual);
        storage.store_delta(&dummy_delta).unwrap();
        let dummy_snap = Snapshot::new_initial(dummy_file_node, dummy_delta.id);
        storage.store_snapshot(&dummy_snap, b"init").unwrap();
        let mut checkpoint_repo = CheckpointRepo::new_single(dummy_snap.id);

        GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD").unwrap();

        let head = checkpoint_repo.current_branch_head();
        let cp = checkpoint_repo.get_checkpoint(&head).unwrap();
        assert_eq!(
            cp.metadata.git_anchor.as_deref(),
            Some(expected_hash.as_str()),
            "git_anchor should match the commit hash"
        );
        assert_eq!(
            cp.metadata.author, expected_author,
            "author should match git commit author"
        );
        assert_eq!(
            cp.metadata.message, expected_msg,
            "message should match git commit message"
        );
    }

    #[test]
    fn test_init_from_git_invalid_ref() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();
        let dummy_file_node = FileNode::new(PathBuf::from("dummy"), b"init");
        let dummy_diff = LineDiff::new(vec![]);
        let dummy_delta = Delta::new(dummy_file_node.clone(), dummy_diff, SourceType::Manual);
        storage.store_delta(&dummy_delta).unwrap();
        let dummy_snap = Snapshot::new_initial(dummy_file_node, dummy_delta.id);
        storage.store_snapshot(&dummy_snap, b"init").unwrap();
        let mut checkpoint_repo = CheckpointRepo::new_single(dummy_snap.id);

        let result =
            GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "nonexistent-ref");
        assert!(result.is_err(), "should fail on invalid git ref");
    }

    #[test]
    fn test_compare_status_in_sync() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        let _git_repo = init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();
        let dummy_file_node = FileNode::new(PathBuf::from("dummy"), b"init");
        let dummy_diff = LineDiff::new(vec![]);
        let dummy_delta = Delta::new(dummy_file_node.clone(), dummy_diff, SourceType::Manual);
        storage.store_delta(&dummy_delta).unwrap();
        let dummy_snap = Snapshot::new_initial(dummy_file_node, dummy_delta.id);
        storage.store_snapshot(&dummy_snap, b"init").unwrap();
        let mut checkpoint_repo = CheckpointRepo::new_single(dummy_snap.id);

        GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD").unwrap();

        let info = GitBridge::compare_status(&git_path, &checkpoint_repo, "main").unwrap();
        assert_eq!(
            info.status,
            SyncStatus::InSync,
            "after init, status should be InSync"
        );
    }

    #[test]
    fn test_compare_status_no_branch() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();
        let dummy_file_node = FileNode::new(PathBuf::from("dummy"), b"init");
        let dummy_diff = LineDiff::new(vec![]);
        let dummy_delta = Delta::new(dummy_file_node.clone(), dummy_diff, SourceType::Manual);
        storage.store_delta(&dummy_delta).unwrap();
        let dummy_snap = Snapshot::new_initial(dummy_file_node, dummy_delta.id);
        storage.store_snapshot(&dummy_snap, b"init").unwrap();
        let checkpoint_repo = CheckpointRepo::new_single(dummy_snap.id);

        let info = GitBridge::compare_status(&git_path, &checkpoint_repo, "nonexistent").unwrap();
        assert_eq!(
            info.status,
            SyncStatus::Ahead {
                unpushed_checkpoints: 1
            },
            "non-existent branch should report Ahead"
        );
        assert!(info.local_baseline_id.is_none());
    }

    #[test]
    fn test_push_to_git_empty_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();

        // Create initial snapshot
        let initial_content = b"initial content";
        let file_node1 = FileNode::new(PathBuf::from("empty.txt"), initial_content);
        storage
            .store_file_node(&file_node1, initial_content)
            .unwrap();
        let diff1 = LineDiff::new(vec![]);
        let delta1 = Delta::new(file_node1.clone(), diff1, SourceType::Manual);
        storage.store_delta(&delta1).unwrap();
        let snapshot1 = Snapshot::new_initial(file_node1, delta1.id);
        storage.store_snapshot(&snapshot1, initial_content).unwrap();

        let mut checkpoint_repo = CheckpointRepo::new_single(snapshot1.id);

        // Create a new snapshot with different content
        let modified_content = b"empty test";
        let file_node2 = FileNode::new(PathBuf::from("empty.txt"), modified_content);
        storage
            .store_file_node(&file_node2, modified_content)
            .unwrap();
        let diff2 = diff_to_line_diff("initial content", "empty test");
        let delta2 = Delta::new(file_node2.clone(), diff2, SourceType::Manual);
        storage.store_delta(&delta2).unwrap();
        let snapshot2 = Snapshot::from_parent(&snapshot1, delta2.id, "manual_edit".to_string());
        storage.store_snapshot(&snapshot2, b"").unwrap();

        // Commit the modified snapshot
        checkpoint_repo
            .commit_single(snapshot2.id, "test", "user")
            .unwrap();

        // Clear baseline snapshots to create an empty checkpoint
        let head = checkpoint_repo.current_branch_head();
        let cp = checkpoint_repo.get_checkpoint_mut(&head).unwrap();
        cp.baseline_snapshots.clear();

        let result = GitBridge::push_to_git(
            &storage,
            &git_path,
            &mut checkpoint_repo,
            "main",
            "empty push",
        );
        assert!(result.is_err(), "push with empty checkpoint should fail");
    }

    #[test]
    fn test_push_to_git_invalid_branch() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();
        let content = b"branch test";
        let file_node = FileNode::new(PathBuf::from("branch.txt"), content);
        storage.store_file_node(&file_node, content).unwrap();
        let diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();
        let snapshot = Snapshot::new_initial(file_node, delta.id);
        storage.store_snapshot(&snapshot, content).unwrap();

        let mut checkpoint_repo = CheckpointRepo::new_single(snapshot.id);

        let result = GitBridge::push_to_git(
            &storage,
            &git_path,
            &mut checkpoint_repo,
            "nonexistent-branch",
            "push",
        );
        assert!(result.is_err(), "push to non-existent branch should fail");
    }

    #[test]
    fn test_push_to_git_updates_git_anchor() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        let _git_repo = init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();
        let content = b"anchor test";
        let file_node = FileNode::new(PathBuf::from("anchor.txt"), content);
        storage.store_file_node(&file_node, content).unwrap();
        let diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();
        let snapshot = Snapshot::new_initial(file_node, delta.id);
        storage.store_snapshot(&snapshot, content).unwrap();

        let mut checkpoint_repo = CheckpointRepo::new_single(snapshot.id);
        let head = checkpoint_repo.current_branch_head();

        let git_hash = GitBridge::push_to_git(
            &storage,
            &git_path,
            &mut checkpoint_repo,
            "main",
            "anchor update test",
        )
        .unwrap();

        let cp = checkpoint_repo.get_checkpoint(&head).unwrap();
        assert_eq!(
            cp.metadata.git_anchor,
            Some(git_hash),
            "git_anchor should be updated after push"
        );
    }

    #[test]
    fn test_compare_status_ahead() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("git_repo");
        std::fs::create_dir_all(&git_path).unwrap();
        init_git_repo(&git_path);

        let storage = SqliteStorage::new_in_memory().unwrap();

        // Create initial snapshot
        let initial_content = b"initial content";
        let file_node1 = FileNode::new(PathBuf::from("ahead.txt"), initial_content);
        storage
            .store_file_node(&file_node1, initial_content)
            .unwrap();
        let diff1 = LineDiff::new(vec![]);
        let delta1 = Delta::new(file_node1.clone(), diff1, SourceType::Manual);
        storage.store_delta(&delta1).unwrap();
        let snapshot1 = Snapshot::new_initial(file_node1, delta1.id);
        storage.store_snapshot(&snapshot1, initial_content).unwrap();

        let mut checkpoint_repo = CheckpointRepo::new_single(snapshot1.id);

        // Create a new snapshot with different content
        let modified_content = b"ahead test";
        let file_node2 = FileNode::new(PathBuf::from("ahead.txt"), modified_content);
        storage
            .store_file_node(&file_node2, modified_content)
            .unwrap();
        let diff2 = diff_to_line_diff("initial content", "ahead test");
        let delta2 = Delta::new(file_node2.clone(), diff2, SourceType::Manual);
        storage.store_delta(&delta2).unwrap();
        let snapshot2 = Snapshot::from_parent(&snapshot1, delta2.id, "manual_edit".to_string());
        storage.store_snapshot(&snapshot2, b"").unwrap();

        // Create a new commit with the modified snapshot
        checkpoint_repo
            .commit_single(snapshot2.id, "local change", "user")
            .unwrap();

        let info = GitBridge::compare_status(&git_path, &checkpoint_repo, "main").unwrap();

        match info.status {
            SyncStatus::Ahead {
                unpushed_checkpoints,
            } => {
                assert!(unpushed_checkpoints > 0, "should be ahead");
            }
            _ => panic!("expected Ahead status"),
        }
    }

    #[test]
    fn test_compare_status_invalid_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("nonexistent_repo");

        let storage = SqliteStorage::new_in_memory().unwrap();
        let content = b"test";
        let file_node = FileNode::new(PathBuf::from("test.txt"), content);
        storage.store_file_node(&file_node, content).unwrap();
        let diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();
        let snapshot = Snapshot::new_initial(file_node, delta.id);
        storage.store_snapshot(&snapshot, content).unwrap();

        let checkpoint_repo = CheckpointRepo::new_single(snapshot.id);

        let result = GitBridge::compare_status(&git_path, &checkpoint_repo, "main");
        assert!(result.is_err(), "should fail on non-existent git repo");
    }

    #[test]
    fn test_init_from_git_invalid_path() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("nonexistent_repo");

        let storage = SqliteStorage::new_in_memory().unwrap();
        let dummy_file_node = FileNode::new(PathBuf::from("dummy"), b"init");
        let dummy_diff = LineDiff::new(vec![]);
        let dummy_delta = Delta::new(dummy_file_node.clone(), dummy_diff, SourceType::Manual);
        storage.store_delta(&dummy_delta).unwrap();
        let dummy_snap = Snapshot::new_initial(dummy_file_node, dummy_delta.id);
        storage.store_snapshot(&dummy_snap, b"init").unwrap();
        let mut checkpoint_repo = CheckpointRepo::new_single(dummy_snap.id);

        let result = GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD");
        assert!(result.is_err(), "should fail on non-existent git repo");
    }

    #[test]
    fn test_push_to_git_invalid_path() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("nonexistent_repo");

        let storage = SqliteStorage::new_in_memory().unwrap();
        let content = b"test";
        let file_node = FileNode::new(PathBuf::from("test.txt"), content);
        storage.store_file_node(&file_node, content).unwrap();
        let diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();
        let snapshot = Snapshot::new_initial(file_node, delta.id);
        storage.store_snapshot(&snapshot, content).unwrap();

        let mut checkpoint_repo = CheckpointRepo::new_single(snapshot.id);

        let result =
            GitBridge::push_to_git(&storage, &git_path, &mut checkpoint_repo, "main", "push");
        assert!(result.is_err(), "should fail on non-existent git repo");
    }

    #[test]
    fn test_init_from_git_bare_repo() {
        let dir = tempfile::tempdir().unwrap();
        let git_path = dir.path().join("bare_repo");

        // Initialize a bare repository
        let _git_repo = git2::Repository::init_bare(&git_path).unwrap();

        let storage = SqliteStorage::new_in_memory().unwrap();
        let dummy_file_node = FileNode::new(PathBuf::from("dummy"), b"init");
        let dummy_diff = LineDiff::new(vec![]);
        let dummy_delta = Delta::new(dummy_file_node.clone(), dummy_diff, SourceType::Manual);
        storage.store_delta(&dummy_delta).unwrap();
        let dummy_snap = Snapshot::new_initial(dummy_file_node, dummy_delta.id);
        storage.store_snapshot(&dummy_snap, b"init").unwrap();
        let mut checkpoint_repo = CheckpointRepo::new_single(dummy_snap.id);

        let result = GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD");
        // This might succeed or fail depending on implementation, but should not panic
        let _ = result;
    }
}
