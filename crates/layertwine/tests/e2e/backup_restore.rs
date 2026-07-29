//! Backup and restore E2E tests

use crate::common::assertions::*;
use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use crate::common::output::*;
use layertwine::api::{ApiService, BackupRequest, RestoreRequest};
use layertwine::core::types::SnapshotId;

#[test]
fn test_backup_and_restore() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_backup_and_restore");

    print_info("Step 1: Initialize repository and create content");
    init_repository(&env);

    // Create multiple edits
    let content1 = "Version 1\n";
    let _snapshot1 = apply_edit(&env, "backup_test.txt", content1);
    commit_changes(&env, "Commit version 1", "user-1");
    print_success("Version 1 committed");

    let content2 = "Version 1\nVersion 2\n";
    let _snapshot2 = apply_edit(&env, "backup_test.txt", content2);
    commit_changes(&env, "Commit version 2", "user-1");
    print_success("Version 2 committed");

    let content3 = "Version 1\nVersion 2\nVersion 3";
    let snapshot3 = apply_edit(&env, "backup_test.txt", content3);
    commit_changes(&env, "Commit version 3", "user-1");
    print_success("Version 3 committed");

    // Verify current content
    print_info("Step 2: Verify current content");
    let current_content =
        reconstruct_text(&env, &snapshot3).expect("Failed to reconstruct current content");
    print_file_content(&current_content, 5);
    assert_eq!(current_content, content3, "Current content mismatch");

    // Create backup
    print_info("Step 3: Create backup from snapshot");
    let backup_response = env
        .api
        .backup(BackupRequest {
            snapshot_id: snapshot3.to_hex(),
            label: Some("Full backup before modifications".to_string()),
        })
        .expect("Failed to create backup");
    print_success(&format!(
        "Backup created: {}",
        &backup_response.backup_id[..12]
    ));
    print_info(&format!(
        "  Source snapshot: {}",
        &backup_response.source_snapshot_id[..12]
    ));
    print_info(&format!(
        "  Label: {}",
        backup_response.label.as_ref().unwrap()
    ));

    let backup_id = backup_response.backup_id.clone();

    // Modify content significantly
    print_info("Step 4: Modify content significantly");
    let modified_content = "Modified content\nLine 2\nLine 3\nLine 4";
    let modified_snapshot = apply_edit(&env, "backup_test.txt", modified_content);
    commit_changes(&env, "Major modifications", "user-1");
    print_success("Content modified");

    // Verify content changed
    print_info("Step 5: Verify content changed");
    let current_modified =
        reconstruct_text(&env, &modified_snapshot).expect("Failed to reconstruct modified content");
    print_file_content(&current_modified, 5);
    assert_eq!(
        current_modified, modified_content,
        "Modified content mismatch"
    );
    assert_ne!(
        current_modified, content3,
        "Content should be different from backup"
    );

    // Restore backup
    print_info("Step 6: Restore from backup");
    let restore_response = env
        .api
        .restore(RestoreRequest {
            backup_id: backup_id.clone(),
        })
        .expect("Failed to restore backup");
    print_success(&format!(
        "Backup restored: {}",
        &restore_response.backup_id[..12]
    ));
    print_info(&format!("  File: {}", restore_response.file));
    print_info(&format!(
        "  Deltas restored: {}",
        restore_response.deltas_restored
    ));

    // Verify restoration
    print_info("Step 7: Verify restoration");
    let restored_snapshot_id =
        SnapshotId::from_hex(&snapshot3.to_hex()).expect("Invalid snapshot ID");
    let restored_content = reconstruct_text(&env, &restored_snapshot_id)
        .expect("Failed to reconstruct restored content");

    print_file_content(&restored_content, 5);
    assert_eq!(restored_content, content3, "Restored content mismatch");
    print_success("Content successfully restored to version 3");

    // Make a small edit after restore to ensure commit is not empty
    print_info("Step 8: Make small edit after restore");
    let final_content = "Version 1\nVersion 2\nVersion 3\nRestored and finalized";
    let final_snapshot = apply_edit(&env, "backup_test.txt", final_content);
    print_success("Final edit applied");

    // Commit final content
    print_info("Step 9: Commit final content");
    commit_changes(&env, "Restore from backup and finalize", "user-1");
    print_success("Final content committed");

    // Verify final content
    print_info("Step 10: Verify final content");
    let final_reconstructed =
        reconstruct_text(&env, &final_snapshot).expect("Failed to reconstruct final content");
    assert_eq!(final_reconstructed, final_content, "Final content mismatch");
    print_success("Final content verified");

    // Verify log
    print_info("Step 11: Verify commit history");
    let log = get_log(&env, None);
    print_checkpoint_log(&log);
    assert_log_entry_count(&env, 5); // V1, V2, V3, modified, final

    print_test_result(true, "test_backup_and_restore", None);
}

#[test]
fn test_multiple_backups() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_multiple_backups");

    print_info("Step 1: Initialize repository");
    init_repository(&env);

    // Create content and first backup
    let content1 = "Version 1";
    let snapshot1 = apply_edit(&env, "multi_backup.txt", content1);
    commit_changes(&env, "Commit version 1", "user-1");

    let backup1 = env
        .api
        .backup(BackupRequest {
            snapshot_id: snapshot1.to_hex(),
            label: Some("Backup of version 1".to_string()),
        })
        .expect("Failed to create backup 1");
    print_success(&format!("Backup 1 created: {}", &backup1.backup_id[..12]));

    // Create more content and second backup
    let content2 = "Version 1\nVersion 2";
    let snapshot2 = apply_edit(&env, "multi_backup.txt", content2);
    commit_changes(&env, "Commit version 2", "user-1");

    let backup2 = env
        .api
        .backup(BackupRequest {
            snapshot_id: snapshot2.to_hex(),
            label: Some("Backup of version 2".to_string()),
        })
        .expect("Failed to create backup 2");
    print_success(&format!("Backup 2 created: {}", &backup2.backup_id[..12]));

    // Create more content and third backup
    let content3 = "Version 1\nVersion 2\nVersion 3";
    let snapshot3 = apply_edit(&env, "multi_backup.txt", content3);
    commit_changes(&env, "Commit version 3", "user-1");

    let backup3 = env
        .api
        .backup(BackupRequest {
            snapshot_id: snapshot3.to_hex(),
            label: Some("Backup of version 3".to_string()),
        })
        .expect("Failed to create backup 3");
    print_success(&format!("Backup 3 created: {}", &backup3.backup_id[..12]));

    // Verify all backups exist
    print_info("Step 2: Verify all backups exist");
    let backups = vec![
        (
            "Backup 1",
            backup1.backup_id.clone(),
            backup1.source_snapshot_id.clone(),
            content1,
        ),
        (
            "Backup 2",
            backup2.backup_id.clone(),
            backup2.source_snapshot_id.clone(),
            content2,
        ),
        (
            "Backup 3",
            backup3.backup_id.clone(),
            backup3.source_snapshot_id.clone(),
            content3,
        ),
    ];

    for (name, backup_id, _source_snapshot_id, _expected_content) in &backups {
        print_info(&format!("  {}: {}", name, &backup_id[..12]));
    }

    // Restore each backup and verify
    print_info("Step 3: Restore and verify each backup");

    for (name, backup_id, source_snapshot_id, expected_content) in &backups {
        let restore_response = env
            .api
            .restore(RestoreRequest {
                backup_id: backup_id.clone(),
            })
            .unwrap_or_else(|_| panic!("Failed to restore {}", name));

        print_info(&format!(
            "  Restored {}: {} deltas",
            name, restore_response.deltas_restored
        ));

        let restored_id = SnapshotId::from_hex(source_snapshot_id).expect("Invalid snapshot ID");

        print_info(&format!(
            "  Attempting to reconstruct snapshot: {}",
            &source_snapshot_id[..12]
        ));

        let restored_content = reconstruct_text(&env, &restored_id);

        if restored_content.is_none() {
            print_error(&format!("Failed to reconstruct {}: returned None", name));
            panic!("Failed to reconstruct {}", name);
        }

        let restored_content = restored_content.unwrap();

        assert_eq!(
            restored_content, **expected_content,
            "{} content mismatch after restore",
            name
        );
    }

    print_success("All backups verified successfully");

    // Verify final state
    print_info("Step 4: Verify final state");
    let final_content =
        reconstruct_text(&env, &snapshot3).expect("Failed to reconstruct final content");
    print_file_content(&final_content, 5);
    assert_eq!(final_content, content3, "Final content should be version 3");

    print_test_result(true, "test_multiple_backups", None);
}

#[test]
fn test_backup_empty_file() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_backup_empty_file");

    print_info("Step 1: Initialize repository with empty file");
    init_repository(&env);

    let empty_content = "";
    let snapshot = apply_edit(&env, "empty.txt", empty_content);
    commit_changes(&env, "Create empty file", "user-1");
    print_success("Empty file created");

    // Verify empty
    let current = reconstruct_text(&env, &snapshot).expect("Failed to reconstruct empty file");
    assert!(current.is_empty(), "File should be empty");
    print_info("Empty file verified");

    // Create backup
    print_info("Step 2: Create backup of empty file");
    let backup = env
        .api
        .backup(BackupRequest {
            snapshot_id: snapshot.to_hex(),
            label: Some("Empty file backup".to_string()),
        })
        .expect("Failed to backup empty file");
    print_success(&format!("Backup created: {}", &backup.backup_id[..12]));

    // Modify file
    print_info("Step 3: Modify file");
    let modified_content = "Now has content";
    let _modified_snapshot = apply_edit(&env, "empty.txt", modified_content);
    commit_changes(&env, "Add content", "user-1");
    print_success("File modified");

    // Restore backup
    print_info("Step 4: Restore backup");
    let restore = env
        .api
        .restore(RestoreRequest {
            backup_id: backup.backup_id.clone(),
        })
        .expect("Failed to restore backup");
    print_success(&format!(
        "Backup restored: {} deltas",
        restore.deltas_restored
    ));

    // Verify empty restored
    print_info("Step 5: Verify empty file restored");
    let restored = reconstruct_text(&env, &snapshot).expect("Failed to reconstruct restored file");
    assert!(restored.is_empty(), "File should be empty after restore");
    print_success("Empty file successfully restored");

    print_test_result(true, "test_backup_empty_file", None);
}

#[test]
fn test_backup_large_file() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_backup_large_file");

    print_info("Step 1: Initialize repository with large file");
    init_repository(&env);

    // Create a large file (100 lines)
    let large_content: String = (1..=100)
        .map(|i| format!("Line {}", i))
        .collect::<Vec<_>>()
        .join("\n");

    let snapshot = apply_edit(&env, "large.txt", &large_content);
    commit_changes(&env, "Create large file", "user-1");
    print_success(&format!(
        "Large file created ({} lines)",
        large_content.lines().count()
    ));

    // Create backup
    print_info("Step 2: Create backup of large file");
    let backup = env
        .api
        .backup(BackupRequest {
            snapshot_id: snapshot.to_hex(),
            label: Some("Large file backup".to_string()),
        })
        .expect("Failed to backup large file");
    print_success(&format!("Backup created: {}", &backup.backup_id[..12]));

    // Modify large file
    print_info("Step 3: Modify large file");
    let modified_content: String = (1..=100)
        .map(|i| format!("Modified Line {}", i))
        .collect::<Vec<_>>()
        .join("\n");

    let _modified_snapshot = apply_edit(&env, "large.txt", &modified_content);
    commit_changes(&env, "Modify large file", "user-1");
    print_success("Large file modified");

    // Restore backup
    print_info("Step 4: Restore backup");
    let restore = env
        .api
        .restore(RestoreRequest {
            backup_id: backup.backup_id.clone(),
        })
        .expect("Failed to restore backup");
    print_success(&format!(
        "Backup restored: {} deltas",
        restore.deltas_restored
    ));

    // Verify restoration
    print_info("Step 5: Verify large file restoration");
    let restored = reconstruct_text(&env, &snapshot).expect("Failed to reconstruct restored file");

    assert_eq!(restored.lines().count(), 100, "Should have 100 lines");
    assert_eq!(restored, large_content, "Large file content mismatch");

    print_info(&format!(
        "Successfully restored large file ({} lines)",
        restored.lines().count()
    ));

    // Show preview
    print_file_content(&restored, 10);

    print_test_result(true, "test_backup_large_file", None);
}
