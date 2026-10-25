use super::*;
use crate::file::util::sha256_hex;

fn write(root: &Path, rel: &str, content: &[u8]) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn make_scanner(custom: Vec<&str>, behavior: FailureBehavior) -> WorkspaceScanner {
    WorkspaceScanner::new(ScanConfig {
        custom_ignore_patterns: custom.into_iter().map(String::from).collect(),
        failure_behavior: behavior,
        ..ScanConfig::default()
    })
}

#[test]
fn scan_finds_files_and_hashes_them() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"hello a");
    write(dir.path(), "sub/b.txt", b"hello b");

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    let mut paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    paths.sort();
    assert_eq!(paths, vec!["a.txt".to_string(), "sub/b.txt".to_string()]);
    assert_eq!(scan.files[0].hash, sha256_hex(b"hello a"));
    assert_eq!(scan.files[0].size, 7);
}

#[test]
fn scan_skips_hardcoded_ignore_dirs_at_any_depth() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "keep.txt", b"keep");
    write(dir.path(), ".git/config", b"git");
    write(dir.path(), "node_modules/lib/x.js", b"js");
    write(dir.path(), "a/node_modules/y.js", b"y");

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    let paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, vec!["keep.txt".to_string()]);
    assert!(!scan.dirs.contains(&".git".to_string()));
    assert!(!scan.dirs.contains(&"node_modules".to_string()));
}

#[test]
fn scan_applies_gitignore_with_directory_prefix() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "keep.txt", b"keep");
    write(dir.path(), ".gitignore", b"build/\n*.log\n");
    write(dir.path(), "build/out.o", b"obj");
    write(dir.path(), "a/build/x.o", b"x");
    write(dir.path(), "sub/debug.log", b"log");
    write(dir.path(), "sub/app.rs", b"rs");

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    let mut paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            ".gitignore".to_string(),
            "keep.txt".to_string(),
            "sub/app.rs".to_string()
        ]
    );
}

#[test]
fn scan_applies_nested_gitignore_with_prefix_join() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "top.txt", b"top");
    write(dir.path(), "src/.gitignore", b"gen/\n");
    write(dir.path(), "src/gen/g.rs", b"g");
    write(dir.path(), "src/keep.rs", b"keep");

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    let mut paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "src/.gitignore".to_string(),
            "src/keep.rs".to_string(),
            "top.txt".to_string()
        ]
    );
}

#[test]
fn scan_applies_custom_ignore_patterns() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"a");
    write(dir.path(), "tmp/x.tmp", b"x");

    let scanner = make_scanner(vec!["tmp/"], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    let paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, vec!["a.txt".to_string()]);
}

#[test]
fn scan_detects_empty_dirs() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a/b/c.txt", b"c");
    write(dir.path(), "d/e/f.txt", b"f");
    std::fs::create_dir_all(dir.path().join("empty/nested")).unwrap();

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    let mut empty: Vec<_> = scan.empty_dirs.clone();
    empty.sort();
    assert_eq!(empty, vec!["empty".to_string(), "empty/nested".to_string()]);
    assert!(!scan.empty_dirs.contains(&"a".to_string()));
}

#[test]
fn scan_ignore_behavior_skips_unreadable_files() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"a");
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.path().join("missing"), dir.path().join("broken.txt")).unwrap();

    for behavior in [FailureBehavior::Warn, FailureBehavior::Ignore] {
        let scanner = make_scanner(vec![], behavior);
        let scan = scanner.scan(dir.path()).unwrap();
        assert_eq!(scan.files.len(), 1);
    }
}

#[test]
fn is_ignored_matches_bare_names_at_any_depth() {
    let scanner = make_scanner(vec!["dist"], FailureBehavior::Warn);
    assert!(scanner.is_ignored("dist"));
    assert!(scanner.is_ignored("dist/out.txt"));
    assert!(scanner.is_ignored("a/b/dist"));
    assert!(scanner.is_ignored("a/b/dist/x.txt"));
    assert!(!scanner.is_ignored("a/dist-x/y.txt"));
}

#[test]
fn hardcoded_ignored_protects_git_and_node_modules() {
    assert!(is_hardcoded_ignored(".git"));
    assert!(is_hardcoded_ignored(".git/config"));
    assert!(is_hardcoded_ignored("a/node_modules/x.js"));
    assert!(!is_hardcoded_ignored("src/main.rs"));
}

#[test]
fn volatile_wf_dirs_are_ignored_but_tracked_roots_are_not() {
    assert!(is_volatile_wf_path(".wf/meta"));
    assert!(is_volatile_wf_path(".wf/meta/session.json"));
    assert!(is_volatile_wf_path(".wf/tmp/out.txt"));
    assert!(!is_volatile_wf_path(".wf"));
    assert!(!is_volatile_wf_path(".wf/config.toml"));
    assert!(!is_volatile_wf_path(".wf/mcp.json"));
    assert!(!is_volatile_wf_path("src/.wf/meta/x"));
    assert!(is_hardcoded_ignored(".wf/meta/session.json"));
    assert!(is_hardcoded_ignored(".wf/tmp"));
    assert!(!is_hardcoded_ignored(".wf/config.toml"));

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    assert!(scanner.is_ignored(".wf/meta/session.json"));
    assert!(scanner.is_ignored(".wf/tmp/out.txt"));
    assert!(!scanner.is_ignored(".wf/config.toml"));
}

#[test]
fn scan_skips_volatile_wf_subtrees() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".wf/config.toml", b"config");
    write(dir.path(), ".wf/meta/session.json", b"meta");
    write(dir.path(), ".wf/tmp/out.txt", b"tmp");
    write(dir.path(), "keep.txt", b"keep");

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();
    let mut paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![".wf/config.toml".to_string(), "keep.txt".to_string()]
    );
}

#[test]
fn negation_reincludes_previously_ignored_path() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "keep.log", b"keep");
    write(dir.path(), "skip.log", b"skip");

    let scanner = make_scanner(vec!["*.log", "!keep.log"], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();
    let paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, vec!["keep.log".to_string()]);
}

#[test]
fn directory_prefix_ignores_nested_files() {
    let scanner = make_scanner(vec!["build/"], FailureBehavior::Warn);
    assert!(scanner.is_ignored("build/out.o"));
    assert!(scanner.is_ignored("a/build/x.o"));
}

const NESTED_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Build a fake nested repository at `rel` with `.git` as a directory.
fn write_nested_dir_repo(root: &Path, rel: &str) {
    write(root, &format!("{rel}/.git/HEAD"), b"ref: refs/heads/main\n");
    write(
        root,
        &format!("{rel}/.git/refs/heads/main"),
        format!("{NESTED_SHA}\n").as_bytes(),
    );
    write(root, &format!("{rel}/inner.txt"), b"nested content");
}

#[test]
fn scan_skips_nested_repo_contents_and_records_pointer() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"outer");
    write_nested_dir_repo(dir.path(), "nested/sub");

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    let paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, vec!["a.txt".to_string()], "nested content is opaque");
    assert_eq!(scan.nested_repos.len(), 1);
    let nested = &scan.nested_repos[0];
    assert_eq!(nested.path, "nested/sub");
    assert_eq!(nested.head_commit.as_deref(), Some(NESTED_SHA));
    assert!(!nested.is_gitfile);
}

#[test]
fn scan_detects_gitfile_submodule_form() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "app/.git", b"gitdir: .git-real\n");
    write(dir.path(), "app/.git-real/HEAD", b"ref: refs/heads/main\n");
    write(
        dir.path(),
        "app/.git-real/refs/heads/main",
        format!("{NESTED_SHA}\n").as_bytes(),
    );
    write(dir.path(), "app/code.rs", b"fn main() {}");

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    let paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, Vec::<String>::new(), "submodule worktree is opaque");
    assert_eq!(scan.nested_repos.len(), 1);
    let nested = &scan.nested_repos[0];
    assert_eq!(nested.path, "app");
    assert_eq!(nested.head_commit.as_deref(), Some(NESTED_SHA));
    assert!(nested.is_gitfile);
}

#[test]
fn scan_does_not_fold_nested_gitignore_into_outer_stack() {
    let dir = tempfile::tempdir().unwrap();
    write_nested_dir_repo(dir.path(), "vendor/lib");
    write(dir.path(), "vendor/lib/.gitignore", b"**/outer.txt\n");
    write(dir.path(), "outer.txt", b"outer");

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    let paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, vec!["outer.txt".to_string()]);
}

#[test]
fn scan_detached_nested_head_resolves_directly() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "repo/.git/HEAD",
        format!("{NESTED_SHA}\n").as_bytes(),
    );

    let scanner = make_scanner(vec![], FailureBehavior::Warn);
    let scan = scanner.scan(dir.path()).unwrap();

    assert_eq!(scan.nested_repos.len(), 1);
    assert_eq!(
        scan.nested_repos[0].head_commit.as_deref(),
        Some(NESTED_SHA)
    );
}

#[test]
fn scan_records_oversized_without_hashing() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "small.txt", b"tiny");
    write(dir.path(), "big.bin", &[b'x'; 100]);

    let scanner = WorkspaceScanner::new(ScanConfig {
        custom_ignore_patterns: Vec::new(),
        failure_behavior: FailureBehavior::Warn,
        max_file_size: 10,
    });
    let scan = scanner.scan(dir.path()).unwrap();
    let big = scan.files.iter().find(|f| f.path == "big.bin").unwrap();
    assert!(big.oversized);
    assert!(big.hash.is_empty());
    assert_eq!(big.size, 100);
    assert_eq!(scan.oversized, vec!["big.bin".to_string()]);
    let small = scan.files.iter().find(|f| f.path == "small.txt").unwrap();
    assert!(!small.oversized);
    assert!(!small.hash.is_empty());

    // A second incremental scan keeps the marker without reading bytes.
    let baseline: HashMap<String, FileState> = scan
        .files
        .iter()
        .map(|f| (f.path.clone(), f.clone()))
        .collect();
    let rescan = scanner.scan_with_baseline(dir.path(), &baseline).unwrap();
    let big_again = rescan.files.iter().find(|f| f.path == "big.bin").unwrap();
    assert!(big_again.oversized);
    assert!(big_again.hash.is_empty());
}

#[test]
fn filestate_deserializes_without_oversized_flag() {
    let legacy = r#"{"path":"a.txt","hash":"ab","size":2,"last_modified":0}"#;
    let state: FileState = serde_json::from_str(legacy).unwrap();
    assert!(!state.oversized);
    assert!(!state.deleted);
}

fn init_git_repo(dir: &Path) {
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()
        .expect("git executable must exist for ignore-delegation tests");
    assert!(status.success());
}

#[test]
fn user_ignored_paths_delegates_to_user_git() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    write(dir.path(), ".gitignore", b"*.log\n");
    let ignored = crate::git_store::external::user_ignored_paths(
        dir.path(),
        &["keep.txt".to_string(), "skip.log".to_string()],
    )
    .expect("check-ignore answers in a fresh repo");
    assert!(!ignored.contains("keep.txt"));
    assert!(ignored.contains("skip.log"));
}

#[test]
fn scanner_user_repo_root_filters_delegated_ignores() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    write(dir.path(), ".gitignore", b"*.log\n");
    write(dir.path(), "keep.txt", b"keep");
    write(dir.path(), "skip.log", b"log");

    let scanner = make_scanner(vec![], FailureBehavior::Warn)
        .with_user_repo_root(crate::git_store::external::discover_repo_root(dir.path()));
    let scan = scanner.scan(dir.path()).unwrap();
    let paths: Vec<_> = scan.files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"keep.txt"));
    assert!(!paths.contains(&"skip.log"));
}

#[test]
fn user_ignore_delegation_is_best_effort_outside_repos() {
    assert!(crate::git_store::external::discover_repo_root(Path::new("/proc")).is_none());
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"a");
    let scanner = make_scanner(vec![], FailureBehavior::Warn)
        .with_user_repo_root(Some(dir.path().join("missing")));
    let scan = scanner.scan(dir.path()).unwrap();
    assert_eq!(scan.files.len(), 1);
}
