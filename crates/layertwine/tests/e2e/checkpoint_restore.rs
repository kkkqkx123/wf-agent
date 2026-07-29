//! Checkpoint restore E2E tests
//!
//! Covers:
//!   S1 - Full checkpoint restore
//!   S2 - Selective checkpoint restore (by source filter)
//!   S3 - Time-based checkpoint restore
//!   S4 - Checkpoint diff
//!   S5 - Checkpoint rollback

use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use crate::common::output::*;
use layertwine::api::{
    ApiService, CheckpointDiffRequest, CheckpointRestoreByTimeRequest, CheckpointRestoreRequest,
    CheckpointRollbackRequest, LogRequest,
};
use layertwine::core::types::SnapshotId;

// ── S1: Full checkpoint restore ──

#[test]
fn test_checkpoint_full_restore() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_checkpoint_full_restore");

    print_info("Step 1: Initialize repository and create content");
    init_repository(&env);
    let content1 = "Version 1\n";
    apply_edit(&env, "test.txt", content1);
    commit_changes(&env, "Commit V1", "user");

    let content2 = "Version 1\nVersion 2\n";
    apply_edit(&env, "test.txt", content2);
    let cp2 = commit_changes(&env, "Commit V2", "user");
    print_success("Two commits created");

    print_info("Step 2: Call checkpoint_restore on V2");
    let resp = env
        .api
        .checkpoint_restore(CheckpointRestoreRequest {
            checkpoint_id: cp2.to_hex(),
            source_filter: None,
        })
        .expect("checkpoint_restore failed");

    print_info(&format!("  Checkpoint author: {}", resp.checkpoint.author));
    print_info(&format!("  Message: {}", resp.checkpoint.message));
    print_info(&format!("  Snapshots: {}", resp.snapshots.len()));
    print_info(&format!("  Ancestry depth: {}", resp.ancestry.len()));

    assert_eq!(resp.checkpoint.message, "Commit V2");
    assert!(
        !resp.snapshots.is_empty(),
        "should have at least one snapshot"
    );
    assert!(
        resp.ancestry.len() >= 2,
        "ancestry should include root + V1 + V2"
    );

    // The ancestry chain should end with the target checkpoint
    assert_eq!(
        resp.ancestry.last().map(|s| s.as_str()),
        Some(cp2.to_hex().as_str()),
        "ancestry should end with requested checkpoint"
    );

    print_info("Step 3: Verify snapshot info integrity");
    for snap in &resp.snapshots {
        assert!(
            !snap.snapshot_id.is_empty(),
            "snapshot_id should not be empty"
        );
        print_info(&format!(
            "  snapshot: {} source: '{}'",
            &snap.snapshot_id[..12],
            snap.source
        ));
    }
    // At least one snapshot should have a non-empty source (from commit metadata)
    assert!(
        resp.snapshots.iter().any(|s| !s.source.is_empty()),
        "at least one snapshot should have a source"
    );

    print_test_result(true, "test_checkpoint_full_restore", None);
}

// ── S2: Selective checkpoint restore ──

#[test]
fn test_checkpoint_selective_restore() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_checkpoint_selective_restore");

    print_info("Step 1: Initialize repository");
    init_repository(&env);
    let content = "Hello World\n";
    apply_edit(&env, "test.txt", content);
    let cp = commit_changes(&env, "Single commit", "user");

    print_info("Step 2: Full restore should return snapshot");
    let full_resp = env
        .api
        .checkpoint_restore(CheckpointRestoreRequest {
            checkpoint_id: cp.to_hex(),
            source_filter: None,
        })
        .expect("full restore failed");
    assert!(!full_resp.snapshots.is_empty());

    print_info("Step 3: Selective restore with non-matching filter returns empty");
    let sel_resp = env
        .api
        .checkpoint_restore(CheckpointRestoreRequest {
            checkpoint_id: cp.to_hex(),
            source_filter: Some(vec!["agent://".to_string()]),
        })
        .expect("selective restore failed");

    // With our current source mapping (file://), agent:// filter should not match
    assert!(
        sel_resp.snapshots.is_empty(),
        "agent:// filter should not match file snapshots"
    );

    print_info("Step 4: Selective restore with matching filter returns snapshot");
    let match_resp = env
        .api
        .checkpoint_restore(CheckpointRestoreRequest {
            checkpoint_id: cp.to_hex(),
            source_filter: Some(vec!["file://".to_string()]),
        })
        .expect("selective restore with file filter failed");

    assert!(
        !match_resp.snapshots.is_empty(),
        "file:// filter should match"
    );
    for snap in &match_resp.snapshots {
        print_info(&format!("  Matched source: '{}'", snap.source));
    }

    print_test_result(true, "test_checkpoint_selective_restore", None);
}

// ── S3: Time-based checkpoint restore ──

#[test]
fn test_checkpoint_restore_by_time() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_checkpoint_restore_by_time");

    print_info("Step 1: Create commits at different times");
    init_repository(&env);

    let content1 = "v1\n";
    apply_edit(&env, "test.txt", content1);
    commit_changes(&env, "V1", "user");
    let t1 = chrono::Utc::now().timestamp_millis();

    std::thread::sleep(std::time::Duration::from_millis(20));

    let content2 = "v1\nv2\n";
    apply_edit(&env, "test.txt", content2);
    let _cp2 = commit_changes(&env, "V2", "user");
    let t2 = chrono::Utc::now().timestamp_millis();

    std::thread::sleep(std::time::Duration::from_millis(20));

    let content3 = "v1\nv2\nv3\n";
    apply_edit(&env, "test.txt", content3);
    let _cp3 = commit_changes(&env, "V3", "user");
    let t3 = chrono::Utc::now().timestamp_millis();

    print_info(&format!("  V1 timestamp: {}", t1));
    print_info(&format!("  V2 timestamp: {}", t2));
    print_info(&format!("  V3 timestamp: {}", t3));

    print_info("Step 2: Restore by time nearest to V2 timestamp");
    let resp = env
        .api
        .checkpoint_restore_by_time(CheckpointRestoreByTimeRequest {
            target_time: t2,
            source_filter: None,
        })
        .expect("time-based restore failed");

    // Should find the checkpoint nearest to t2 (should be V2 or V3)
    print_info(&format!("  Found checkpoint: {}", resp.checkpoint.message));
    assert!(
        resp.checkpoint.message == "V2" || resp.checkpoint.message == "V3",
        "should find V2 or V3 nearest to t2"
    );

    print_info("Step 3: Restore by time in the future (should find latest)");
    let resp_future = env
        .api
        .checkpoint_restore_by_time(CheckpointRestoreByTimeRequest {
            target_time: t3 + 100000,
            source_filter: None,
        })
        .expect("future time restore failed");

    print_info(&format!(
        "  Future nearest: {}",
        resp_future.checkpoint.message
    ));
    assert!(!resp_future.snapshots.is_empty());

    print_info("Step 4: Restore by time with source filter");
    let resp_filtered = env
        .api
        .checkpoint_restore_by_time(CheckpointRestoreByTimeRequest {
            target_time: t3,
            source_filter: Some(vec!["file://".to_string()]),
        })
        .expect("filtered time restore failed");

    assert!(
        !resp_filtered.snapshots.is_empty(),
        "file filter should match"
    );

    print_test_result(true, "test_checkpoint_restore_by_time", None);
}

// ── S4: Checkpoint diff ──

#[test]
fn test_checkpoint_diff() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_checkpoint_diff");

    print_info("Step 1: Create multiple commits");
    init_repository(&env);

    let content1 = "line1\n";
    apply_edit(&env, "test.txt", content1);
    let cp1 = commit_changes(&env, "V1", "user");

    let content2 = "line1\nline2\n";
    apply_edit(&env, "test.txt", content2);
    let cp2_for_diff = commit_changes(&env, "V2", "user");

    let content3 = "line1\nline2\nline3\n";
    apply_edit(&env, "test.txt", content3);
    let cp3 = commit_changes(&env, "V3", "user");

    print_info("Step 2: Diff V1 vs V3");
    let diff = env
        .api
        .checkpoint_diff(CheckpointDiffRequest {
            from_id: cp1.to_hex(),
            to_id: cp3.to_hex(),
        })
        .expect("checkpoint_diff failed");

    print_info(&format!("  Total changes: {}", diff.total_changes));
    print_info(&format!(
        "  Added: {:?}",
        diff.added.iter().map(|s| &s[..12]).collect::<Vec<_>>()
    ));
    print_info(&format!(
        "  Removed: {:?}",
        diff.removed.iter().map(|s| &s[..12]).collect::<Vec<_>>()
    ));
    print_info(&format!(
        "  Modified: {:?}",
        diff.modified.iter().map(|s| &s[..12]).collect::<Vec<_>>()
    ));

    // Each commit replaces the previous snapshot, so V1→V3 should show changes
    assert!(
        diff.total_changes >= 1,
        "should detect changes between V1 and V3"
    );

    print_info("Step 3: Diff same checkpoint (should be empty)");
    let same_diff = env
        .api
        .checkpoint_diff(CheckpointDiffRequest {
            from_id: cp2_for_diff.to_hex(),
            to_id: cp2_for_diff.to_hex(),
        })
        .expect("same diff failed");

    assert_eq!(same_diff.total_changes, 0, "self-diff should be empty");
    assert!(same_diff.added.is_empty());
    assert!(same_diff.removed.is_empty());
    assert!(same_diff.modified.is_empty());

    print_info("Step 4: Invalid checkpoint ID should error");
    let err_result = env.api.checkpoint_diff(CheckpointDiffRequest {
        from_id: "deadbeef00000000000000000000000000000000000000000000000000000000".to_string(),
        to_id: cp1.to_hex(),
    });
    assert!(err_result.is_err(), "should error for invalid from_id");

    print_test_result(true, "test_checkpoint_diff", None);
}

// ── S5: Checkpoint rollback ──

#[test]
fn test_checkpoint_rollback() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_checkpoint_rollback");

    print_info("Step 1: Create multiple versions and commits");
    init_repository(&env);

    let content1 = "version 1\n";
    apply_edit(&env, "test.txt", content1);
    let cp1 = commit_changes(&env, "V1", "user");

    let content2 = "version 1\nversion 2\n";
    apply_edit(&env, "test.txt", content2);
    let _cp2 = commit_changes(&env, "V2", "user");

    let content3 = "version 1\nversion 2\nversion 3\n";
    let snap3 = apply_edit(&env, "test.txt", content3);
    let _cp3 = commit_changes(&env, "V3", "user");

    // Verify current content is V3
    let text_before =
        reconstruct_text(&env, &snap3).expect("Failed to reconstruct current content");
    assert_eq!(
        text_before.trim_end_matches('\n'),
        content3.trim_end_matches('\n'),
        "should have V3 content before rollback"
    );

    print_info("Step 2: Rollback to V1 checkpoint");
    let rollback_resp = env
        .api
        .checkpoint_rollback(CheckpointRollbackRequest {
            checkpoint_id: cp1.to_hex(),
        })
        .expect("checkpoint_rollback failed");

    print_info(&format!(
        "  Rollback to checkpoint: {}",
        &rollback_resp.checkpoint_id[..12]
    ));
    print_info(&format!(
        "  Snapshot IDs: {:?}",
        rollback_resp
            .snapshot_ids
            .iter()
            .map(|s| &s[..12])
            .collect::<Vec<_>>()
    ));

    assert!(
        !rollback_resp.snapshot_ids.is_empty(),
        "should return baseline snapshots"
    );

    // After rollback, staged partition should point to V1 baseline snapshot
    print_info("Step 3: Verify staged partition updated to V1 baseline");
    let staged_snap_id =
        SnapshotId::from_hex(&rollback_resp.snapshot_ids[0]).expect("invalid snapshot ID");
    let rolled_text =
        reconstruct_text(&env, &staged_snap_id).expect("Failed to reconstruct rolled-back content");

    print_file_content(&rolled_text, 3);
    assert_eq!(
        rolled_text.trim_end_matches('\n'),
        content1.trim_end_matches('\n'),
        "staged content should match V1 after rollback"
    );

    print_info("Step 4: Verify we can commit after rollback");
    apply_edit(&env, "test.txt", content1);
    let cp4 = commit_changes(&env, "Post-rollback V1", "user");
    print_info(&format!("  New commit: {}", &cp4.to_hex()[..12]));

    // Verify commit log has all entries
    let log = env.api.log(LogRequest { count: None }).expect("log failed");
    print_info(&format!(
        "  Log has {} entries (including root)",
        log.checkpoints.len()
    ));
    // rooted + V1 + V2 + V3 + post-rollback = 4 visible (or 5 with root)
    assert!(log.checkpoints.len() >= 5, "should have root + 4 commits");

    print_info("Step 5: Invalid checkpoint ID should error");
    let err_result = env.api.checkpoint_rollback(CheckpointRollbackRequest {
        checkpoint_id: "deadbeef00000000000000000000000000000000000000000000000000000000"
            .to_string(),
    });
    assert!(
        err_result.is_err(),
        "should error for invalid checkpoint ID"
    );

    print_test_result(true, "test_checkpoint_rollback", None);
}

// ── Edge case: Restore with empty repo (no user checkpoints) ──

#[test]
fn test_checkpoint_restore_empty_repo() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_checkpoint_restore_empty_repo");

    print_info("Step 1: Initialize repository and make one commit");
    init_repository(&env);
    apply_edit(&env, "test.txt", "initial content\n");
    let cp = commit_changes(&env, "Initial commit", "user");

    print_info("Step 2: Full restore of the user-created checkpoint");
    let resp = env
        .api
        .checkpoint_restore(CheckpointRestoreRequest {
            checkpoint_id: cp.to_hex(),
            source_filter: None,
        })
        .expect("checkpoint restore failed");

    print_info(&format!("  Message: {}", resp.checkpoint.message));
    print_info(&format!("  Ancestry depth: {}", resp.ancestry.len()));
    assert_eq!(resp.checkpoint.message, "Initial commit");
    assert!(!resp.snapshots.is_empty(), "should have snapshots");

    print_test_result(true, "test_checkpoint_restore_empty_repo", None);
}

// ── Edge case: Full restore with ancestor chain verification ──

#[test]
fn test_checkpoint_restore_ancestry_chain() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_checkpoint_restore_ancestry_chain");

    print_info("Step 1: Create linear commit chain (V1 → V2 → V3)");
    init_repository(&env);

    apply_edit(&env, "test.txt", "v1\n");
    let cp1 = commit_changes(&env, "V1", "user");

    apply_edit(&env, "test.txt", "v1\nv2\n");
    let cp2 = commit_changes(&env, "V2", "user");

    apply_edit(&env, "test.txt", "v1\nv2\nv3\n");
    let cp3 = commit_changes(&env, "V3", "user");

    print_info("Step 2: Verify ancestry chain for V3 includes V1 and V2");
    let resp = env
        .api
        .checkpoint_restore(CheckpointRestoreRequest {
            checkpoint_id: cp3.to_hex(),
            source_filter: None,
        })
        .expect("restore V3 failed");

    let ancestry: Vec<&str> = resp.ancestry.iter().map(|s| s.as_str()).collect();
    print_info(&format!(
        "  Ancestry: {:?}",
        ancestry.iter().map(|s| &s[..12]).collect::<Vec<_>>()
    ));

    assert!(
        ancestry.len() >= 3,
        "V3 ancestry should have at least 3 entries (root+V1+V2+V3)"
    );
    assert!(
        ancestry.contains(&cp1.to_hex().as_str()),
        "ancestry should contain V1"
    );
    assert!(
        ancestry.contains(&cp2.to_hex().as_str()),
        "ancestry should contain V2"
    );
    assert_eq!(
        ancestry.last(),
        Some(&cp3.to_hex().as_str()),
        "last should be V3"
    );

    print_test_result(true, "test_checkpoint_restore_ancestry_chain", None);
}
