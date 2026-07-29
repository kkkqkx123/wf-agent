//! HTTP API integration tests
//!
//! These tests verify the complete HTTP API workflows described in
//! `docs/user-guide/02-HTTP-API使用指南.md` using a real SQLite database
//! via `ApiServiceImpl`.
//!
//! Each test mirrors a real business scenario from the user guide.

use layertwine::api::{
    ApiService, ApproveRequest, BackupRequest, BranchCreateRequest, BranchSwitchRequest,
    CommitRequest, EditRequest, GcRequest, InitRequest, LogRequest, MergeRequest, RestoreRequest,
    ShowRequest,
};

mod common;

use common::fixture::{TestConfig, TestEnvironment};

// ── Scenario 1: Single user edit workflow (ref: user-guide §典型工作流程/单人编辑流程) ──

#[test]
fn test_single_user_workflow_init_edit_commit_log() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    // Step 1: Init
    let init_resp = env
        .api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");
    assert_eq!(init_resp.branch, "main");
    assert!(!init_resp.manual_partition_id.is_empty());
    assert!(!init_resp.staged_partition_id.is_empty());

    // Step 2: Edit
    let edit_resp = env
        .api
        .edit(EditRequest {
            file: "src/main.rs".into(),
            content: Some("fn main() {}\n".into()),
        })
        .expect("edit should succeed");
    assert!(!edit_resp.snapshot_id.is_empty());
    assert!(edit_resp.staged_snapshot_id.is_some());

    // Step 3: Commit
    let commit_resp = env
        .api
        .commit(CommitRequest {
            message: "initial commit".into(),
            author: Some("dev-1".into()),
        })
        .expect("commit should succeed");
    assert!(!commit_resp.checkpoint_id.is_empty());
    assert_eq!(commit_resp.message, "initial commit");

    // Step 4: Log
    let log_resp = env
        .api
        .log(LogRequest { count: Some(10) })
        .expect("log should succeed");
    assert!(log_resp.total >= 1);
    assert_eq!(log_resp.checkpoints[0].author, "dev-1");
    assert_eq!(log_resp.checkpoints[0].message, "initial commit");
}

// ── Scenario 2: Status inspection ──

#[test]
fn test_status_after_init_shows_partitions() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    let status = env.api.status().expect("status should succeed");
    assert!(!status.partitions.is_empty());
    // Should have at least manual and staged partitions
    let layers: Vec<&str> = status.partitions.iter().map(|p| p.layer.as_str()).collect();
    assert!(layers.contains(&"manual_edit"));
    assert!(layers.contains(&"staged"));
}

// ── Scenario 3: Multi-agent collaboration (ref: user-guide §多 Agent 协同流程) ──

#[test]
fn test_multi_agent_workflow_edit_submit_approve() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    // Init
    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    // Agent A: edit
    let agent_a_edit = env
        .api
        .agent_edit(layertwine::api::AgentEditRequest {
            agent_id: "agent-a".into(),
            file: "src/auth.rs".into(),
            content: Some("pub fn login() {}\n".into()),
        })
        .expect("agent A edit should succeed");
    assert!(!agent_a_edit.snapshot_id.is_empty());
    assert_eq!(agent_a_edit.staged_snapshot_id, None);

    // Agent A: submit
    let agent_a_submit = env
        .api
        .agent_submit(layertwine::api::AgentSubmitRequest {
            agent_id: "agent-a".into(),
        })
        .expect("agent A submit should succeed");
    assert!(!agent_a_submit.snapshot_id.is_empty());

    // Agent B: edit
    let agent_b_edit = env
        .api
        .agent_edit(layertwine::api::AgentEditRequest {
            agent_id: "agent-b".into(),
            file: "src/db.rs".into(),
            content: Some("pub fn connect() {}\n".into()),
        })
        .expect("agent B edit should succeed");
    assert!(!agent_b_edit.snapshot_id.is_empty());

    // Agent B: submit
    let agent_b_submit = env
        .api
        .agent_submit(layertwine::api::AgentSubmitRequest {
            agent_id: "agent-b".into(),
        })
        .expect("agent B submit should succeed");
    assert!(!agent_b_submit.snapshot_id.is_empty());

    // Approve agent A
    let approve_a = env
        .api
        .approve(ApproveRequest {
            agent_id: "agent-a".into(),
        })
        .expect("approve agent A should succeed");
    assert!(!approve_a.integrated_snapshot_id.is_empty());
    assert!(!approve_a.staged_snapshot_id.is_empty());

    // Approve agent B
    let approve_b = env
        .api
        .approve(ApproveRequest {
            agent_id: "agent-b".into(),
        })
        .expect("approve agent B should succeed");
    assert!(!approve_b.staged_snapshot_id.is_empty());

    // Final commit
    let commit = env
        .api
        .commit(CommitRequest {
            message: "merge auth and db modules".into(),
            author: Some("reviewer".into()),
        })
        .expect("final commit should succeed");
    assert!(!commit.checkpoint_id.is_empty());
}

// ── Scenario 4: Edit with empty content errors ──

#[test]
fn test_edit_without_content_returns_error() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    let result = env.api.edit(EditRequest {
        file: "test.txt".into(),
        content: None,
    });
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, "INVALID_PARAMS");
}

// ── Scenario 5: Multiple sequential edits ──

#[test]
fn test_multiple_sequential_edits() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    let edit1 = env
        .api
        .edit(EditRequest {
            file: "test.txt".into(),
            content: Some("v1\n".into()),
        })
        .expect("edit 1 should succeed");

    let edit2 = env
        .api
        .edit(EditRequest {
            file: "test.txt".into(),
            content: Some("v2\n".into()),
        })
        .expect("edit 2 should succeed");

    // Snapshot IDs should be different
    assert_ne!(edit1.snapshot_id, edit2.snapshot_id);
}

// ── Scenario 6: Branch create, switch, and merge (ref: user-guide §分支与合并流程) ──

#[test]
fn test_branch_create_switch_and_merge() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    // Init and first commit on main
    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    env.api
        .edit(EditRequest {
            file: "main.txt".into(),
            content: Some("main content\n".into()),
        })
        .expect("edit should succeed");

    env.api
        .commit(CommitRequest {
            message: "commit on main".into(),
            author: Some("dev".into()),
        })
        .expect("commit should succeed");

    // Create feature branch
    let create = env
        .api
        .branch_create(BranchCreateRequest {
            name: "feature/login".into(),
        })
        .expect("branch create should succeed");
    assert_eq!(create.name, "feature/login");
    assert!(!create.head.is_empty());

    // Switch to feature branch
    let switch = env
        .api
        .branch_switch(BranchSwitchRequest {
            name: "feature/login".into(),
        })
        .expect("branch switch should succeed");
    assert_eq!(switch.name, "feature/login");

    // Edit on feature branch
    env.api
        .edit(EditRequest {
            file: "feature.txt".into(),
            content: Some("feature content\n".into()),
        })
        .expect("edit on feature should succeed");

    env.api
        .commit(CommitRequest {
            message: "commit on feature".into(),
            author: Some("dev".into()),
        })
        .expect("commit on feature should succeed");

    // Switch back to main
    env.api
        .branch_switch(BranchSwitchRequest {
            name: "main".into(),
        })
        .expect("switch back to main should succeed");

    // Merge feature into main
    let merge = env
        .api
        .merge(MergeRequest {
            branch: "feature/login".into(),
            message: Some("merge feature/login".into()),
        })
        .expect("merge should succeed");
    assert_eq!(merge.source_branch, "feature/login");
    assert_eq!(merge.target_branch, "main");
    assert!(!merge.checkpoint_id.is_empty());
}

// ── Scenario 7: Branch list shows current branch ──

#[test]
fn test_branch_list_shows_current_and_is_current() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    env.api
        .edit(EditRequest {
            file: "test.txt".into(),
            content: Some("content\n".into()),
        })
        .expect("edit should succeed");

    env.api
        .commit(CommitRequest {
            message: "commit".into(),
            author: None,
        })
        .expect("commit should succeed");

    // Create another branch
    env.api
        .branch_create(BranchCreateRequest {
            name: "feature".into(),
        })
        .expect("branch create should succeed");

    // List branches
    let branches = env.api.branch_list().expect("branch list should succeed");
    assert!(branches.current.is_some());
    assert_eq!(branches.current.as_deref(), Some("main"));

    for b in &branches.branches {
        if b.name == "main" {
            assert!(b.is_current, "main should be current");
        } else {
            assert!(!b.is_current, "feature should not be current");
        }
    }

    // Switch to feature
    env.api
        .branch_switch(BranchSwitchRequest {
            name: "feature".into(),
        })
        .expect("switch should succeed");

    // Verify current changed
    let branches2 = env.api.branch_list().expect("branch list should succeed");
    assert_eq!(branches2.current.as_deref(), Some("feature"));
    for b in &branches2.branches {
        if b.name == "feature" {
            assert!(b.is_current, "feature should be current after switch");
        }
    }
}

// ── Scenario 8: Show staged diff ──

#[test]
fn test_show_staged_diff() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    env.api
        .edit(EditRequest {
            file: "test.txt".into(),
            content: Some("new content\n".into()),
        })
        .expect("edit should succeed");

    let show = env
        .api
        .show(ShowRequest {
            show_what: "staged".into(),
            target_id: None,
        })
        .expect("show staged should succeed");
    assert_eq!(show.target, "staged");
    assert!(!show.diffs.is_empty());
    assert_eq!(show.diffs[0].file_path, "test.txt");
    assert!(!show.diffs[0].unified_diff.is_empty());
}

// ── Scenario 9: Show checkpoint diff ──

#[test]
fn test_show_checkpoint_diff() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    env.api
        .edit(EditRequest {
            file: "test.txt".into(),
            content: Some("v1\n".into()),
        })
        .expect("edit should succeed");

    let commit = env
        .api
        .commit(CommitRequest {
            message: "first".into(),
            author: None,
        })
        .expect("commit should succeed");

    env.api
        .edit(EditRequest {
            file: "test.txt".into(),
            content: Some("v2\n".into()),
        })
        .expect("edit should succeed");

    env.api
        .commit(CommitRequest {
            message: "second".into(),
            author: None,
        })
        .expect("commit should succeed");

    let show = env
        .api
        .show(ShowRequest {
            show_what: "checkpoint".into(),
            target_id: Some(commit.checkpoint_id.clone()),
        })
        .expect("show checkpoint should succeed");
    assert!(show.target.contains("checkpoint"));
    assert!(!show.diffs.is_empty());
    assert_eq!(show.diffs[0].file_path, "test.txt");
}

// ── Scenario 10: Backup and restore ──

#[test]
fn test_backup_and_restore_workflow() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    let edit = env
        .api
        .edit(EditRequest {
            file: "important.txt".into(),
            content: Some("critical data\n".into()),
        })
        .expect("edit should succeed");

    // Backup
    let backup = env
        .api
        .backup(BackupRequest {
            snapshot_id: edit.snapshot_id.clone(),
            label: Some("pre-release baseline".into()),
        })
        .expect("backup should succeed");
    assert!(!backup.backup_id.is_empty());
    assert_eq!(backup.source_snapshot_id, edit.snapshot_id);
    assert_eq!(backup.label.as_deref(), Some("pre-release baseline"));

    // Restore
    let restore = env
        .api
        .restore(RestoreRequest {
            backup_id: backup.backup_id.clone(),
        })
        .expect("restore should succeed");
    assert_eq!(restore.backup_id, backup.backup_id);
    assert_eq!(restore.file, "important.txt");
    assert!(restore.deltas_restored > 0);
}

// ── Scenario 11: Error handling - branch not found ──

#[test]
fn test_switch_nonexistent_branch_returns_error() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    let result = env.api.branch_switch(BranchSwitchRequest {
        name: "nonexistent".into(),
    });
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.code == "NOT_FOUND" || err.code == "CHECKPOINT_ERROR",
        "expected NOT_FOUND or CHECKPOINT_ERROR, got {}",
        err.code
    );
}

// ── Scenario 12: Error handling - duplicate branch ──

#[test]
fn test_create_duplicate_branch_returns_error() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    env.api
        .edit(EditRequest {
            file: "test.txt".into(),
            content: Some("content\n".into()),
        })
        .expect("edit");

    env.api
        .commit(CommitRequest {
            message: "commit".into(),
            author: None,
        })
        .expect("commit");

    env.api
        .branch_create(BranchCreateRequest {
            name: "my-feature".into(),
        })
        .expect("first create should succeed");

    let result = env.api.branch_create(BranchCreateRequest {
        name: "my-feature".into(),
    });
    assert!(result.is_err());
}

// ── Scenario 13: Agent submit without edits errors ──

#[test]
fn test_agent_submit_without_edit_returns_error() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    let result = env.api.agent_submit(layertwine::api::AgentSubmitRequest {
        agent_id: "no-edit-agent".into(),
    });
    assert!(result.is_err());
}

// ── Scenario 14: GC after commits ──

#[test]
fn test_gc_after_commits() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    for i in 1..=3 {
        env.api
            .edit(EditRequest {
                file: "test.txt".into(),
                content: Some(format!("v{}\n", i)),
            })
            .expect("edit");

        env.api
            .commit(CommitRequest {
                message: format!("commit {}", i),
                author: None,
            })
            .expect("commit");
    }

    let gc_resp = env.api.gc(GcRequest {}).expect("gc should succeed");
    // GC may or may not remove things, but should not crash
    let _ = gc_resp;
}

// ── Scenario 15: Compact after operations ──

#[test]
fn test_compact_after_operations() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("init should succeed");

    let compact = env
        .api
        .compact(layertwine::api::CompactRequest {
            vacuum_full: Some(false),
        })
        .expect("compact should succeed");
    assert!(compact.wal_checkpointed);
}
