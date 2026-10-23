use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use super::{ExportOptions, SYNC_RECONCILE_KEY};
use crate::file::FileCheckpointManager;
use crate::gc::GcRetention;
use crate::git_store::{BaselineRequest, MODE_FILE, REF_HUMAN, REF_MAIN};

fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("test-gitconfig").as_os_str())
        .output()
        .expect("git executable must exist for sync tests");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn make_user_repo() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("workspace tempdir");
    std::fs::write(dir.path().join("test-gitconfig"), "").unwrap();
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["branch", "-M", "main"]);
    run_git(dir.path(), &["config", "user.email", "t@t"]);
    run_git(dir.path(), &["config", "user.name", "t"]);
    run_git(dir.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.path().join("f"), "one").unwrap();
    run_git(dir.path(), &["add", "-A"]);
    run_git(dir.path(), &["commit", "-qm", "c1"]);
    let head = run_git(dir.path(), &["rev-parse", "HEAD"]);
    (dir, head)
}

fn commit_external(dir: &Path, path: &str, content: &str, message: &str) -> String {
    std::fs::write(dir.join(path), content).unwrap();
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-qm", message]);
    run_git(dir, &["rev-parse", "HEAD"])
}

fn manager_for(workspace: &Path) -> (FileCheckpointManager, tempfile::TempDir) {
    let store_dir = tempfile::tempdir().expect("store tempdir");
    let git = crate::git_store::GitStore::init(workspace, &store_dir.path().join("repo.git"))
        .expect("checkpoint store");
    let mut manager = FileCheckpointManager::new_in_memory().expect("manager");
    manager.store.git = Some(Arc::new(git));
    (manager, store_dir)
}

fn changes(pairs: &[(&str, &[u8])]) -> HashMap<String, Option<(String, Vec<u8>)>> {
    pairs
        .iter()
        .map(|(p, b)| (p.to_string(), Some((MODE_FILE.to_string(), b.to_vec()))))
        .collect()
}

#[test]
fn reconcile_key_does_not_collide_with_workspace_key() {
    assert_ne!(
        SYNC_RECONCILE_KEY,
        checkpoint_base::metadata::keys::WORKSPACE_ROOT_KEY
    );
    assert!(SYNC_RECONCILE_KEY.starts_with("file_checkpoint.sync."));
}

#[test]
fn export_rejects_dirty_worktree_without_confirmation() {
    let (repo, _) = make_user_repo();
    let (manager, _store) = manager_for(repo.path());
    let git = manager.git_ref().unwrap();
    git.commit_on_ref(
        REF_MAIN,
        &changes(&[("f", b"one"), ("g", b"local")]),
        "t",
        "main",
    )
    .unwrap();
    let err = manager
        .export_to_workspace(
            repo.path(),
            ExportOptions {
                confirm_overwrite: false,
                auto_commit: false,
                message: None,
            },
        )
        .expect_err("dirty worktree must require confirmation");
    assert!(
        matches!(
            err,
            checkpoint_base::error::CheckpointError::Validation { .. }
        ),
        "dirty export must be a validation error, got {err:?}"
    );
}

#[test]
fn export_materializes_and_records_reconcile() {
    let (repo, head) = make_user_repo();
    let (manager, _store) = manager_for(repo.path());
    let git = manager.git_ref().unwrap();
    let main = git
        .commit_on_ref(
            REF_MAIN,
            &changes(&[("f", b"one"), ("g", b"local")]),
            "t",
            "main",
        )
        .unwrap()
        .id;
    let outcome = manager
        .export_to_workspace(
            repo.path(),
            ExportOptions {
                confirm_overwrite: true,
                auto_commit: false,
                message: Some("export one".to_string()),
            },
        )
        .unwrap();
    assert!(!outcome.idempotent);
    assert_eq!(outcome.reconcile.internal_main, main);
    assert_eq!(outcome.reconcile.external_commit, head);
    assert_eq!(std::fs::read(repo.path().join("g")).unwrap(), b"local");
    let dirty = manager.export_dirty_paths(repo.path()).unwrap();
    assert!(dirty.is_empty(), "worktree matches main after export");
    let stored = manager
        .reconcile_point()
        .unwrap()
        .expect("reconcile stored");
    assert_eq!(stored, outcome.reconcile);
}

#[test]
fn export_auto_commit_aligns_external_head() {
    let (repo, _) = make_user_repo();
    let (manager, _store) = manager_for(repo.path());
    let git = manager.git_ref().unwrap();
    let main = git
        .commit_on_ref(
            REF_MAIN,
            &changes(&[("f", b"one"), ("g", b"local")]),
            "t",
            "main",
        )
        .unwrap()
        .id;
    let outcome = manager
        .export_to_workspace(
            repo.path(),
            ExportOptions {
                confirm_overwrite: true,
                auto_commit: true,
                message: Some("export auto".to_string()),
            },
        )
        .unwrap();
    let external = outcome.external_commit.clone().expect("auto commit head");
    assert_eq!(outcome.reconcile.internal_main, main);
    assert_eq!(outcome.reconcile.external_commit, external);
    let second = manager
        .export_to_workspace(
            repo.path(),
            ExportOptions {
                confirm_overwrite: true,
                auto_commit: true,
                message: Some("export auto".to_string()),
            },
        )
        .unwrap();
    assert!(second.idempotent, "aligned export is idempotent");
}

#[test]
fn export_rejects_unresolved_conflicts() {
    let (repo, _) = make_user_repo();
    let (manager, _store) = manager_for(repo.path());
    let git = manager.git_ref().unwrap();
    let tree = git
        .build_tree_from_parent(None, &changes(&[("f", b"bad")]))
        .unwrap();
    let conflicted = git
        .write_commit(
            &tree,
            &[],
            "t",
            crate::git_store::SYSTEM_COMMITTER,
            1_000,
            &format!(
                "conflicted\n{}: {}\n{}: f",
                crate::git_store::TRAILER_STATE,
                crate::git_store::STATE_CONFLICT_UNRESOLVED,
                crate::git_store::TRAILER_CONFLICT_FILE,
            ),
        )
        .unwrap();
    git.write_ref(REF_MAIN, &conflicted).unwrap();
    let err = manager
        .export_to_workspace(
            repo.path(),
            ExportOptions {
                confirm_overwrite: true,
                auto_commit: false,
                message: None,
            },
        )
        .expect_err("conflicted main must block export");
    assert!(
        matches!(
            err,
            checkpoint_base::error::CheckpointError::MergeConflict { .. }
        ),
        "conflict export must be a merge conflict, got {err:?}"
    );
}

#[test]
fn export_lock_blocks_concurrent_exports() {
    let (repo, _) = make_user_repo();
    let (manager, _store) = manager_for(repo.path());
    let git = manager.git_ref().unwrap();
    git.commit_on_ref(REF_MAIN, &changes(&[("f", b"one")]), "t", "main")
        .unwrap();
    let lock_path = git.git_dir().join("export.lock");
    std::fs::write(&lock_path, "held").unwrap();
    let err = manager
        .export_to_workspace(
            repo.path(),
            ExportOptions {
                confirm_overwrite: true,
                auto_commit: false,
                message: None,
            },
        )
        .expect_err("held lock must block export");
    assert!(
        matches!(err, checkpoint_base::error::CheckpointError::Branch(_)),
        "busy export must be a branch error, got {err:?}"
    );
    std::fs::remove_file(&lock_path).unwrap();
}

#[test]
fn import_captures_external_change_as_human() {
    let (repo, _) = make_user_repo();
    let (manager, _store) = manager_for(repo.path());
    let git = manager.git_ref().unwrap();
    git.commit_on_ref(REF_MAIN, &changes(&[("f", b"one")]), "t", "main")
        .unwrap();
    manager
        .export_to_workspace(
            repo.path(),
            ExportOptions {
                confirm_overwrite: true,
                auto_commit: true,
                message: Some("base export".to_string()),
            },
        )
        .unwrap();
    assert!(manager.external_update_hint(repo.path()).unwrap().is_none());
    let external = commit_external(repo.path(), "h", "hello", "external work");
    let hint = manager
        .external_update_hint(repo.path())
        .unwrap()
        .expect("external move must hint");
    assert_eq!(hint.external_head, external);
    let outcome = manager.import_from_external(repo.path()).unwrap();
    assert!(!outcome.idempotent);
    assert!(!outcome.had_conflicts);
    assert!(!outcome.no_base);
    assert!(!outcome.fallback_poll);
    let created = outcome.created.expect("human commit created");
    assert_eq!(outcome.advanced_to, external);
    assert_eq!(
        git.read_ref(REF_HUMAN).unwrap().as_deref(),
        Some(created.as_str())
    );
    let human_commit = git.read_commit(&created).unwrap();
    let human_files = git.tree_to_bytes(&human_commit.tree).unwrap();
    assert_eq!(
        human_files.get("h").map(Vec::as_slice),
        Some(b"hello".as_slice())
    );
    assert!(manager.external_update_hint(repo.path()).unwrap().is_none());
    let again = manager.import_from_external(repo.path()).unwrap();
    assert!(again.idempotent);
    assert!(again.created.is_none());
}

#[test]
fn import_without_reconcile_rejects() {
    let (repo, _) = make_user_repo();
    let (manager, _store) = manager_for(repo.path());
    let err = manager
        .import_from_external(repo.path())
        .expect_err("import without export must fail");
    assert!(
        matches!(
            err,
            checkpoint_base::error::CheckpointError::Validation { .. }
        ),
        "missing reconcile must be validation, got {err:?}"
    );
}

#[test]
fn import_overlap_enters_conflict_flow() {
    let (repo, _) = make_user_repo();
    let (manager, _store) = manager_for(repo.path());
    let git = manager.git_ref().unwrap();
    git.commit_on_ref(REF_MAIN, &changes(&[("f", b"base\n")]), "t", "main")
        .unwrap();
    manager
        .export_to_workspace(
            repo.path(),
            ExportOptions {
                confirm_overwrite: true,
                auto_commit: true,
                message: Some("base".to_string()),
            },
        )
        .unwrap();
    git.commit_on_ref(
        REF_MAIN,
        &changes(&[("f", b"ours\n")]),
        "t",
        "internal change",
    )
    .unwrap();
    commit_external(repo.path(), "f", "theirs\n", "external change");
    let outcome = manager.import_from_external(repo.path()).unwrap();
    assert!(outcome.had_conflicts, "overlap must conflict");
    let created = outcome.created.expect("conflicted human commit");
    let commit = git.read_commit(&created).unwrap();
    assert_eq!(
        commit.trailer(crate::git_store::TRAILER_STATE).as_deref(),
        Some(crate::git_store::STATE_CONFLICT_UNRESOLVED)
    );
    assert!(!commit
        .trailers(crate::git_store::TRAILER_CONFLICT_FILE)
        .is_empty());
    assert!(!manager.list_conflicts().unwrap().is_empty());
}

#[test]
fn rebase_transplant_preserves_main_tree() {
    let dir = tempfile::tempdir().expect("repo dir");
    std::fs::write(dir.path().join("test-gitconfig"), "").unwrap();
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["branch", "-M", "main"]);
    run_git(dir.path(), &["config", "user.email", "t@t"]);
    run_git(dir.path(), &["config", "user.name", "t"]);
    run_git(dir.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.path().join("f"), "one").unwrap();
    run_git(dir.path(), &["add", "-A"]);
    run_git(dir.path(), &["commit", "-qm", "c1"]);
    std::fs::write(dir.path().join("f"), "two").unwrap();
    run_git(dir.path(), &["add", "-A"]);
    run_git(dir.path(), &["commit", "-qm", "c2"]);
    let second = run_git(dir.path(), &["rev-parse", "HEAD"]);
    let store_dir = tempfile::tempdir().expect("store dir");
    let store = crate::git_store::GitStore::init(dir.path(), &store_dir.path().join("repo.git"))
        .expect("store");
    let first_req = BaselineRequest {
        mode: crate::git_store::BaselineMode::Borrow,
        branch: Some("main".to_string()),
        commit_prefix: None,
        behind: Some(1),
    };
    let seeded = match store.ensure_baseline(&first_req).unwrap() {
        crate::git_store::BaselineSeedOutcome::Seeded(a) => a,
        other => panic!("fresh store must seed, got {other:?}"),
    };
    assert_eq!(seeded.id.len(), 40);
    let old_main = store.read_ref(REF_MAIN).unwrap().unwrap();
    store
        .commit_on_ref(REF_MAIN, &changes(&[("g", b"local")]), "t", "local work")
        .unwrap();
    let old_tree = store
        .read_commit(&store.read_ref(REF_MAIN).unwrap().unwrap())
        .unwrap()
        .tree;
    std::fs::write(dir.path().join("f"), "three").unwrap();
    run_git(dir.path(), &["add", "-A"]);
    run_git(dir.path(), &["commit", "-qm", "c3"]);
    let third = run_git(dir.path(), &["rev-parse", "HEAD"]);
    assert_ne!(second, third);
    let rebase_req = BaselineRequest {
        mode: crate::git_store::BaselineMode::Borrow,
        branch: Some("main".to_string()),
        commit_prefix: None,
        behind: None,
    };
    let outcome = store.rebase_to(&rebase_req).unwrap();
    assert_eq!(outcome.new_anchor.id, third);
    assert_eq!(outcome.old_main.len(), 40);
    let new_main = store.read_ref(REF_MAIN).unwrap().unwrap();
    assert_eq!(new_main, outcome.transplant_id);
    let new_tree = store.read_commit(&new_main).unwrap().tree;
    assert_eq!(new_tree, old_tree, "transplant keeps old main content");
    let transplant = store.read_commit(&new_main).unwrap();
    assert_eq!(transplant.parents, vec![third]);
    assert!(
        store.read_commit(&old_main).is_ok(),
        "old chain stays readable"
    );
    assert!(matches!(
        store.validate_baseline().unwrap(),
        crate::git_store::BaselineStatus::Healthy(_)
    ));
}

#[test]
fn gc_keeps_baseline_chain_and_local_work() {
    let dir = tempfile::tempdir().expect("repo dir");
    std::fs::write(dir.path().join("test-gitconfig"), "").unwrap();
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["branch", "-M", "main"]);
    run_git(dir.path(), &["config", "user.email", "t@t"]);
    run_git(dir.path(), &["config", "user.name", "t"]);
    run_git(dir.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.path().join("f"), "one").unwrap();
    run_git(dir.path(), &["add", "-A"]);
    run_git(dir.path(), &["commit", "-qm", "c1"]);
    let store_dir = tempfile::tempdir().expect("store dir");
    let store = crate::git_store::GitStore::init(dir.path(), &store_dir.path().join("repo.git"))
        .expect("store");
    let req = BaselineRequest {
        mode: crate::git_store::BaselineMode::Borrow,
        branch: Some("main".to_string()),
        commit_prefix: None,
        behind: None,
    };
    store.ensure_baseline(&req).unwrap();
    let anchor = store.baseline_anchor_id().expect("anchor pinned");
    let local = store
        .commit_on_ref(REF_MAIN, &changes(&[("g", b"local")]), "t", "local work")
        .unwrap()
        .id;
    let manager = {
        let mut m = FileCheckpointManager::new_in_memory().expect("manager");
        m.store.git = Some(Arc::new(
            crate::git_store::GitStore::init(dir.path(), &store_dir.path().join("repo.git"))
                .expect("reopen"),
        ));
        m
    };
    let stats = manager
        .run_gc_with_grace(GcRetention::default(), std::time::Duration::ZERO)
        .unwrap();
    assert_eq!(stats.removed_checkpoints, 0);
    let git = manager.git_ref().unwrap();
    assert!(git.read_commit(&anchor).is_ok(), "anchor stays readable");
    assert!(git.read_commit(&local).is_ok(), "local work survives gc");
    let log = git.log(&local, 0).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].id, anchor);
}

#[test]
fn gc_enumeration_stays_local_and_pack_skips_borrowed_objects() {
    let dir = tempfile::tempdir().expect("repo dir");
    std::fs::write(dir.path().join("test-gitconfig"), "").unwrap();
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["branch", "-M", "main"]);
    run_git(dir.path(), &["config", "user.email", "t@t"]);
    run_git(dir.path(), &["config", "user.name", "t"]);
    run_git(dir.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.path().join("f"), "one").unwrap();
    run_git(dir.path(), &["add", "-A"]);
    run_git(dir.path(), &["commit", "-qm", "c1"]);
    let store_dir = tempfile::tempdir().expect("store dir");
    let store = crate::git_store::GitStore::init(dir.path(), &store_dir.path().join("repo.git"))
        .expect("store");
    let req = BaselineRequest {
        mode: crate::git_store::BaselineMode::Borrow,
        branch: Some("main".to_string()),
        commit_prefix: None,
        behind: None,
    };
    store.ensure_baseline(&req).unwrap();
    let anchor = store.baseline_anchor_id().unwrap();
    let anchor_commit = store.read_commit(&anchor).unwrap();
    let borrowed_files = store.tree_to_files(&anchor_commit.tree).unwrap();
    let borrowed_blob = borrowed_files
        .values()
        .next()
        .map(|(_, b)| b.clone())
        .expect("borrowed blob");
    assert!(
        !FileCheckpointManager::list_object_ids(&store).contains(&borrowed_blob),
        "borrowed blobs must not appear in local enumeration"
    );
    assert!(
        !FileCheckpointManager::list_object_ids(&store).contains(&anchor),
        "borrowed anchor commit must not appear in local enumeration"
    );
    for i in 0..400 {
        let mut one = HashMap::new();
        one.insert(
            format!("payload-{i}.txt"),
            Some((MODE_FILE.to_string(), format!("payload-{i}").into_bytes())),
        );
        store.commit_on_ref(REF_MAIN, &one, "t", "bulk").unwrap();
    }
    let manager = {
        let mut m = FileCheckpointManager::new_in_memory().expect("manager");
        m.store.git = Some(Arc::new(
            crate::git_store::GitStore::init(dir.path(), &store_dir.path().join("repo.git"))
                .expect("reopen"),
        ));
        m
    };
    let stats = manager
        .run_gc_with_grace(GcRetention::default(), std::time::Duration::ZERO)
        .unwrap();
    assert!(stats.packed_objects > 0, "threshold breach must pack");
    let pack_dir = store.git_dir().join("objects/pack");
    let packs: Vec<_> = std::fs::read_dir(&pack_dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "pack"))
        .collect();
    assert!(!packs.is_empty(), "packfile must exist after threshold gc");
    for pack in packs {
        let idx = pack.with_extension("idx");
        let out = Command::new("git")
            .arg("verify-pack")
            .arg("-v")
            .arg(&idx)
            .output()
            .expect("git verify-pack");
        assert!(out.status.success());
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            !text.contains(&borrowed_blob),
            "pack must not absorb borrowed objects"
        );
        assert!(
            !text.contains(&anchor),
            "pack must not absorb the borrowed anchor"
        );
    }
    assert!(
        store.read_commit(&anchor).is_ok(),
        "anchor readable after pack"
    );
}
