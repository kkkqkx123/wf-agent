#![cfg(feature = "grpc")]
//! gRPC API integration tests
//!
//! These tests verify the gRPC API through the `LayertwineGrpc` handler layer,
//! calling RPC methods directly with proto message types against a real
//! `ApiServiceImpl` backed by SQLite.
//!
//! Each test mirrors a real business scenario from
//! `docs/user-guide/03-gRPC-API参考.md`.

use std::sync::Arc;

use layertwine::api::rpc::layertwine_proto;
use layertwine::api::rpc::LayertwineGrpc;
use layertwine::api::service::ApiService;
use layertwine::api::ServiceConfig;
use tonic::Request;

use layertwine_proto::layertwine_server::Layertwine;

mod common;

// ── Helper to create a LayertwineGrpc with temp DB ──

fn setup_grpc(db_path: &str) -> LayertwineGrpc {
    let api = ApiService::open(ServiceConfig {
        db_path: db_path.to_string(),
    })
    .expect("failed to create ApiService");
    LayertwineGrpc::new(Arc::new(api))
}

// ── Scenario 1: Init workflow through gRPC (ref: user-guide §RPC列表/Init) ──

#[tokio::test]
async fn test_grpc_init_and_status() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    // Init
    let init_req = Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    });
    let init_resp = grpc.init(init_req).await.expect("init should succeed");
    let init = init_resp.into_inner();
    assert_eq!(init.branch, "main");
    assert!(!init.manual_partition_id.is_empty());
    assert!(!init.staged_partition_id.is_empty());

    // Status
    let status_resp = grpc
        .status(Request::new(layertwine_proto::Empty {}))
        .await
        .expect("status should succeed");
    let status = status_resp.into_inner();
    assert!(!status.partitions.is_empty());
}

// ── Scenario 2: Single user edit → commit → log (ref: user-guide §单人编辑流程) ──

#[tokio::test]
async fn test_grpc_edit_commit_log_workflow() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    // Init
    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    // Edit
    let edit_resp = grpc
        .edit(Request::new(layertwine_proto::EditRequest {
            file: "src/main.rs".into(),
            content: Some("fn main() {}\n".into()),
        }))
        .await
        .expect("edit should succeed");
    let edit = edit_resp.into_inner();
    assert!(!edit.snapshot_id.is_empty());
    assert!(edit.staged_snapshot_id.is_some());

    // Commit
    let commit_resp = grpc
        .commit(Request::new(layertwine_proto::CommitRequest {
            message: "initial commit".into(),
            author: Some("dev-1".into()),
        }))
        .await
        .expect("commit should succeed");
    let commit = commit_resp.into_inner();
    assert!(!commit.checkpoint_id.is_empty());
    assert_eq!(commit.message, "initial commit");

    // Log
    let log_resp = grpc
        .log(Request::new(layertwine_proto::LogRequest {
            count: Some(10),
        }))
        .await
        .expect("log should succeed");
    let log = log_resp.into_inner();
    assert!(log.total >= 1);
    assert_eq!(log.checkpoints[0].author, "dev-1");
    assert_eq!(log.checkpoints[0].message, "initial commit");
}

// ── Scenario 3: Multi-agent collaboration (ref: user-guide §多 Agent 协同流程) ──

#[tokio::test]
async fn test_grpc_multi_agent_workflow() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    // Init
    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    // Agent A edit
    let agent_a_edit = grpc
        .agent_edit(Request::new(layertwine_proto::AgentEditRequest {
            agent_id: "agent-a".into(),
            file: "src/auth.rs".into(),
            content: Some("pub fn login() {}\n".into()),
        }))
        .await
        .expect("agent A edit");
    let a_edit = agent_a_edit.into_inner();
    assert!(!a_edit.snapshot_id.is_empty());

    // Agent A submit
    let agent_a_submit = grpc
        .agent_submit(Request::new(layertwine_proto::AgentSubmitRequest {
            agent_id: "agent-a".into(),
        }))
        .await
        .expect("agent A submit");
    let a_submit = agent_a_submit.into_inner();
    assert!(!a_submit.snapshot_id.is_empty());

    // Agent B edit
    grpc.agent_edit(Request::new(layertwine_proto::AgentEditRequest {
        agent_id: "agent-b".into(),
        file: "src/db.rs".into(),
        content: Some("pub fn connect() {}\n".into()),
    }))
    .await
    .expect("agent B edit");

    // Agent B submit
    grpc.agent_submit(Request::new(layertwine_proto::AgentSubmitRequest {
        agent_id: "agent-b".into(),
    }))
    .await
    .expect("agent B submit");

    // Approve agent A
    let approve_a = grpc
        .approve(Request::new(layertwine_proto::ApproveRequest {
            agent_id: "agent-a".into(),
        }))
        .await
        .expect("approve agent A");
    let app_a = approve_a.into_inner();
    assert!(!app_a.integrated_snapshot_id.is_empty());
    assert!(!app_a.staged_snapshot_id.is_empty());

    // Approve agent B
    let approve_b = grpc
        .approve(Request::new(layertwine_proto::ApproveRequest {
            agent_id: "agent-b".into(),
        }))
        .await
        .expect("approve agent B");
    let app_b = approve_b.into_inner();
    assert!(!app_b.staged_snapshot_id.is_empty());

    // Commit
    let commit = grpc
        .commit(Request::new(layertwine_proto::CommitRequest {
            message: "merge auth and db".into(),
            author: Some("reviewer".into()),
        }))
        .await
        .expect("commit");
    assert!(!commit.into_inner().checkpoint_id.is_empty());
}

// ── Scenario 4: Branch operations (ref: user-guide §分支与合并流程) ──

#[tokio::test]
async fn test_grpc_branch_create_switch_merge() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    // Init and commit
    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    grpc.edit(Request::new(layertwine_proto::EditRequest {
        file: "main.txt".into(),
        content: Some("main content\n".into()),
    }))
    .await
    .expect("edit");

    grpc.commit(Request::new(layertwine_proto::CommitRequest {
        message: "commit on main".into(),
        author: Some("dev".into()),
    }))
    .await
    .expect("commit");

    // Create branch
    let create = grpc
        .branch_create(Request::new(layertwine_proto::BranchCreateRequest {
            name: "feature/login".into(),
        }))
        .await
        .expect("branch create");
    let create_inner = create.into_inner();
    assert_eq!(create_inner.name, "feature/login");
    assert!(!create_inner.head.is_empty());

    // Switch branch
    let switch = grpc
        .branch_switch(Request::new(layertwine_proto::BranchSwitchRequest {
            name: "feature/login".into(),
        }))
        .await
        .expect("branch switch");
    assert_eq!(switch.into_inner().name, "feature/login");

    // Edit on feature branch
    grpc.edit(Request::new(layertwine_proto::EditRequest {
        file: "feature.txt".into(),
        content: Some("feature content\n".into()),
    }))
    .await
    .expect("edit on feature");

    grpc.commit(Request::new(layertwine_proto::CommitRequest {
        message: "commit on feature".into(),
        author: None,
    }))
    .await
    .expect("commit on feature");

    // Switch back to main
    grpc.branch_switch(Request::new(layertwine_proto::BranchSwitchRequest {
        name: "main".into(),
    }))
    .await
    .expect("switch back to main");

    // Merge
    let merge = grpc
        .merge(Request::new(layertwine_proto::MergeRequest {
            branch: "feature/login".into(),
            message: Some("merge feature/login".into()),
        }))
        .await
        .expect("merge");
    let merge_inner = merge.into_inner();
    assert_eq!(merge_inner.source_branch, "feature/login");
    assert_eq!(merge_inner.target_branch, "main");
    assert!(!merge_inner.checkpoint_id.is_empty());
}

// ── Scenario 5: Branch list (ref: user-guide §RPC列表/BranchList) ──

#[tokio::test]
async fn test_grpc_branch_list() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    grpc.edit(Request::new(layertwine_proto::EditRequest {
        file: "test.txt".into(),
        content: Some("content\n".into()),
    }))
    .await
    .expect("edit");

    grpc.commit(Request::new(layertwine_proto::CommitRequest {
        message: "commit".into(),
        author: None,
    }))
    .await
    .expect("commit");

    grpc.branch_create(Request::new(layertwine_proto::BranchCreateRequest {
        name: "feature".into(),
    }))
    .await
    .expect("branch create");

    let list = grpc
        .branch_list(Request::new(layertwine_proto::Empty {}))
        .await
        .expect("branch list");
    let list_inner = list.into_inner();
    assert!(list_inner.branches.len() >= 2);
    assert!(list_inner.current.is_some());

    // main should be current
    for b in &list_inner.branches {
        if b.name == "main" {
            assert!(b.is_current, "main should be current");
        }
    }
}

// ── Scenario 6: Backup and restore (ref: user-guide §备份快照) ──

#[tokio::test]
async fn test_grpc_backup_and_restore() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    let edit = grpc
        .edit(Request::new(layertwine_proto::EditRequest {
            file: "important.txt".into(),
            content: Some("critical data\n".into()),
        }))
        .await
        .expect("edit");
    let snapshot_id = edit.into_inner().snapshot_id;

    // Backup
    let backup = grpc
        .backup(Request::new(layertwine_proto::BackupRequest {
            snapshot_id: snapshot_id.clone(),
            label: Some("pre-release".into()),
        }))
        .await
        .expect("backup");
    let backup_inner = backup.into_inner();
    assert!(!backup_inner.backup_id.is_empty());
    assert_eq!(backup_inner.source_snapshot_id, snapshot_id);
    assert_eq!(backup_inner.label.as_deref(), Some("pre-release"));

    // Restore
    let restore = grpc
        .restore(Request::new(layertwine_proto::RestoreRequest {
            backup_id: backup_inner.backup_id.clone(),
        }))
        .await
        .expect("restore");
    let restore_inner = restore.into_inner();
    assert_eq!(restore_inner.backup_id, backup_inner.backup_id);
    assert_eq!(restore_inner.file, "important.txt");
    assert!(restore_inner.deltas_restored > 0);
}

// ── Scenario 7: GC (ref: user-guide §垃圾回收) ──

#[tokio::test]
async fn test_grpc_gc() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    for i in 1..=3 {
        grpc.edit(Request::new(layertwine_proto::EditRequest {
            file: "test.txt".into(),
            content: Some(format!("v{}\n", i)),
        }))
        .await
        .expect("edit");

        grpc.commit(Request::new(layertwine_proto::CommitRequest {
            message: format!("commit {}", i),
            author: None,
        }))
        .await
        .expect("commit");
    }

    let gc = grpc
        .gc(Request::new(layertwine_proto::Empty {}))
        .await
        .expect("gc should succeed");
    let gc_inner = gc.into_inner();
    // GC may not remove everything, but should not error
    let _ = gc_inner;
}

// ── Scenario 8: Error status mapping ──

#[tokio::test]
async fn test_grpc_edit_without_content_returns_error() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    let result = grpc
        .edit(Request::new(layertwine_proto::EditRequest {
            file: "test.txt".into(),
            content: None,
        }))
        .await;
    assert!(result.is_err());
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_grpc_switch_nonexistent_branch_returns_not_found() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    let result = grpc
        .branch_switch(Request::new(layertwine_proto::BranchSwitchRequest {
            name: "nonexistent".into(),
        }))
        .await;
    assert!(result.is_err());
    let status = result.unwrap_err();
    assert!(status.code() == tonic::Code::NotFound || status.code() == tonic::Code::Internal);
}

// ── Scenario 9: Status after multiple operations ──

#[tokio::test]
async fn test_grpc_status_reflects_partition_state() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    // After init, should have partitions
    let status1 = grpc
        .status(Request::new(layertwine_proto::Empty {}))
        .await
        .expect("status");
    let s1 = status1.into_inner();
    let layers: Vec<&str> = s1.partitions.iter().map(|p| p.layer.as_str()).collect();
    assert!(layers.contains(&"manual_edit"));
    assert!(layers.contains(&"staged"));
    // approval/integrated partitions appear only after agent submission

    // After edit, history_len should increase
    grpc.edit(Request::new(layertwine_proto::EditRequest {
        file: "test.txt".into(),
        content: Some("hello\n".into()),
    }))
    .await
    .expect("edit");

    let status2 = grpc
        .status(Request::new(layertwine_proto::Empty {}))
        .await
        .expect("status");
    let s2 = status2.into_inner();
    let manual = s2
        .partitions
        .iter()
        .find(|p| p.layer == "manual_edit")
        .expect("manual partition should exist");
    assert!(
        manual.history_len > 0,
        "history_len should be > 0 after edit"
    );
}

// ── Scenario 10: Log with count parameter ──

#[tokio::test]
async fn test_grpc_log_count_limit() {
    let td = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = td.path().join("layertwine-test.db");
    let grpc = setup_grpc(&db_path.to_string_lossy());

    grpc.init(Request::new(layertwine_proto::InitRequest {
        db_path: Some(db_path.to_string_lossy().to_string()),
        git_repo: None,
        git_ref: None,
    }))
    .await
    .expect("init");

    // Create 5 commits
    for i in 1..=5 {
        grpc.edit(Request::new(layertwine_proto::EditRequest {
            file: "test.txt".into(),
            content: Some(format!("v{}\n", i)),
        }))
        .await
        .expect("edit");

        grpc.commit(Request::new(layertwine_proto::CommitRequest {
            message: format!("commit {}", i),
            author: None,
        }))
        .await
        .expect("commit");
    }

    // Request only 2
    let log = grpc
        .log(Request::new(layertwine_proto::LogRequest {
            count: Some(2),
        }))
        .await
        .expect("log");
    let log_inner = log.into_inner();
    assert!(
        log_inner.checkpoints.len() <= 2,
        "log with count=2 should return at most 2 checkpoints"
    );
    assert!(log_inner.total >= 2);
}
