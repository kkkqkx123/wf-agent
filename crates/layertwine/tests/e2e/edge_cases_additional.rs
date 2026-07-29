//! Additional E2E tests for edge cases and error handling

use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use crate::common::output::*;
use layertwine::api::*;

#[test]
fn test_large_file_handling() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_large_file_handling");

    print_info("Step 1: Initialize repository");
    init_repository(&env);

    print_info("Step 2: Create large file (1000 lines)");
    let large_content: String = (1..=1000).map(|i| format!("Line {}\n", i)).collect();

    print_info(&format!("  File size: {} bytes", large_content.len()));

    let snapshot = apply_edit(&env, "large.txt", &large_content);
    commit_changes(&env, "Add large file", "user-1");
    print_success("Large file committed");

    print_info("Step 3: Verify reconstruction");
    let reconstructed = reconstruct_text(&env, &snapshot);
    assert!(reconstructed.is_some(), "Failed to reconstruct large file");

    let content = reconstructed.unwrap();
    assert_eq!(content.lines().count(), 1000, "Line count mismatch");
    assert!(content.contains("Line 1"), "Should contain first line");
    assert!(content.contains("Line 1000"), "Should contain last line");
    print_success("Large file reconstructed correctly");

    print_test_result(true, "test_large_file_handling", None);
}

#[test]
fn test_special_characters() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_special_characters");

    print_info("Step 1: Initialize repository");
    init_repository(&env);

    print_info("Step 2: Create content with special characters");
    let special_content = "Line with special chars: @#$%^&*()\nUnicode: 你好世界\n";

    let snapshot = apply_edit(&env, "special.txt", special_content);
    commit_changes(&env, "Add special characters", "user-1");
    print_success("Special characters committed");

    print_info("Step 3: Verify reconstruction");
    let reconstructed = reconstruct_text(&env, &snapshot);
    assert!(
        reconstructed.is_some(),
        "Failed to reconstruct special characters"
    );

    let content = reconstructed.unwrap();
    assert!(
        content.contains("@#$%^&*()"),
        "Special chars should be preserved"
    );
    assert!(content.contains("你好世界"), "Unicode should be preserved");
    print_success("Special characters preserved correctly");

    print_test_result(true, "test_special_characters", None);
}

#[test]
fn test_invalid_snapshot_id() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_invalid_snapshot_id");

    print_info("Step 1: Initialize repository");
    init_repository(&env);

    print_info("Step 2: Try to backup with invalid snapshot ID");
    let backup_response = env.api.backup(BackupRequest {
        snapshot_id: "invalid_snapshot_id_12345".to_string(),
        label: Some("Test backup".to_string()),
    });

    assert!(
        backup_response.is_err(),
        "Backup with invalid snapshot ID should fail"
    );
    print_success("Correctly rejected invalid snapshot ID");

    print_info("Step 3: Try to restore with invalid backup ID");
    let restore_response = env.api.restore(RestoreRequest {
        backup_id: "invalid_backup_id_12345".to_string(),
    });

    assert!(
        restore_response.is_err(),
        "Restore with invalid backup ID should fail"
    );
    print_success("Correctly rejected invalid backup ID");

    print_test_result(true, "test_invalid_snapshot_id", None);
}

#[test]
fn test_nonexistent_branch_operations() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_nonexistent_branch_operations");

    print_info("Step 1: Initialize repository");
    init_repository(&env);
    apply_edit(&env, "test.txt", "Initial\n");
    commit_changes(&env, "Initial commit", "user-1");

    print_info("Step 2: Try to switch to nonexistent branch");
    let switch_response = env.api.branch_switch(BranchSwitchRequest {
        name: "nonexistent_branch".to_string(),
    });

    assert!(
        switch_response.is_err(),
        "Switching to nonexistent branch should fail"
    );
    print_success("Correctly rejected nonexistent branch switch");

    print_info("Step 3: Try to merge nonexistent branch");
    let merge_response = env.api.merge(MergeRequest {
        branch: "nonexistent_branch".to_string(),
        message: Some("Try to merge".to_string()),
    });

    assert!(
        merge_response.is_err(),
        "Merging nonexistent branch should fail"
    );
    print_success("Correctly rejected nonexistent branch merge");

    print_test_result(true, "test_nonexistent_branch_operations", None);
}

#[test]
fn test_many_small_files() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_many_small_files");

    print_info("Step 1: Initialize repository");
    init_repository(&env);

    print_info("Step 2: Create many small files (20 files)");
    let mut snapshots = Vec::new();

    for i in 1..=20 {
        let file_name = format!("file_{:03}.txt", i);
        let content = format!("Content of file {}\n", i);

        let snapshot = apply_edit(&env, &file_name, &content);
        snapshots.push(snapshot);

        if i % 5 == 0 {
            print_info(&format!("  Created {} files", i));
        }
    }

    commit_changes(&env, "Add 20 small files", "user-1");
    print_success("All 20 files committed");

    print_info("Step 3: Verify all files");
    let mut verified_count = 0;
    for (i, snapshot) in snapshots.iter().enumerate() {
        let _file_name = format!("file_{:03}.txt", i + 1);
        let _expected_content = format!("Content of file {}\n", i + 1);

        let reconstructed = reconstruct_text(&env, snapshot);
        if let Some(content) = reconstructed {
            if content.contains(&format!("Content of file {}", i + 1)) {
                verified_count += 1;
            }
        }
    }

    assert_eq!(verified_count, 20, "Should verify all 20 files");
    print_success(&format!("Verified {} out of 20 files", verified_count));

    print_test_result(true, "test_many_small_files", None);
}
