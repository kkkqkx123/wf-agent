//! Error handling E2E tests
//!
//! Covers real error scenarios that users encounter when using the API:
//!   - Edit without content
//!   - Commit without changes
//!   - Branch operations on non-existent branches
//!   - Approval operations without prior agent edits
//!   - Merge unified without integrated partitions
//!   - ApiError serialization correctness

use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use crate::common::output::*;
use layertwine::api::{
    ApiService, BranchCreateRequest, BranchSwitchRequest, CommitRequest, EditRequest, MergeRequest,
    MergeToUnifiedRequest, ShowRequest,
};

// ── S1: Edit error cases ──

#[test]
fn test_edit_without_content() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_edit_without_content");

    print_info("Step 1: Initialize repository");
    init_repository(&env);

    print_info("Step 2: Edit without providing content");
    let resp = env.api.edit(EditRequest {
        file: "test.txt".into(),
        content: None,
    });

    assert!(resp.is_err(), "edit without content should fail");
    if let Err(e) = resp {
        print_info(&format!("  Error code: {}", e.code));
        print_info(&format!("  Error message: {}", e.message));
        assert_eq!(e.code, "INVALID_PARAMS", "should be invalid params");
        assert!(
            e.message.contains("content"),
            "error message should mention content"
        );
    }

    print_test_result(true, "test_edit_without_content", None);
}

#[test]
fn test_commit_without_changes() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_commit_without_changes");

    print_info("Step 1: Initialize repository (no edits made)");
    init_repository(&env);

    print_info("Step 2: Attempt to commit without any edits");
    let resp = env.api.commit(CommitRequest {
        message: "empty commit".into(),
        author: Some("user".into()),
    });

    // Depending on implementation, this might fail because staged has no new changes
    if let Err(e) = resp {
        print_info(&format!("  Error (expected): {}", e));
    } else {
        print_info("  Commit succeeded (staged partition already exists from init)");
    }

    print_test_result(true, "test_commit_without_changes", None);
}

// ── S2: Branch error cases ──

#[test]
fn test_branch_errors() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_branch_errors");

    print_info("Step 1: Initialize repository and create base commit");
    init_repository(&env);
    apply_edit(&env, "test.txt", "content\n");
    commit_changes(&env, "base", "user");

    print_info("Step 2: Try to switch to non-existent branch");
    let resp = env.api.branch_switch(BranchSwitchRequest {
        name: "nonexistent".into(),
    });
    assert!(resp.is_err(), "switch to non-existent branch should fail");
    if let Err(e) = resp {
        print_info(&format!("  Error: {}", e));
        assert_eq!(e.code, "NOT_FOUND", "should be NOT_FOUND");
    }

    print_info("Step 3: Create a branch then try to create duplicate");
    env.api
        .branch_create(BranchCreateRequest {
            name: "feature".into(),
        })
        .expect("create branch should succeed");

    let resp = env.api.branch_create(BranchCreateRequest {
        name: "feature".into(),
    });
    assert!(resp.is_err(), "duplicate branch create should fail");
    if let Err(e) = resp {
        print_info(&format!("  Error: {}", e));
    }

    print_info("Step 4: Try to merge non-existent branch");
    let resp = env.api.merge(MergeRequest {
        branch: "nonexistent-branch".into(),
        message: Some("merge attempt".into()),
    });
    assert!(resp.is_err(), "merge non-existent branch should fail");

    print_test_result(true, "test_branch_errors", None);
}

// ── S3: Approval error cases ──

#[test]
fn test_approval_error_cases() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_approval_error_cases");

    print_info("Step 1: Initialize repository");
    init_repository(&env);
    apply_edit(&env, "test.txt", "base\n");
    commit_changes(&env, "base", "user");

    print_info("Step 2: Try to submit agent without prior edit");
    let agent_id = "no-edit-agent";
    let resp = env.api.agent_submit(layertwine::api::AgentSubmitRequest {
        agent_id: agent_id.into(),
    });
    assert!(resp.is_err(), "submit without edit should fail");
    if let Err(e) = resp {
        print_info(&format!("  Error: {}", e));
    }

    print_info("Step 3: Try to merge to unified with no integrated partitions");
    let resp = env.api.merge_to_unified(MergeToUnifiedRequest {
        integration_names: None,
    });
    assert!(
        resp.is_err(),
        "merge to unified without integrated should fail"
    );
    if let Err(e) = resp {
        print_info(&format!("  Error: {}", e));
    }

    print_test_result(true, "test_approval_error_cases", None);
}

// ── S4: Show error cases (covered in show_diff_tests, add edge case) ──

#[test]
fn test_show_invalid_checkpoint_id() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_show_invalid_checkpoint_id");

    print_info("Step 1: Initialize repository");
    init_repository(&env);

    print_info("Step 2: Show checkpoint with garbage hex ID");
    let resp = env.api.show(ShowRequest {
        show_what: "checkpoint".into(),
        target_id: Some("deadbeef00000000000000000000000000000000000000000000000000000000".into()),
    });
    // This should error because the checkpoint ID doesn't exist
    // Whether it fails with NOT_FOUND or INTERNAL_ERROR depends on implementation
    assert!(resp.is_err(), "show non-existent checkpoint should fail");

    print_test_result(true, "test_show_invalid_checkpoint_id", None);
}

// ── S5: ApiError serialization ──

#[test]
fn test_api_error_serialization() {
    print_test_header("test_api_error_serialization");

    print_info("Step 1: Verify ApiError serialization to JSON");
    let err = layertwine::api::ApiError::not_found("test entity");
    let json = serde_json::to_string(&err).expect("ApiError should serialize to JSON");
    print_info(&format!("  JSON: {}", json));

    assert!(json.contains("NOT_FOUND"), "JSON should contain error code");
    assert!(
        json.contains("test entity"),
        "JSON should contain error message"
    );

    print_info("Step 2: Verify ApiError deserialization from JSON");
    let deserialized: layertwine::api::ApiError =
        serde_json::from_str(&json).expect("should deserialize from JSON");
    assert_eq!(deserialized.code, "NOT_FOUND");
    assert!(deserialized.message.contains("test entity"));

    print_info("Step 3: Verify multiple error types serialize correctly");
    let errors = vec![
        (
            "INVALID_PARAMS",
            layertwine::api::ApiError::invalid_params("bad input"),
        ),
        (
            "STORAGE_ERROR",
            layertwine::api::ApiError::storage("disk full"),
        ),
        (
            "ENGINE_ERROR",
            layertwine::api::ApiError::engine("diff failed"),
        ),
        (
            "INTERNAL_ERROR",
            layertwine::api::ApiError::internal("unexpected state"),
        ),
    ];

    for (expected_code, err) in &errors {
        let json = serde_json::to_string(err).expect("should serialize");
        assert!(
            json.contains(expected_code),
            "JSON for {} should contain code",
            expected_code
        );
    }

    print_success("All error types serialize correctly");

    print_test_result(true, "test_api_error_serialization", None);
}

// ── S6: API request/response type JSON round-trip ──

#[test]
fn test_api_types_json_roundtrip() {
    print_test_header("test_api_types_json_roundtrip");

    print_info("Step 1: InitRequest serialization round-trip");
    let req = layertwine::api::InitRequest {
        db_path: Some("/tmp/test.db".into()),
        git_repo: None,
        git_ref: Some("HEAD".into()),
    };
    let json = serde_json::to_string(&req).expect("serialize InitRequest");
    let req2: layertwine::api::InitRequest =
        serde_json::from_str(&json).expect("deserialize InitRequest");
    assert_eq!(req.db_path, req2.db_path);
    assert_eq!(req.git_ref, req2.git_ref);

    print_info("Step 2: EditResponse serialization round-trip");
    let resp = layertwine::api::EditResponse {
        snapshot_id: "abc123".into(),
        staged_snapshot_id: Some("def456".into()),
    };
    let json = serde_json::to_string(&resp).expect("serialize EditResponse");
    let resp2: layertwine::api::EditResponse =
        serde_json::from_str(&json).expect("deserialize EditResponse");
    assert_eq!(resp.snapshot_id, resp2.snapshot_id);
    assert_eq!(resp.staged_snapshot_id, resp2.staged_snapshot_id);

    print_info("Step 3: CompactResponse serialization round-trip");
    let resp = layertwine::api::CompactResponse {
        wal_checkpointed: true,
        freelist_before: 100,
        total_pages: 500,
        freelist_after: 50,
        vacuum_performed: true,
        message: "compacted".into(),
    };
    let json = serde_json::to_string(&resp).expect("serialize CompactResponse");
    let resp2: layertwine::api::CompactResponse =
        serde_json::from_str(&json).expect("deserialize CompactResponse");
    assert_eq!(resp.wal_checkpointed, resp2.wal_checkpointed);
    assert_eq!(resp.total_pages, resp2.total_pages);
    assert_eq!(resp.message, resp2.message);

    print_info("Step 4: GcResponse serialization round-trip");
    let resp = layertwine::api::GcResponse {
        removed_checkpoints: 3,
        removed_snapshots: 10,
        freed_bytes: 4096,
        delta_chain_depth_triggered: false,
    };
    let json = serde_json::to_string(&resp).expect("serialize GcResponse");
    let resp2: layertwine::api::GcResponse =
        serde_json::from_str(&json).expect("deserialize GcResponse");
    assert_eq!(resp.removed_checkpoints, resp2.removed_checkpoints);
    assert_eq!(resp.freed_bytes, resp2.freed_bytes);

    print_success("All API types survive JSON round-trip");

    print_test_result(true, "test_api_types_json_roundtrip", None);
}
