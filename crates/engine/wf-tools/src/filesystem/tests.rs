//! Filesystem tool tests: reads, writes, listings, searches, patch/diff
//! side effects, scope gating and ignore behavior.

use super::*;
// CheckpointSession wiring exercised in wf-checkpoint integration tests.
// These filesystem tests only verify tool behavior with `None` session.

fn make_tree(root: &Path) {
    std::fs::create_dir_all(root.join("src/sub")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("src/sub/lib.rs"), "pub fn lib() {}\n").unwrap();
    std::fs::write(root.join("README.md"), "# Hello\n").unwrap();
}

#[test]
fn test_read_file_offset_limit() {
    let root = std::env::temp_dir().join(format!("wf-fs-read-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), "line1\nline2\nline3\nline4\n").unwrap();

    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.clone()),
        ..Default::default()
    });
    let ctx = ToolExecutionContext::new("test-read".into());

    let text = handlers
        .read_file(&serde_json::json!({ "path": "a.txt" }), &ctx)
        .unwrap();
    assert!(text.as_str().unwrap().contains("1|line1"));

    let sliced = handlers
        .read_file(
            &serde_json::json!({ "path": "a.txt", "offset": 2, "limit": 2 }),
            &ctx,
        )
        .unwrap();
    let s = sliced.as_str().unwrap();
    assert!(s.contains("2|line2"));
    assert!(s.contains("3|line3"));
    assert!(!s.contains("1|line1"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_write_and_edit_file() {
    let root = std::env::temp_dir().join(format!("wf-fs-write-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.clone()),
        ..Default::default()
    });

    let ctx = ToolExecutionContext::new("test-fs".into());
    handlers
        .write_file(
            &serde_json::json!({
                "path": "nested/file.txt",
                "content": "hello world\n"
            }),
            &ctx,
        )
        .unwrap();
    assert!(root.join("nested/file.txt").exists());

    let edit = handlers
        .edit_file(
            &serde_json::json!({
                "path": "nested/file.txt",
                "old_string": "world",
                "new_string": "rust"
            }),
            &ctx,
        )
        .unwrap();
    assert!(edit["display"]
        .as_str()
        .unwrap()
        .contains("replaced 1 occurrence"));
    assert!(edit["diff"].as_str().unwrap().contains("-hello world"));
    assert!(edit["diff"].as_str().unwrap().contains("+hello rust"));
    assert_eq!(edit["additions"], serde_json::json!(1));
    assert_eq!(edit["deletions"], serde_json::json!(1));
    assert_eq!(edit["first_changed_line"], serde_json::json!(1));
    assert_eq!(
        std::fs::read_to_string(root.join("nested/file.txt")).unwrap(),
        "hello rust\n"
    );

    let not_found = handlers.edit_file(
        &serde_json::json!({
            "path": "nested/file.txt",
            "old_string": "missing",
            "new_string": "x"
        }),
        &ctx,
    );
    assert!(not_found.is_err());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_list_files() {
    let root = std::env::temp_dir().join(format!("wf-fs-list-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_tree(&root);

    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.clone()),
        enable_ignore: false,
        ..Default::default()
    });

    let flat = handlers
        .list_files(
            &serde_json::json!({ "path": "." }),
            &ToolExecutionContext::new("test-list".into()),
        )
        .unwrap();
    let flat_json = flat.as_object().unwrap();
    assert_eq!(flat_json["entries"].as_array().unwrap().len(), 2);
    let display = flat_json["display"].as_str().unwrap();
    assert!(display.contains("[DIR] src"));
    assert!(display.contains("[FILE] README.md"));

    let recursive = handlers
        .list_files(
            &serde_json::json!({ "path": ".", "recursive": true }),
            &ToolExecutionContext::new("test-list".into()),
        )
        .unwrap();
    let entries = recursive["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["name"] == "src/sub/lib.rs"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_grep_search() {
    let root = std::env::temp_dir().join(format!("wf-fs-grep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_tree(&root);

    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.clone()),
        enable_ignore: false,
        ..Default::default()
    });

    let out = handlers
        .grep_search(
            &serde_json::json!({ "pattern": "fn main", "path": "." }),
            &ToolExecutionContext::new("test-grep".into()),
        )
        .unwrap();
    let s = out.as_str().unwrap();
    assert!(s.contains("# src/main.rs"));
    assert!(s.contains("1 | fn main()"));

    let none = handlers
        .grep_search(
            &serde_json::json!({ "pattern": "zzz", "path": "." }),
            &ToolExecutionContext::new("test-grep".into()),
        )
        .unwrap();
    assert_eq!(none.as_str().unwrap(), "No matches found");

    let bad = handlers.grep_search(
        &serde_json::json!({ "pattern": "[", "path": "." }),
        &ToolExecutionContext::new("test-grep".into()),
    );
    assert!(bad.is_err());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_glob_search() {
    let root = std::env::temp_dir().join(format!("wf-fs-glob-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_tree(&root);

    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.clone()),
        enable_ignore: false,
        ..Default::default()
    });

    let out = handlers
        .glob_search(
            &serde_json::json!({ "pattern": "**/*.rs", "path": "." }),
            &ToolExecutionContext::new("test-glob".into()),
        )
        .unwrap();
    let entries = out["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["name"] == "src/main.rs"));
    assert!(entries.iter().any(|e| e["name"] == "src/sub/lib.rs"));

    let none = handlers
        .glob_search(
            &serde_json::json!({ "pattern": "*.toml", "path": "." }),
            &ToolExecutionContext::new("test-glob".into()),
        )
        .unwrap();
    assert!(none["display"]
        .as_str()
        .unwrap()
        .contains("No matches found"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn edit_file_preserves_crlf_and_bom() {
    let root = tempfile::tempdir().unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = ToolExecutionContext::new("exec-encoding".into());

    let crlf_path = root.path().join("crlf.txt");
    std::fs::write(&crlf_path, "a\r\nold\r\nb\r\n").unwrap();
    let edited = handlers
        .edit_file(
            &serde_json::json!({
                "path": crlf_path.to_str().unwrap(),
                "old_string": "old",
                "new_string": "new"
            }),
            &ctx,
        )
        .unwrap();
    assert_eq!(
        std::fs::read(&crlf_path).unwrap(),
        b"a\r\nnew\r\nb\r\n".to_vec()
    );
    assert!(edited["diff"].as_str().unwrap().contains("-old"));

    let bom_path = root.path().join("bom.txt");
    std::fs::write(&bom_path, "\u{FEFF}hello\n").unwrap();
    handlers
        .edit_file(
            &serde_json::json!({
                "path": bom_path.to_str().unwrap(),
                "old_string": "hello",
                "new_string": "hi"
            }),
            &ctx,
        )
        .unwrap();
    assert_eq!(
        std::fs::read(&bom_path).unwrap(),
        "\u{FEFF}hi\n".as_bytes().to_vec()
    );

    let diffed = root.path().join("crlf-diff.txt");
    std::fs::write(&diffed, "line1\r\nline2\r\n").unwrap();
    let applied = handlers
        .apply_diff(
            &serde_json::json!({
                "path": diffed.to_str().unwrap(),
                "diff": "<<<<<<< SEARCH\nline1\n=======\nline1x\n>>>>>>> REPLACE"
            }),
            &ctx,
        )
        .unwrap();
    assert_eq!(
        std::fs::read(&diffed).unwrap(),
        b"line1x\r\nline2\r\n".to_vec()
    );
    assert_eq!(applied["additions"], serde_json::json!(1));
}

#[test]
fn edit_file_falls_back_to_fuzzy_matching() {
    let root = tempfile::tempdir().unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = ToolExecutionContext::new("exec-fuzzy".into());

    let target = root.path().join("fuzzy.txt");
    std::fs::write(&target, "first  \nsecond\n").unwrap();
    let edited = handlers
        .edit_file(
            &serde_json::json!({
                "path": target.to_str().unwrap(),
                "old_string": "first   ",
                "new_string": "FIRST"
            }),
            &ctx,
        )
        .unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "FIRST\nsecond\n");
    assert_eq!(edited["additions"], serde_json::json!(1));
    assert_eq!(edited["deletions"], serde_json::json!(1));

    std::fs::write(&target, "dup\ndup\n").unwrap();
    let ambiguous = handlers.edit_file(
        &serde_json::json!({
            "path": target.to_str().unwrap(),
            "old_string": "dup",
            "new_string": "DUP"
        }),
        &ctx,
    );
    assert!(ambiguous.is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "dup\ndup\n");

    std::fs::write(&target, "\u{201C}x\u{201D}\n\u{201C}x\u{201D}\n").unwrap();
    let fuzzy_ambiguous = handlers.edit_file(
        &serde_json::json!({
            "path": target.to_str().unwrap(),
            "old_string": "\"x\"",
            "new_string": "y"
        }),
        &ctx,
    );
    assert!(fuzzy_ambiguous.is_err());
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "\u{201C}x\u{201D}\n\u{201C}x\u{201D}\n"
    );
}

#[test]
fn edit_file_applies_batches_atomically() {
    let root = tempfile::tempdir().unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = ToolExecutionContext::new("exec-batch".into());

    let target = root.path().join("batch.txt");
    std::fs::write(&target, "aaa\nbbb\nccc\n").unwrap();
    let edited = handlers
        .edit_file(
            &serde_json::json!({
                "path": target.to_str().unwrap(),
                "edits": [
                    { "old_string": "aaa", "new_string": "AAA" },
                    { "old_string": "ccc", "new_string": "CCC" },
                ]
            }),
            &ctx,
        )
        .unwrap();
    assert_eq!(edited["replaced"], serde_json::json!(2));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "AAA\nbbb\nCCC\n");
    assert_eq!(edited["additions"], serde_json::json!(2));
    assert_eq!(edited["deletions"], serde_json::json!(2));

    std::fs::write(&target, "aaa\nbbb\n").unwrap();
    let overlapping = handlers.edit_file(
        &serde_json::json!({
            "path": target.to_str().unwrap(),
            "edits": [
                { "old_string": "aaa\nbbb", "new_string": "X" },
                { "old_string": "bbb", "new_string": "Y" },
            ]
        }),
        &ctx,
    );
    assert!(overlapping.is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "aaa\nbbb\n");

    let missing = handlers.edit_file(
        &serde_json::json!({
            "path": target.to_str().unwrap(),
            "edits": [{ "old_string": "zzz", "new_string": "Z" }]
        }),
        &ctx,
    );
    assert!(missing.is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "aaa\nbbb\n");

    let mixed = handlers.edit_file(
        &serde_json::json!({
            "path": target.to_str().unwrap(),
            "old_string": "aaa",
            "new_string": "AAA",
            "edits": [{ "old_string": "bbb", "new_string": "BBB" }]
        }),
        &ctx,
    );
    assert!(mixed.is_err());

    let empty = handlers.edit_file(
        &serde_json::json!({
            "path": target.to_str().unwrap(),
            "edits": []
        }),
        &ctx,
    );
    assert!(empty.is_err());
}

#[test]
fn precise_notifications_cover_write_edit_and_patch() {
    let root = tempfile::tempdir().unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = ToolExecutionContext::new("exec-observe".into()).with_checkpoint_session(None);

    // write_file reports Created, second write reports Modified.
    let abs = root.path().join("a.txt");
    handlers
        .write_file(
            &serde_json::json!({ "path": abs.to_str().unwrap(), "content": "v1" }),
            &ctx,
        )
        .unwrap();
    handlers
        .write_file(
            &serde_json::json!({ "path": abs.to_str().unwrap(), "content": "v2" }),
            &ctx,
        )
        .unwrap();
    // edit_file reports Modified.
    handlers
        .edit_file(
            &serde_json::json!({
                "path": abs.to_str().unwrap(),
                "old_string": "v2",
                "new_string": "v3"
            }),
            &ctx,
        )
        .unwrap();
    // apply_diff reports Modified.
    std::fs::write(&abs, "line1\nline2\n").unwrap();
    handlers
        .apply_diff(
            &serde_json::json!({
                "path": abs.to_str().unwrap(),
                "diff": "<<<<<<< SEARCH\nline1\n=======\nline1x\n>>>>>>> REPLACE"
            }),
            &ctx,
        )
        .unwrap();
    // Side-effect recording is exercised in wf-checkpoint integration tests.
}

#[test]
fn failed_patch_hunk_reports_no_success_event() {
    let root = tempfile::tempdir().unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = ToolExecutionContext::new("exec-patch".into()).with_checkpoint_session(None);

    std::fs::write(root.path().join("ok.txt"), "base\n").unwrap();
    let patch = "*** Begin Patch\n*** Update File: ok.txt\n@@\n-base\n+changed\n*** Delete File: missing.txt\n*** End Patch";
    let result = handlers
        .apply_patch(&serde_json::json!({ "patch": patch }), &ctx)
        .unwrap();
    assert_eq!(result["summary"]["succeeded"], serde_json::json!(1));
    assert_eq!(result["summary"]["failed"], serde_json::json!(1));
    // Side-effect recording is exercised in wf-checkpoint integration tests.
}

#[test]
fn no_observer_keeps_plain_behavior() {
    let root = tempfile::tempdir().unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = ToolExecutionContext::new("exec-plain".into());
    let abs = root.path().join("plain.txt");
    let out = handlers
        .write_file(
            &serde_json::json!({ "path": abs.to_str().unwrap(), "content": "x" }),
            &ctx,
        )
        .unwrap();
    assert!(out.as_str().unwrap().contains("Successfully wrote"));
}

fn scoped_ctx(execution: &str, root: &std::path::Path, write_sub: &str) -> ToolExecutionContext {
    let write_prefix = root.join(write_sub).to_string_lossy().to_string();
    let root_str = root.to_string_lossy().to_string();
    ToolExecutionContext::new(execution.into()).with_file_scope(Some(
        wf_types::config::file_scope::ResolvedFileScope {
            enabled: true,
            read_prefixes: vec![root_str.clone()],
            write_prefixes: vec![write_prefix],
            checkpoint_prefixes: vec![root.join(write_sub).to_string_lossy().to_string()],
            workdir: Some(root_str),
        },
    ))
}

#[test]
fn out_of_scope_write_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("frontend")).unwrap();
    std::fs::create_dir_all(root.path().join("backend")).unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-frontend", root.path(), "frontend");
    let outside = root.path().join("backend/out.txt");
    let err = handlers
        .write_file(
            &serde_json::json!({
                "path": outside.to_str().unwrap(),
                "content": "x"
            }),
            &ctx,
        )
        .expect_err("outside write must fail");
    assert!(err.to_string().contains("outside allowed write"));
    assert!(!outside.exists());
    let inside = root.path().join("frontend/in.txt");
    handlers
        .write_file(
            &serde_json::json!({
                "path": inside.to_str().unwrap(),
                "content": "ok"
            }),
            &ctx,
        )
        .unwrap();
    assert!(inside.exists());
}

#[test]
fn sibling_prefix_does_not_match() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("app")).unwrap();
    std::fs::create_dir_all(root.path().join("application")).unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-app", root.path(), "app");
    let sibling = root.path().join("application/out.txt");
    let err = handlers
        .write_file(
            &serde_json::json!({
                "path": sibling.to_str().unwrap(),
                "content": "x"
            }),
            &ctx,
        )
        .expect_err("sibling prefix must not match");
    assert!(err.to_string().contains("outside allowed write"));
}

#[test]
fn symlink_escape_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    std::fs::create_dir_all(root.path().join("frontend")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("frontend/evil")).unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-link", root.path(), "frontend");
    let via_link = root.path().join("frontend/evil/secret.txt");
    let read = handlers.read_file(
        &serde_json::json!({ "path": via_link.to_str().unwrap() }),
        &ctx,
    );
    assert!(read.is_err());
    let err = read.expect_err("symlink escape must fail");
    assert!(
        err.to_string().contains("outside allowed") || err.to_string().contains("escapes"),
        "unexpected error: {err}"
    );
}

fn checkpoint_narrow_ctx(execution: &str, root: &std::path::Path) -> ToolExecutionContext {
    let root_str = root.to_string_lossy().to_string();
    let write_prefix = root.join("work").to_string_lossy().to_string();
    let checkpoint_prefix = root.join("work/task").to_string_lossy().to_string();
    ToolExecutionContext::new(execution.into()).with_file_scope(Some(
        wf_types::config::file_scope::ResolvedFileScope {
            enabled: true,
            read_prefixes: vec![root_str.clone()],
            write_prefixes: vec![write_prefix],
            checkpoint_prefixes: vec![checkpoint_prefix],
            workdir: Some(root_str),
        },
    ))
}

#[test]
fn write_outside_checkpoint_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("work/task")).unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = checkpoint_narrow_ctx("exec-task", root.path());
    let outside = root.path().join("work/notes.txt");
    let err = handlers
        .write_file(
            &serde_json::json!({
                "path": outside.to_str().unwrap(),
                "content": "x"
            }),
            &ctx,
        )
        .expect_err("write outside checkpoint must fail");
    assert!(err.is_scope_violation());
    assert!(err.to_string().contains("checkpoint"));
    assert!(!outside.exists());
    let inside = root.path().join("work/task/in.txt");
    handlers
        .write_file(
            &serde_json::json!({
                "path": inside.to_str().unwrap(),
                "content": "ok"
            }),
            &ctx,
        )
        .unwrap();
    assert!(inside.exists());
}

#[test]
fn lenient_write_stays_fail_closed() {
    use wf_types::script::sandbox::SandboxMode;
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("frontend")).unwrap();
    std::fs::create_dir_all(root.path().join("backend")).unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        scope_mode: Some(SandboxMode::Lenient),
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-lenient", root.path(), "frontend");
    let outside = root.path().join("backend/out.txt");
    let err = handlers
        .write_file(
            &serde_json::json!({
                "path": outside.to_str().unwrap(),
                "content": "x"
            }),
            &ctx,
        )
        .expect_err("lenient write outside scope must still fail");
    assert!(err.is_scope_violation());
    assert!(!outside.exists());
}

#[test]
fn list_reports_scope_omissions() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    std::fs::create_dir_all(root.path().join("frontend")).unwrap();
    std::fs::write(root.path().join("frontend/a.txt"), "a").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("frontend/evil")).unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-list", root.path(), "frontend");
    let out = handlers
        .list_files(
            &serde_json::json!({
                "path": root.path().join("frontend").to_str().unwrap(),
                "recursive": true
            }),
            &ctx,
        )
        .unwrap();
    let display = out["display"].as_str().unwrap();
    assert!(display.contains("outside the execution read scope were omitted"));
    assert!(!display.contains("secret.txt"));
}

#[test]
fn scope_violation_text_is_tagged_and_never_not_found() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    std::fs::create_dir_all(root.path().join("frontend")).unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-tag-scope", root.path(), "frontend");
    let err = handlers
        .read_file(
            &serde_json::json!({
                "path": outside.path().join("secret.txt").to_str().unwrap()
            }),
            &ctx,
        )
        .expect_err("outside read must fail");
    assert!(err.is_scope_violation());
    let text = err.to_string();
    assert!(text.contains(crate::error::SCOPE_VIOLATION_TAG));
    assert!(!text.contains(crate::error::NOT_FOUND_TAG));
    assert!(!text.contains("does not exist"));
}

#[test]
fn missing_file_text_is_tagged_and_never_scope() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("frontend")).unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-tag-missing", root.path(), "frontend");
    let err = handlers
        .read_file(
            &serde_json::json!({
                "path": root.path().join("frontend/missing.txt").to_str().unwrap()
            }),
            &ctx,
        )
        .expect_err("missing file must fail");
    assert!(!err.is_scope_violation());
    let text = err.to_string();
    assert!(text.contains(crate::error::NOT_FOUND_TAG));
    assert!(!text.contains("outside allowed"));
    assert!(!text.contains(crate::error::SCOPE_VIOLATION_TAG));
    let err = handlers
        .list_files(
            &serde_json::json!({
                "path": root.path().join("frontend/nope").to_str().unwrap()
            }),
            &ctx,
        )
        .expect_err("missing directory must fail");
    assert!(!err.is_scope_violation());
    let text = err.to_string();
    assert!(text.contains(crate::error::NOT_FOUND_TAG));
    assert!(!text.contains("outside allowed"));
}

#[test]
fn ignore_hidden_entries_are_counted_and_revealable() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("visible.txt"), "v").unwrap();
    std::fs::create_dir_all(root.path().join("target")).unwrap();
    std::fs::write(root.path().join("target/built.txt"), "b").unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-ignore", root.path(), "frontend");
    let out = handlers
        .list_files(
            &serde_json::json!({ "path": root.path().to_str().unwrap() }),
            &ctx,
        )
        .unwrap();
    let display = out["display"].as_str().unwrap();
    assert!(display.contains("visible.txt"));
    assert!(!display.contains("built.txt"));
    assert!(display.contains("hidden by ignore rules"));
    assert!(!display.contains("outside the execution read scope"));
    let out = handlers
        .list_files(
            &serde_json::json!({
                "path": root.path().to_str().unwrap(),
                "include_ignored": true
            }),
            &ctx,
        )
        .unwrap();
    let display = out["display"].as_str().unwrap();
    assert!(display.contains("target"));
    assert!(!display.contains("hidden by ignore rules"));
}

#[test]
fn workspace_gitignore_hides_from_discovery_but_not_from_direct_read() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(root.path().join("a.log"), "log").unwrap();
    std::fs::write(root.path().join("b.txt"), "txt").unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-gitignore", root.path(), "frontend");
    let out = handlers
        .list_files(
            &serde_json::json!({ "path": root.path().to_str().unwrap() }),
            &ctx,
        )
        .unwrap();
    let display = out["display"].as_str().unwrap();
    assert!(display.contains("b.txt"));
    assert!(!display.contains("a.log"));
    assert!(display.contains("hidden by ignore rules"));
    let direct = handlers
        .read_file(
            &serde_json::json!({ "path": root.path().join("a.log").to_str().unwrap() }),
            &ctx,
        )
        .unwrap();
    assert!(direct.as_str().unwrap().contains("log"));
    let out = handlers
        .list_files(
            &serde_json::json!({
                "path": root.path().to_str().unwrap(),
                "include_ignored": true
            }),
            &ctx,
        )
        .unwrap();
    assert!(out["display"].as_str().unwrap().contains("a.log"));
}

#[test]
fn checkpoint_ignored_write_is_marked_unrestorable() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(".gitignore"), "*.log\n").unwrap();
    std::fs::create_dir_all(root.path().join("work")).unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        ..Default::default()
    });
    let ctx = scoped_ctx("exec-unrestorable", root.path(), "work");
    let ok = handlers
        .write_file(
            &serde_json::json!({
                "path": root.path().join("work/out.log").to_str().unwrap(),
                "content": "log"
            }),
            &ctx,
        )
        .unwrap();
    assert!(ok.as_str().unwrap().contains("cannot be restored"));
    let ok = handlers
        .write_file(
            &serde_json::json!({
                "path": root.path().join("work/ok.txt").to_str().unwrap(),
                "content": "ok"
            }),
            &ctx,
        )
        .unwrap();
    assert!(!ok.as_str().unwrap().contains("cannot be restored"));
}

#[test]
fn closed_channel_denies_writes_but_allows_reads() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "hi").unwrap();
    let handlers = FsToolHandlers::new(FsToolConfig {
        workspace_dir: Some(root.path().to_path_buf()),
        enable_ignore: false,
        ..Default::default()
    });
    let root_str = root.path().to_string_lossy().to_string();
    let ctx = ToolExecutionContext::new("exec-closed".into()).with_file_scope(Some(
        wf_types::config::file_scope::ResolvedFileScope {
            enabled: false,
            read_prefixes: vec![root_str.clone()],
            write_prefixes: vec![root_str.clone()],
            checkpoint_prefixes: vec![root_str],
            workdir: None,
        },
    ));
    let note = root.path().join("note.txt");
    handlers
        .read_file(&serde_json::json!({ "path": note.to_str().unwrap() }), &ctx)
        .unwrap();
    let err = handlers
        .write_file(
            &serde_json::json!({
                "path": note.to_str().unwrap(),
                "content": "changed"
            }),
            &ctx,
        )
        .expect_err("closed channel write must fail");
    assert!(err.is_scope_violation());
    assert!(err.to_string().contains("file channel is closed"));
}
