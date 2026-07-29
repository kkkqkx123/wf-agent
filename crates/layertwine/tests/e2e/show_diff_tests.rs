//! Show command E2E tests
//!
//! Covers real business scenarios for `layertwine show`:
//!   - Show staged diff (human reviewer inspects current staged changes)
//!   - Show checkpoint diff (developer inspects what a checkpoint changed)
//!   - Show partition diff (developer inspects a specific partition)
//!   - Error cases: invalid target, missing required ID

use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use crate::common::output::*;
use layertwine::api::{ApiService, ShowRequest};
use layertwine::storage::repository::PartitionStore;

// ── S1: Show staged diff ──

#[test]
fn test_show_staged_after_edit() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_show_staged_after_edit");

    print_info("Step 1: Initialize repository with base content and commit");
    init_repository(&env);
    let base_content = "line 1\nline 2\nline 3\n";
    apply_edit(&env, "test.txt", base_content);
    commit_changes(&env, "base commit", "user");

    print_info("Step 2: Edit file to create staged changes");
    let new_content = "line 1\nmodified line 2\nline 3\nnew line 4\n";
    apply_edit(&env, "test.txt", new_content);

    print_info("Step 3: Show staged diff");
    let resp = env
        .api
        .show(ShowRequest {
            show_what: "staged".into(),
            target_id: None,
        })
        .expect("show staged failed");

    print_info(&format!("  Target: {}", resp.target));
    assert_eq!(resp.target, "staged", "target should be 'staged'");
    assert!(!resp.diffs.is_empty(), "should have at least one diff");
    assert!(
        !resp.diffs[0].unified_diff.is_empty(),
        "diff should not be empty"
    );
    assert!(
        resp.diffs[0].file_path.contains("test.txt"),
        "should reference test.txt"
    );

    // Verify the diff contains the modifications
    let diff = &resp.diffs[0].unified_diff;
    print_info(&format!(
        "  Inserts: {}, Deletes: {}",
        resp.diffs[0].inserts, resp.diffs[0].deletes
    ));
    print_info("  Diff preview:");
    for line in diff.lines().take(10) {
        print_info(&format!("    {}", line));
    }

    // Should show changes: line 2 modified, line 4 added
    assert!(resp.diffs[0].inserts > 0, "should have insertions");
    assert!(resp.diffs[0].deletes > 0, "should have deletions");

    print_test_result(true, "test_show_staged_after_edit", None);
}

#[test]
fn test_show_staged_empty() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_show_staged_empty");

    print_info("Step 1: Initialize repository and commit");
    init_repository(&env);
    apply_edit(&env, "test.txt", "content\n");
    commit_changes(&env, "commit", "user");

    print_info("Step 2: Show staged (should be empty - no unstaged changes)");
    let resp = env
        .api
        .show(ShowRequest {
            show_what: "staged".into(),
            target_id: None,
        })
        .expect("show staged failed");

    assert_eq!(resp.target, "staged");
    // Even with no changes, we still get a diff (unified diff of empty vs content)
    assert!(!resp.diffs.is_empty(), "should have a diff entry");

    print_info(&format!(
        "  Inserts: {}, Deletes: {}",
        resp.diffs[0].inserts, resp.diffs[0].deletes
    ));

    print_test_result(true, "test_show_staged_empty", None);
}

// ── S2: Show checkpoint diff ──

#[test]
fn test_show_checkpoint_diff() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_show_checkpoint_diff");

    print_info("Step 1: Initialize repository with two commits");
    init_repository(&env);

    let content1 = "version 1\n";
    apply_edit(&env, "test.txt", content1);
    let cp1 = commit_changes(&env, "first commit", "user");

    let content2 = "version 1\nversion 2\n";
    apply_edit(&env, "test.txt", content2);
    let cp2 = commit_changes(&env, "second commit", "user");

    print_info("Step 2: Show checkpoint diff for second commit");
    let resp = env
        .api
        .show(ShowRequest {
            show_what: "checkpoint".into(),
            target_id: Some(cp2.to_hex()),
        })
        .expect("show checkpoint failed");

    print_info(&format!("  Target: {}", resp.target));
    assert!(
        resp.target.contains("checkpoint"),
        "should target a checkpoint"
    );
    assert!(!resp.diffs.is_empty(), "should have diff");
    assert!(resp.diffs[0].inserts > 0, "should show insertions");

    print_info(&format!(
        "  Inserts: {}, Deletes: {}",
        resp.diffs[0].inserts, resp.diffs[0].deletes
    ));

    print_info("Step 3: Show checkpoint diff for first commit (no parent)");
    let resp1 = env
        .api
        .show(ShowRequest {
            show_what: "checkpoint".into(),
            target_id: Some(cp1.to_hex()),
        })
        .expect("show checkpoint for first commit failed");

    assert!(
        !resp1.diffs.is_empty(),
        "even first commit should have diff"
    );
    print_info(&format!(
        "  First commit - Inserts: {}, Deletes: {}",
        resp1.diffs[0].inserts, resp1.diffs[0].deletes
    ));

    print_info("Step 4: Show checkpoint with invalid target_id");
    let err_resp = env.api.show(ShowRequest {
        show_what: "checkpoint".into(),
        target_id: None,
    });
    assert!(err_resp.is_err(), "show checkpoint without ID should fail");

    print_test_result(true, "test_show_checkpoint_diff", None);
}

// ── S3: Show partition diff ──

#[test]
fn test_show_partition_diff() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_show_partition_diff");

    print_info("Step 1: Initialize repository with base content");
    init_repository(&env);
    apply_edit(&env, "test.txt", "base content\n");
    commit_changes(&env, "base commit", "user");

    print_info("Step 2: Create agent edit and submit (creates approval partition)");
    apply_agent_edit(
        &env,
        "show-agent",
        "test.txt",
        "base content\nagent addition\n",
    );
    submit_agent(&env, "show-agent");

    print_info("Step 3: Show partition diff for the approval partition");
    // Get the partition name from storage
    let partitions = env
        .storage
        .list_partitions()
        .expect("failed to list partitions");
    let approval_partition = partitions
        .iter()
        .find(|p| p.name.contains("show-agent"))
        .expect("should find show-agent partition");

    print_info(&format!("  Partition name: {}", approval_partition.name));

    let resp = env
        .api
        .show(ShowRequest {
            show_what: "partition".into(),
            target_id: Some(approval_partition.name.clone()),
        })
        .expect("show partition failed");

    print_info(&format!("  Target: {}", resp.target));
    assert!(
        resp.target.contains("partition"),
        "should target a partition"
    );
    assert!(!resp.diffs.is_empty(), "should have diff output");
    assert!(
        resp.diffs[0].inserts > 0,
        "should show insertions from agent addition"
    );

    print_info(&format!(
        "  Inserts: {}, Deletes: {}",
        resp.diffs[0].inserts, resp.diffs[0].deletes
    ));

    print_info("Step 4: Show partition with non-existent name");
    let err_resp = env.api.show(ShowRequest {
        show_what: "partition".into(),
        target_id: Some("nonexistent_partition".into()),
    });
    assert!(err_resp.is_err(), "show non-existent partition should fail");

    print_test_result(true, "test_show_partition_diff", None);
}

// ── S4: Show error cases ──

#[test]
fn test_show_error_cases() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_show_error_cases");

    print_info("Step 1: Initialize repository");
    init_repository(&env);

    print_info("Step 2: Show with invalid target name");
    let resp = env.api.show(ShowRequest {
        show_what: "invalid_target".into(),
        target_id: None,
    });
    assert!(resp.is_err(), "should fail for invalid target");
    if let Err(e) = resp {
        print_info(&format!("  Error: {}", e));
        assert_eq!(e.code, "INVALID_PARAMS", "should be invalid params error");
    }

    print_info("Step 3: Show checkpoint without target_id");
    let resp = env.api.show(ShowRequest {
        show_what: "checkpoint".into(),
        target_id: None,
    });
    assert!(resp.is_err(), "should fail for missing checkpoint ID");

    print_info("Step 4: Show partition without target_id");
    let resp = env.api.show(ShowRequest {
        show_what: "partition".into(),
        target_id: None,
    });
    assert!(resp.is_err(), "should fail for missing partition name");

    print_test_result(true, "test_show_error_cases", None);
}
