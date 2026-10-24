use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use super::*;
use crate::git_store::REF_MAIN;

fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("test-gitconfig").as_os_str())
        .output()
        .expect("git executable must exist for baseline tests");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// User repository fixture: three linear commits on `main`.
/// Returns the repo dir guard plus (second, third) commit ids.
fn make_user_repo() -> (tempfile::TempDir, String, String) {
    let dir = tempfile::tempdir().expect("fixture tempdir");
    fs::write(dir.path().join("test-gitconfig"), "").unwrap();
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["branch", "-M", "main"]);
    let commit = |message: &str| {
        run_git(
            dir.path(),
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                message,
            ],
        )
    };
    fs::write(dir.path().join("f"), "one").unwrap();
    run_git(dir.path(), &["add", "-A"]);
    commit("c1");
    fs::write(dir.path().join("f"), "two").unwrap();
    run_git(dir.path(), &["add", "-A"]);
    commit("c2");
    let second = run_git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("f"), "three").unwrap();
    run_git(dir.path(), &["add", "-A"]);
    commit("c3");
    let third = run_git(dir.path(), &["rev-parse", "HEAD"]);
    (dir, second, third)
}

fn change(path: &str, content: &[u8]) -> HashMap<String, Option<(String, Vec<u8>)>> {
    let mut map = HashMap::new();
    map.insert(
        path.to_string(),
        Some((super::super::MODE_FILE.to_string(), content.to_vec())),
    );
    map
}

fn borrow_main() -> BaselineRequest {
    BaselineRequest {
        mode: BaselineMode::Borrow,
        branch: Some("main".to_string()),
        commit_prefix: None,
        behind: None,
        tag: None,
    }
}

#[test]
fn baseline_request_validation_rejects_bad_selectors() {
    let store = GitStore::init_temp().expect("temp store");

    let behind_without_branch = BaselineRequest {
        mode: BaselineMode::Borrow,
        branch: None,
        commit_prefix: None,
        behind: Some(2),
        tag: None,
    };
    assert!(matches!(
        store.ensure_baseline(&behind_without_branch),
        Err(GitStoreError::InvalidInput(_))
    ));

    let both_selectors = BaselineRequest {
        mode: BaselineMode::Borrow,
        branch: Some("main".to_string()),
        commit_prefix: Some("abcd".to_string()),
        behind: Some(1),
        tag: None,
    };
    assert!(matches!(
        store.ensure_baseline(&both_selectors),
        Err(GitStoreError::InvalidInput(_))
    ));

    let bad_prefix = BaselineRequest {
        mode: BaselineMode::Borrow,
        branch: None,
        commit_prefix: Some("xyz".to_string()),
        behind: None,
        tag: None,
    };
    assert!(matches!(
        store.ensure_baseline(&bad_prefix),
        Err(GitStoreError::InvalidInput(_))
    ));

    let copy_mode = BaselineRequest {
        mode: BaselineMode::Copy,
        branch: Some("main".to_string()),
        commit_prefix: None,
        behind: None,
        tag: None,
    };
    // Copy mode is a supported self-contained seed; without a user
    // repository behind the temp store it fails as unavailable, not as
    // unimplemented.
    assert!(matches!(
        store.ensure_baseline(&copy_mode),
        Err(GitStoreError::Baseline(_))
    ));
}

#[test]
fn seed_outside_a_repository_fails_without_touching_refs() {
    let workspace = tempfile::tempdir().expect("plain dir");
    let store = GitStore::init(workspace.path(), &workspace.path().join("store"))
        .expect("store in plain dir");
    let err = store
        .ensure_baseline(&borrow_main())
        .expect_err("plain dir is not a repository");
    assert!(matches!(err, GitStoreError::Baseline(_)));
    assert_eq!(store.read_ref(REF_MAIN).unwrap(), None);
}

#[test]
fn seed_skips_when_main_ref_already_exists() {
    let store = GitStore::init_temp().expect("temp store");
    store
        .commit_on_ref(REF_MAIN, &change("f", b"local"), "t", "local history")
        .unwrap();
    let outcome = store.ensure_baseline(&borrow_main()).unwrap();
    assert!(
        matches!(outcome, BaselineSeedOutcome::Skipped(_)),
        "pre-existing history must stay untouched"
    );
    assert!(
        matches!(
            store.validate_baseline().unwrap(),
            BaselineStatus::NoBaseline
        ),
        "skipped stores carry no anchor"
    );
}

#[test]
fn seed_borrow_freezes_history_and_bounds_traversal() {
    let (repo, second, third) = make_user_repo();
    let store_dir = tempfile::tempdir().expect("store dir");
    let store = GitStore::init(repo.path(), &store_dir.path().join("repo.git"))
        .expect("store bound to fixture repo");

    let outcome = store.ensure_baseline(&borrow_main()).unwrap();
    let BaselineSeedOutcome::Seeded(anchor) = outcome else {
        panic!("fresh store must seed, got {outcome:?}");
    };
    assert_eq!(anchor.id, third);
    assert_eq!(
        store.read_ref(REF_MAIN).unwrap().as_deref(),
        Some(third.as_str())
    );

    // Anchor survives a reopen: it is pinned in the store, not re-resolved.
    let reopened =
        GitStore::init(repo.path(), &store_dir.path().join("repo.git")).expect("reopen store");
    assert!(matches!(
        reopened.validate_baseline().unwrap(),
        BaselineStatus::Healthy(_)
    ));

    // One local commit on top of the borrowed anchor.
    let head = store
        .commit_on_ref(REF_MAIN, &change("g", b"local"), "t", "local work")
        .unwrap()
        .id;

    // Log records the anchor but never descends into external history.
    let log = store.log(&head, 0).unwrap();
    let ids: Vec<_> = log.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec![head.as_str(), third.as_str()]);

    // Full enumeration sees the same boundary.
    let all = store.all_commits().unwrap();
    assert_eq!(all.len(), 2);

    // Ancestor checks hold inside the boundary and stop at it.
    assert!(store.is_ancestor(&third, &head).unwrap());
    assert!(!store.is_ancestor(&second, &head).unwrap());
    assert_eq!(
        store.merge_base(&head, &head).unwrap().as_deref(),
        Some(head.as_str())
    );
    assert_eq!(store.merge_base(&head, &second).unwrap(), None);
}

#[test]
fn seed_recovers_anchor_when_main_ref_is_missing() {
    let (repo, _second, third) = make_user_repo();
    let store_dir = tempfile::tempdir().expect("store dir");
    let store = GitStore::init(repo.path(), &store_dir.path().join("repo.git"))
        .expect("store bound to fixture repo");
    assert!(matches!(
        store.ensure_baseline(&borrow_main()).unwrap(),
        BaselineSeedOutcome::Seeded(_)
    ));

    store.delete_ref(REF_MAIN).unwrap();
    let outcome = store.ensure_baseline(&borrow_main()).unwrap();
    assert!(
        matches!(outcome, BaselineSeedOutcome::AlreadySeeded(_)),
        "anchor without main ref must finish the seed"
    );
    assert_eq!(
        store.read_ref(REF_MAIN).unwrap().as_deref(),
        Some(third.as_str())
    );
}

#[test]
fn validate_repairs_stale_alternates_and_reports_a_lost_repo() {
    let (repo, _second, _third) = make_user_repo();
    let store_dir = tempfile::tempdir().expect("store dir");
    let git_dir = store_dir.path().join("repo.git");
    let store = GitStore::init(repo.path(), &git_dir).expect("store bound to fixture repo");
    assert!(matches!(
        store.ensure_baseline(&borrow_main()).unwrap(),
        BaselineSeedOutcome::Seeded(_)
    ));

    // Break the alternates entry: re-discovery must repair it.
    fs::write(
        git_dir.join("objects/info/alternates"),
        "/nonexistent/objects\n",
    )
    .unwrap();
    assert!(matches!(
        store.validate_baseline().unwrap(),
        BaselineStatus::Healthy(_)
    ));

    // Remove the whole repository: nothing left to repair from.
    let held = tempfile::tempdir().expect("hold dir");
    let moved = held.path().join("repo-gone");
    fs::rename(repo.path(), &moved).unwrap();
    let status = store.validate_baseline().unwrap();
    assert!(
        matches!(status, BaselineStatus::Degraded { .. }),
        "lost repository must degrade, got {status:?}"
    );
}

#[test]
fn seed_copy_stays_self_contained_without_alternates() {
    let (repo, _second, third) = make_user_repo();
    let store_dir = tempfile::tempdir().expect("store dir");
    let git_dir = store_dir.path().join("repo.git");
    let store = GitStore::init(repo.path(), &git_dir).expect("store bound to fixture repo");
    let req = BaselineRequest {
        mode: BaselineMode::Copy,
        branch: Some("main".to_string()),
        commit_prefix: None,
        behind: None,
        tag: None,
    };
    let anchor = match store.ensure_baseline(&req).unwrap() {
        BaselineSeedOutcome::Seeded(a) => a,
        other => panic!("fresh store must seed copy, got {other:?}"),
    };
    assert_eq!(anchor.id, third);
    assert!(
        !git_dir.join("objects/info/alternates").is_file(),
        "copy mode must not leave an alternates entry"
    );
    assert!(matches!(
        store.validate_baseline().unwrap(),
        BaselineStatus::Healthy(_)
    ));
    let head = store
        .commit_on_ref(REF_MAIN, &change("g", b"local"), "t", "local work")
        .unwrap()
        .id;
    let log = store.log(&head, 0).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].id, third);
}

#[test]
fn seed_copy_uses_pack_and_survives_external_loss() {
    let (repo, _second, third) = make_user_repo();
    let store_dir = tempfile::tempdir().expect("store dir");
    let git_dir = store_dir.path().join("repo.git");
    let store = GitStore::init(repo.path(), &git_dir).expect("store bound to fixture repo");
    let req = BaselineRequest {
        mode: BaselineMode::Copy,
        branch: Some("main".to_string()),
        commit_prefix: None,
        behind: None,
        tag: None,
    };
    assert!(matches!(
        store.ensure_baseline(&req).unwrap(),
        BaselineSeedOutcome::Seeded(_)
    ));
    let packs: Vec<_> = std::fs::read_dir(git_dir.join("objects/pack"))
        .expect("pack dir exists after copy seed")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "pack"))
        .collect();
    assert!(
        !packs.is_empty(),
        "copy seed must install one packfile, not loose objects"
    );
    for pack in &packs {
        let idx = pack.with_extension("idx");
        assert!(idx.is_file(), "pack index must move with the pack");
    }
    assert!(
        store.read_commit(&third).is_ok(),
        "anchor commit must read through the pack"
    );
    let held = tempfile::tempdir().expect("hold dir");
    let moved = held.path().join("repo-gone");
    std::fs::rename(repo.path(), &moved).expect("remove external repo");
    assert!(
        store.read_commit(&third).is_ok(),
        "anchor must stay readable after the external repo is gone"
    );
    assert!(matches!(
        store.validate_baseline().unwrap(),
        BaselineStatus::Healthy(_)
    ));
}

#[test]
fn seed_tag_pins_the_tagged_commit() {
    let (repo, second, _third) = make_user_repo();
    run_git(repo.path(), &["tag", "v1", &second]);
    let store_dir = tempfile::tempdir().expect("store dir");
    let store = GitStore::init(repo.path(), &store_dir.path().join("repo.git"))
        .expect("store bound to fixture repo");
    let req = BaselineRequest {
        mode: BaselineMode::Borrow,
        branch: None,
        commit_prefix: None,
        behind: None,
        tag: Some("v1".to_string()),
    };
    let anchor = match store.ensure_baseline(&req).unwrap() {
        BaselineSeedOutcome::Seeded(anchor) => anchor,
        other => panic!("fresh store must seed, got {other:?}"),
    };
    assert_eq!(anchor.id, second);
    assert_eq!(anchor.source_spec, format!("tag v1 -> {second}"));
}

#[test]
fn tag_selector_rejects_other_locators() {
    let store = GitStore::init_temp().expect("temp store");
    let req = BaselineRequest {
        mode: BaselineMode::Borrow,
        branch: Some("main".to_string()),
        commit_prefix: None,
        behind: None,
        tag: Some("v1".to_string()),
    };
    assert!(matches!(
        store.ensure_baseline(&req),
        Err(GitStoreError::InvalidInput(_))
    ));
}

#[test]
fn validate_treats_a_local_repository_move_as_healthy() {
    let (repo, _second, third) = make_user_repo();
    let store_dir = tempfile::tempdir().expect("store dir");
    let git_dir = store_dir.path().join("repo.git");
    let store = GitStore::init(repo.path(), &git_dir).expect("store bound to fixture repo");
    assert!(matches!(
        store.ensure_baseline(&borrow_main()).unwrap(),
        BaselineSeedOutcome::Seeded(_)
    ));

    let held = tempfile::tempdir().expect("hold dir");
    let moved = held.path().join("moved-repo");
    fs::rename(repo.path(), &moved).expect("move the user repository");

    let reopened = GitStore::init(&moved, &git_dir).expect("reopen store on the moved workspace");
    let anchor = match reopened.validate_baseline().unwrap() {
        BaselineStatus::Healthy(anchor) => anchor,
        other => panic!("moved repository must stay healthy, got {other:?}"),
    };
    assert_eq!(anchor.id, third);
    let discovered =
        crate::git_store::external::discover_user_objects_dir(&moved).expect("object discovery");
    assert_eq!(Path::new(&anchor.objects_path), discovered.as_path());
}

#[test]
fn validate_degrades_when_remote_identity_changes() {
    let (repo, _second, _third) = make_user_repo();
    run_git(
        repo.path(),
        &["remote", "add", "origin", "https://example.invalid/one.git"],
    );
    let store_dir = tempfile::tempdir().expect("store dir");
    let store = GitStore::init(repo.path(), &store_dir.path().join("repo.git"))
        .expect("store bound to fixture repo");
    assert!(matches!(
        store.ensure_baseline(&borrow_main()).unwrap(),
        BaselineSeedOutcome::Seeded(_)
    ));

    run_git(
        repo.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "https://example.invalid/two.git",
        ],
    );
    assert!(
        matches!(
            store.validate_baseline().unwrap(),
            BaselineStatus::Degraded { .. }
        ),
        "a different origin must be reported as a replacement"
    );
}

#[test]
fn anchor_parse_rejects_malformed_fields() {
    let anchor = BaselineAnchor {
        id: "a".repeat(40),
        mode: BaselineMode::Borrow,
        objects_path: "/objects".to_string(),
        source_repo: "/repo".to_string(),
        source_spec: "branch main tip".to_string(),
        remote_identity: String::new(),
        objects_fingerprint: "path:/objects|format:sha1|full".to_string(),
        resolved_at_ms: 1,
    };
    let text = anchor.serialize();
    assert!(BaselineAnchor::parse(&text).is_ok());

    assert!(BaselineAnchor::parse(&text.replace(&anchor.id, &"a".repeat(39))).is_err());
    assert!(BaselineAnchor::parse(&text.replace(&anchor.id, &"g".repeat(40))).is_err());
    assert!(
        BaselineAnchor::parse(&text.replace("baseline-anchor v1", "baseline-anchor v2")).is_err()
    );
    assert!(BaselineAnchor::parse(&text.replace("spec branch main tip", "spec ")).is_err());
    assert!(BaselineAnchor::parse(&text.replace("resolved_at_ms 1", "resolved_at_ms -1")).is_err());
}

#[test]
fn corrupt_anchor_is_an_error_for_validation_and_total_for_hot_paths() {
    let (repo, _second, _third) = make_user_repo();
    let store_dir = tempfile::tempdir().expect("store dir");
    let store = GitStore::init(repo.path(), &store_dir.path().join("repo.git"))
        .expect("store bound to fixture repo");
    assert!(matches!(
        store.ensure_baseline(&borrow_main()).unwrap(),
        BaselineSeedOutcome::Seeded(_)
    ));
    fs::write(store.anchor_path(), "baseline-anchor v1\nid abc\n").unwrap();
    assert!(matches!(
        store.validate_baseline(),
        Err(GitStoreError::Baseline(_))
    ));
    assert!(store.baseline_anchor_id().is_none());
    assert!(store.baseline_anchor_oid().is_none());
}
