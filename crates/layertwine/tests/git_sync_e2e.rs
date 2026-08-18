//! git_sync e2e tests — real git operations against `GitBridge`.
//!
//! Each test creates a temporary directory, initialises a real git repository,
//! exercises `GitBridge` methods, and verifies results by inspecting the
//! actual git repository (via libgit2) and the layertwine storage.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use layertwine::checkpoint::repo::CheckpointRepo;
use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{CheckpointId, LineDiff, SnapshotId, SourceType};
use layertwine::error::Result;
use layertwine::git_sync::GitBridge;
use layertwine::git_sync::SyncStatus;
use layertwine::storage::repository::{DeltaStore, FileNodeStore, SnapshotStore};
use layertwine::storage::SqliteStorage;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimum git user config required for libgit2 to create commits.
static GIT_USER: LazyLock<(String, String)> = LazyLock::new(|| {
    let name = std::env::var("GIT_AUTHOR_NAME").unwrap_or_else(|_| "E2E Test".into());
    let email = std::env::var("GIT_AUTHOR_EMAIL").unwrap_or_else(|_| "e2e@layertwine.test".into());
    (name, email)
});

/// Initialise an empty git repository and create an initial commit so HEAD exists.
fn init_git_repo(path: &Path) -> git2::Repository {
    let repo = git2::Repository::init(path).expect("git init failed");
    let readme = path.join("README.md");
    std::fs::write(&readme, b"# E2E Test\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("README.md")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = git2::Signature::now(&GIT_USER.0, &GIT_USER.1).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
        .unwrap();
    drop(tree);
    repo
}

/// Create layertwine storage + checkpoint repo with a dummy root checkpoint.
fn create_layertwine_state() -> Result<(SqliteStorage, CheckpointRepo, SnapshotId)> {
    let storage = SqliteStorage::new_in_memory()?;
    let file_node = FileNode::new(PathBuf::from(".layertwine_root"), b"root");
    let delta = Delta::new(file_node.clone(), LineDiff::new(vec![]), SourceType::Manual);
    storage.store_delta(&delta)?;
    let snapshot = Snapshot::new_initial(file_node, delta.id);
    storage.store_snapshot(&snapshot, b"root")?;
    let checkpoint_repo = CheckpointRepo::new_single(snapshot.id);
    Ok((storage, checkpoint_repo, snapshot.id))
}

/// Store a file in layertwine storage (without git).
fn store_file(storage: &SqliteStorage, rel_path: &str, content: &[u8]) -> Result<SnapshotId> {
    let file_node = FileNode::new(PathBuf::from(rel_path), content);
    storage.store_file_node(&file_node, content)?;
    let delta = Delta::new(file_node.clone(), LineDiff::new(vec![]), SourceType::Manual);
    storage.store_delta(&delta)?;
    let snapshot = Snapshot::new_initial(file_node, delta.id);
    storage.store_snapshot(&snapshot, content)?;
    Ok(snapshot.id)
}

/// Assert that a git repo's HEAD has a file with the expected content.
fn assert_git_file_content(repo: &git2::Repository, rel_path: &str, expected: &[u8]) {
    let commit = repo.head().unwrap().peel_to_commit().unwrap();
    let tree = commit.tree().unwrap();
    let entry = tree
        .get_path(Path::new(rel_path))
        .unwrap_or_else(|_| panic!("file '{}' not found in git HEAD tree", rel_path));
    let blob = repo.find_blob(entry.id()).expect("not a blob");
    assert_eq!(
        blob.content(),
        expected,
        "content mismatch for '{}'",
        rel_path
    );
}

/// Commit a snapshot on the current branch.
fn commit_snapshot(
    checkpoint_repo: &mut CheckpointRepo,
    snapshot_id: SnapshotId,
    message: &str,
) -> CheckpointId {
    checkpoint_repo
        .commit_single(snapshot_id, message, "e2e-test")
        .expect("checkpoint commit failed")
}

/// Make a direct git commit (bypassing layertwine).
fn git_commit_file(repo: &git2::Repository, rel_path: &str, content: &[u8], msg: &str) {
    let workdir = repo.workdir().unwrap().to_path_buf();
    let full_path = workdir.join(rel_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&full_path, content).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(rel_path)).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = git2::Signature::now(&GIT_USER.0, &GIT_USER.1).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&parent])
        .unwrap();
}

/// Combined fixture: creates a git repo, initialises layertwine from it.
struct GitSyncFixture {
    _dir: tempfile::TempDir,
    git_repo_path: PathBuf,
    git_repo: git2::Repository,
    storage: SqliteStorage,
    checkpoint_repo: CheckpointRepo,
}

impl GitSyncFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let git_path = dir.path().join("repo");
        std::fs::create_dir_all(&git_path).unwrap();
        let git_repo = init_git_repo(&git_path);
        let (storage, mut checkpoint_repo, _root) =
            create_layertwine_state().expect("layertwine state");
        GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD")
            .expect("init_from_git");
        GitSyncFixture {
            _dir: dir,
            git_repo_path: git_path,
            git_repo,
            storage,
            checkpoint_repo,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests: init_from_git
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_init_from_git_basic() {
    let dir = tempfile::tempdir().unwrap();
    let git_path = dir.path().join("repo");
    std::fs::create_dir_all(&git_path).unwrap();
    let repo = init_git_repo(&git_path);

    let (storage, mut checkpoint_repo, _root) = create_layertwine_state().unwrap();
    GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD")
        .expect("init_from_git should succeed");

    let head_id = checkpoint_repo.current_branch_head();
    let cp = checkpoint_repo.get_checkpoint(&head_id).unwrap();
    assert!(
        cp.metadata.git_anchor.is_some(),
        "git_anchor should be set after init_from_git"
    );

    let expected_hash = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    assert_eq!(
        cp.metadata.git_anchor.as_deref(),
        Some(expected_hash.as_str()),
        "git_anchor mismatch"
    );
}

#[test]
fn test_e2e_init_from_git_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let git_path = dir.path().join("repo");
    std::fs::create_dir_all(&git_path).unwrap();
    let repo = init_git_repo(&git_path);

    let commit = repo.head().unwrap().peel_to_commit().unwrap();
    let expected_author = commit.author().name().unwrap_or("").to_string();
    let expected_msg = commit.message().unwrap_or("").trim().to_string();

    let (storage, mut checkpoint_repo, _root) = create_layertwine_state().unwrap();
    GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD").unwrap();

    let head_id = checkpoint_repo.current_branch_head();
    let cp = checkpoint_repo.get_checkpoint(&head_id).unwrap();
    assert_eq!(cp.metadata.author, expected_author);
    assert_eq!(cp.metadata.message, expected_msg);
}

#[test]
fn test_e2e_init_from_git_multiple_files() {
    let dir = tempfile::tempdir().unwrap();
    let git_path = dir.path().join("repo");
    std::fs::create_dir_all(&git_path).unwrap();

    let repo = git2::Repository::init(&git_path).unwrap();
    let sig = git2::Signature::now("test", "test@test.com").unwrap();

    for (name, content) in &[
        ("a.txt", &b"AAA\n"[..]),
        ("b.txt", &b"BBB\n"[..]),
        ("c.txt", &b"CCC\n"[..]),
    ] {
        std::fs::write(git_path.join(name), content).unwrap();
    }
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.txt")).unwrap();
    index.add_path(Path::new("b.txt")).unwrap();
    index.add_path(Path::new("c.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "multi-file", &tree, &[])
        .unwrap();

    let (storage, mut checkpoint_repo, _root) = create_layertwine_state().unwrap();
    GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD").unwrap();

    let head_id = checkpoint_repo.current_branch_head();
    let cp = checkpoint_repo.get_checkpoint(&head_id).unwrap();
    // README.md is NOT in this repo (we created from scratch), only a.txt, b.txt, c.txt = 3
    assert_eq!(
        cp.baseline_snapshots.len(),
        3,
        "expected 3 baseline snapshots"
    );

    for snap_id in &cp.baseline_snapshots {
        let snapshot = storage.get_snapshot(snap_id).unwrap();
        let content = storage
            .get_file_content(snapshot.file.path_str(), &snapshot.file.base_hash)
            .unwrap();
        assert!(!content.is_empty(), "snapshot content should not be empty");
    }
}

#[test]
fn test_e2e_init_from_git_subdirectories() {
    let dir = tempfile::tempdir().unwrap();
    let git_path = dir.path().join("repo");
    std::fs::create_dir_all(&git_path).unwrap();

    let repo = git2::Repository::init(&git_path).unwrap();
    let sig = git2::Signature::now("test", "test@test.com").unwrap();

    let sub_paths: &[(&str, &[u8])] = &[
        ("src/main.rs", b"fn main() {}\n"),
        ("src/lib.rs", b"pub fn f() {}\n"),
        ("tests/test.rs", b"#[test] fn t() {}\n"),
    ];
    for (rel, content) in sub_paths {
        let full = git_path.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, content).unwrap();
    }
    let mut index = repo.index().unwrap();
    for (rel, _) in sub_paths {
        index.add_path(Path::new(rel)).unwrap();
    }
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "subdirs", &tree, &[])
        .unwrap();

    let (storage, mut checkpoint_repo, _root) = create_layertwine_state().unwrap();
    GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD").unwrap();

    let head_id = checkpoint_repo.current_branch_head();
    let cp = checkpoint_repo.get_checkpoint(&head_id).unwrap();
    assert_eq!(cp.baseline_snapshots.len(), 3);

    let found = cp.baseline_snapshots.iter().any(|sid| {
        storage
            .get_snapshot(sid)
            .map(|s| s.file.path_str() == "src/main.rs")
            .unwrap_or(false)
    });
    assert!(found, "src/main.rs should be in snapshots");
}

#[test]
fn test_e2e_init_from_git_invalid_ref_fails() {
    let dir = tempfile::tempdir().unwrap();
    let git_path = dir.path().join("repo");
    std::fs::create_dir_all(&git_path).unwrap();
    init_git_repo(&git_path);

    let (storage, mut checkpoint_repo, _root) = create_layertwine_state().unwrap();
    let result = GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "nonexistent");
    assert!(result.is_err(), "should fail with invalid ref");
}

// ---------------------------------------------------------------------------
// Tests: push_to_git
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_push_to_git_basic() {
    let dir = tempfile::tempdir().unwrap();
    let git_path = dir.path().join("repo");
    std::fs::create_dir_all(&git_path).unwrap();
    let repo = init_git_repo(&git_path);

    let (storage, mut checkpoint_repo, _root) = create_layertwine_state().unwrap();
    let snap_id = store_file(&storage, "pushed.txt", b"pushed content\n").unwrap();
    commit_snapshot(&mut checkpoint_repo, snap_id, "prep push");

    let hash = GitBridge::push_to_git(
        &storage,
        &git_path,
        &mut checkpoint_repo,
        "main",
        "layertwine push",
    )
    .expect("push_to_git should succeed");

    assert!(!hash.is_empty(), "should return a git hash");
    assert_git_file_content(&repo, "pushed.txt", b"pushed content\n");
}

#[test]
fn test_e2e_push_to_git_updates_anchor() {
    let dir = tempfile::tempdir().unwrap();
    let git_path = dir.path().join("repo");
    std::fs::create_dir_all(&git_path).unwrap();
    let _repo = init_git_repo(&git_path);

    let (storage, mut checkpoint_repo, _root) = create_layertwine_state().unwrap();
    let snap_id = store_file(&storage, "data.txt", b"data\n").unwrap();
    commit_snapshot(&mut checkpoint_repo, snap_id, "prep");

    GitBridge::push_to_git(&storage, &git_path, &mut checkpoint_repo, "main", "push").unwrap();

    let head_id = checkpoint_repo.current_branch_head();
    let cp = checkpoint_repo.get_checkpoint(&head_id).unwrap();
    assert!(
        cp.metadata.git_anchor.is_some(),
        "git_anchor should be set after push"
    );
}

// ---------------------------------------------------------------------------
// Tests: round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_roundtrip_sync() {
    let dir = tempfile::tempdir().unwrap();
    let git_path = dir.path().join("repo");
    std::fs::create_dir_all(&git_path).unwrap();
    let repo = init_git_repo(&git_path);

    let (storage, mut checkpoint_repo, _root) = create_layertwine_state().unwrap();

    // Phase 1: init_from_git
    GitBridge::init_from_git(&git_path, &storage, &mut checkpoint_repo, "HEAD").unwrap();

    // Phase 2: add a new file in layertwine
    let snap_id = store_file(&storage, "roundtrip.txt", b"roundtrip content\n").unwrap();
    commit_snapshot(&mut checkpoint_repo, snap_id, "add roundtrip.txt");

    // Phase 3: push back to git
    GitBridge::push_to_git(
        &storage,
        &git_path,
        &mut checkpoint_repo,
        "main",
        "layertwine roundtrip",
    )
    .unwrap();

    assert_git_file_content(&repo, "README.md", b"# E2E Test\n");
    assert_git_file_content(&repo, "roundtrip.txt", b"roundtrip content\n");
}

// ---------------------------------------------------------------------------
// Tests: compare_status
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_compare_status_in_sync() {
    let fixture = GitSyncFixture::new();
    let info = GitBridge::compare_status(&fixture.git_repo_path, &fixture.checkpoint_repo, "main")
        .expect("compare_status");
    assert_eq!(
        info.status,
        SyncStatus::InSync,
        "init_from_git should produce InSync status (git HEAD == anchor)"
    );
}

#[test]
fn test_e2e_compare_status_ahead() {
    let mut fixture = GitSyncFixture::new();
    let snap_id = store_file(&fixture.storage, "new.txt", b"new\n").unwrap();
    commit_snapshot(&mut fixture.checkpoint_repo, snap_id, "ahead");

    let info = GitBridge::compare_status(&fixture.git_repo_path, &fixture.checkpoint_repo, "main")
        .expect("compare_status");
    assert!(
        matches!(info.status, SyncStatus::Ahead { .. }),
        "expected Ahead status, got {:?}",
        info.status
    );
}

#[test]
fn test_e2e_compare_status_behind() {
    let fixture = GitSyncFixture::new();
    git_commit_file(
        &fixture.git_repo,
        "external.txt",
        b"from git\n",
        "external git commit",
    );

    let info = GitBridge::compare_status(&fixture.git_repo_path, &fixture.checkpoint_repo, "main")
        .expect("compare_status");
    assert!(
        matches!(info.status, SyncStatus::Behind { .. }),
        "expected Behind status, got {:?}",
        info.status
    );
}

#[test]
fn test_e2e_compare_status_divergent() {
    let mut fixture = GitSyncFixture::new();

    // To produce Divergent, we need:
    //   1. A layertwine checkpoint with a git_anchor (by pushing to git)
    //   2. A git commit on a path that diverges from that anchor
    //
    // Strategy: after push_to_git, reset git HEAD to the anchor's parent
    // and make a conflicting git commit.

    // Step 1: make a layertwine commit and push it to git
    let snap_id = store_file(&fixture.storage, "diverged.txt", b"layertwine\n").unwrap();
    commit_snapshot(&mut fixture.checkpoint_repo, snap_id, "layertwine prep");

    // This creates a git commit and sets git_anchor on the current checkpoint
    GitBridge::push_to_git(
        &fixture.storage,
        &fixture.git_repo_path,
        &mut fixture.checkpoint_repo,
        "main",
        "layertwine push for divergence",
    )
    .expect("push_to_git");

    // Step 2: get the anchor OID, then reset git HEAD to the anchor's parent
    let head_id = fixture.checkpoint_repo.current_branch_head();
    let cp = fixture.checkpoint_repo.get_checkpoint(&head_id).unwrap();
    let anchor_oid = cp
        .metadata
        .git_anchor
        .as_ref()
        .map(|h| git2::Oid::from_str(h).unwrap())
        .expect("git_anchor should exist after push");

    // Find the anchor commit's parent to reset to
    let anchor_commit = fixture
        .git_repo
        .find_commit(anchor_oid)
        .expect("anchor commit");
    let parent_oid = anchor_commit.parent_id(0).expect("anchor has parent");

    // Hard-reset git HEAD to the anchor's parent (simulating a different history line)
    let reset_target = fixture
        .git_repo
        .find_object(parent_oid, None)
        .expect("find reset target");
    fixture
        .git_repo
        .reset(&reset_target, git2::ResetType::Hard, None)
        .expect("git reset");

    // Step 3: make a divergent git commit from this older base
    git_commit_file(
        &fixture.git_repo,
        "conflict.txt",
        b"from git on different path\n",
        "divergent git commit",
    );

    // Now: git HEAD is a sibling of the anchor (both derive from the same parent)
    // graph_ahead_behind should report divergence
    let info = GitBridge::compare_status(&fixture.git_repo_path, &fixture.checkpoint_repo, "main")
        .expect("compare_status");
    assert!(
        matches!(info.status, SyncStatus::Divergent { .. }),
        "expected Divergent status after reset + divergent commit, got {:?}",
        info.status
    );
}

// ---------------------------------------------------------------------------
// Tests: GC integration with real git_anchor
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_gc_protects_git_anchored_checkpoints() {
    let mut fixture = GitSyncFixture::new();

    let head_id = fixture.checkpoint_repo.current_branch_head();
    let cp = fixture.checkpoint_repo.get_checkpoint(&head_id).unwrap();
    assert!(
        cp.metadata.git_anchor.is_some(),
        "init checkpoint should have git_anchor"
    );

    // Protected as branch head + git_anchor
    let protected = layertwine::git_sync::collect_protected_checkpoints(&fixture.checkpoint_repo);
    assert!(
        protected.contains(&head_id),
        "branch head with git_anchor should be protected"
    );

    // After removing the branch, should still be protected by git_anchor alone
    fixture
        .checkpoint_repo
        .branches
        .retain(|b| b.name != "main");
    let protected_after =
        layertwine::git_sync::collect_protected_checkpoints(&fixture.checkpoint_repo);
    assert!(
        protected_after.contains(&head_id),
        "checkpoint with git_anchor should still be protected even without a branch"
    );
}
