//! Basic workflow E2E tests

use crate::common::assertions::*;
use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use crate::common::output::*;
use layertwine::api::{ApiService, CommitRequest, EditRequest, InitRequest, LogRequest};
use layertwine::core::types::SnapshotId;
use layertwine::storage::repository::PartitionStore;

#[test]
fn test_complete_edit_workflow() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_complete_edit_workflow");

    print_info("Setting up test environment...");
    print_info(&format!("Database path: {}", env.db_path_str()));

    // Step 1: Initialize repository
    print_info("Step 1: Initialize repository");
    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: None,
            git_ref: None,
        })
        .expect("Failed to initialize repository");
    print_success("Repository initialized");

    // Verify initial state
    let status = get_status(&env);
    print_layer_state_from_status(&status);

    // Step 2: Apply edit
    print_info("Step 2: Apply edit to 'test.txt'");
    let file_content = "Hello, World!\nThis is a test file.";
    let edit_response = env
        .api
        .edit(EditRequest {
            file: "test.txt".into(),
            content: Some(file_content.to_string()),
        })
        .expect("Failed to apply edit");
    print_success(&format!(
        "Edit applied, snapshot_id: {}",
        &edit_response.snapshot_id[..12]
    ));

    let snapshot_id =
        SnapshotId::from_hex(&edit_response.snapshot_id).expect("Invalid snapshot ID");
    assert_valid_snapshot_id(&snapshot_id);

    // Step 3: Commit changes
    print_info("Step 3: Commit changes");
    let commit_response = env
        .api
        .commit(CommitRequest {
            message: "Initial commit".into(),
            author: Some("test-user".into()),
        })
        .expect("Failed to commit");
    print_success(&format!(
        "Changes committed, checkpoint_id: {}",
        &commit_response.checkpoint_id[..12]
    ));

    let checkpoint_id =
        SnapshotId::from_hex(&commit_response.checkpoint_id).expect("Invalid checkpoint ID");
    assert_valid_snapshot_id(&checkpoint_id);

    // Step 4: Verify log history
    print_info("Step 4: Verify log history");
    let log_response = env
        .api
        .log(LogRequest { count: Some(10) })
        .expect("Failed to get log");

    print_checkpoint_log(
        &log_response
            .checkpoints
            .iter()
            .map(|cp| format!("{}: {}", cp.id, cp.message))
            .collect::<Vec<_>>(),
    );

    assert_log_entry_count(&env, 1);
    assert_log_contains(&env, "Initial commit");

    // Step 5: Verify file content
    print_info("Step 5: Verify file content");
    let reconstructed = reconstruct_text(&env, &snapshot_id);
    assert!(reconstructed.is_some(), "Failed to reconstruct text");

    let actual_content = reconstructed.unwrap();
    print_file_content(&actual_content, 5);
    assert_eq!(actual_content, file_content, "File content mismatch");

    // Final state
    print_info("Final state verification");
    let final_status = get_status(&env);
    print_layer_state_from_status(&final_status);

    print_test_result(true, "test_complete_edit_workflow", None);
}

#[test]
fn test_multiple_commits() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_multiple_commits");

    print_info("Step 1: Initialize repository");
    init_repository(&env);
    print_success("Repository initialized");

    // First commit
    print_info("Step 2: First edit and commit");
    apply_edit(&env, "test.txt", "Line 1\nLine 2\n");
    commit_changes(&env, "First commit", "user-1");
    print_success("First commit completed");

    // Second commit
    print_info("Step 3: Second edit and commit");
    apply_edit(&env, "test.txt", "Line 1\nLine 2\nLine 3\n");
    commit_changes(&env, "Second commit", "user-1");
    print_success("Second commit completed");

    // Third commit
    print_info("Step 4: Third edit and commit");
    apply_edit(&env, "test.txt", "Line 1\nModified line 2\nLine 3\n");
    commit_changes(&env, "Third commit", "user-1");
    print_success("Third commit completed");

    // Verify log
    print_info("Step 5: Verify commit history");
    let log = get_log(&env, Some(10));
    print_checkpoint_log(&log);

    assert_log_entry_count(&env, 3);
    assert_log_contains(&env, "First commit");
    assert_log_contains(&env, "Second commit");
    assert_log_contains(&env, "Third commit");

    // Final state
    print_info("Final state verification");
    let status = get_status(&env);
    print_layer_state_from_status(&status);

    print_test_result(true, "test_multiple_commits", None);
}

#[test]
fn test_manual_to_staged_flow() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_manual_to_staged_flow");

    print_info("Step 1: Initialize repository");
    init_repository(&env);
    print_success("Repository initialized");

    // Apply manual edit
    print_info("Step 2: Apply manual edit");
    let content = "Manual edit content\nLine 2\nLine 3";
    let snapshot_id = apply_edit(&env, "manual.txt", content);
    print_success(&format!(
        "Manual edit applied, snapshot_id: {}",
        snapshot_id.to_hex()
    ));

    // Verify manual_edit layer
    print_info("Step 3: Verify manual_edit layer");
    let manual_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::ManualEdit);
    assert!(
        !manual_partitions.is_empty(),
        "manual_edit layer should have partitions"
    );

    for partition in &manual_partitions {
        print_info(&format!(
            "  - Partition: {}, current_snapshot: {}",
            partition.name,
            truncate_id(&partition.current_snapshot.to_hex())
        ));
    }

    // Commit to staged
    print_info("Step 4: Commit to staged");
    commit_changes(&env, "Manual edit commit", "user-1");
    print_success("Committed to staged");

    // Verify staged layer
    print_info("Step 5: Verify staged layer");
    let staged_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::Staged);
    assert!(
        !staged_partitions.is_empty(),
        "staged layer should have partitions"
    );

    for partition in &staged_partitions {
        print_info(&format!(
            "  - Partition: {}, current_snapshot: {}",
            partition.name,
            truncate_id(&partition.current_snapshot.to_hex())
        ));
    }

    // Verify content reconstruction
    print_info("Step 6: Verify content reconstruction");
    let reconstructed = reconstruct_text(&env, &snapshot_id);
    assert!(reconstructed.is_some(), "Failed to reconstruct text");

    let actual_content = reconstructed.unwrap();
    print_file_content(&actual_content, 3);
    assert_eq!(actual_content, content, "Content mismatch");

    // Final state
    print_info("Final state verification");
    let all_partitions = env
        .storage
        .list_partitions()
        .expect("Failed to list partitions");
    print_all_layer_states(&all_partitions);

    print_test_result(true, "test_manual_to_staged_flow", None);
}

#[test]
fn test_file_content_evolution() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_file_content_evolution");

    print_info("Step 1: Initialize repository with initial content");
    init_repository(&env);
    let v1_snapshot = apply_edit(&env, "evolution.txt", "Version 1\n");
    commit_changes(&env, "Add version 1", "user-1");
    print_success("Version 1 committed");

    let v1_content = reconstruct_text(&env, &v1_snapshot).expect("Failed to reconstruct V1");
    print_info("V1 content:");
    print_file_content(&v1_content, 5);

    // V2: Add more lines
    print_info("Step 2: Add more lines");
    let v2_content = "Version 1\nVersion 2\nVersion 3";
    let v2_snapshot = apply_edit(&env, "evolution.txt", v2_content);
    commit_changes(&env, "Add versions 2 and 3", "user-1");
    print_success("Version 2 committed");

    let reconstructed_v2 = reconstruct_text(&env, &v2_snapshot).expect("Failed to reconstruct V2");
    print_info("V2 content:");
    print_file_content(&reconstructed_v2, 5);
    assert_eq!(reconstructed_v2, v2_content, "V2 content mismatch");

    // V3: Modify lines
    print_info("Step 3: Modify existing lines");
    let v3_content = "Modified Version 1\nModified Version 2\nModified Version 3";
    let v3_snapshot = apply_edit(&env, "evolution.txt", v3_content);
    commit_changes(&env, "Modify all lines", "user-1");
    print_success("Version 3 committed");

    let reconstructed_v3 = reconstruct_text(&env, &v3_snapshot).expect("Failed to reconstruct V3");
    print_info("V3 content:");
    print_file_content(&reconstructed_v3, 5);
    assert_eq!(reconstructed_v3, v3_content, "V3 content mismatch");

    // Verify all versions are different
    print_info("Step 4: Verify snapshot IDs are different");
    assert_snapshots_different(&v1_snapshot, &v2_snapshot);
    assert_snapshots_different(&v2_snapshot, &v3_snapshot);
    assert_snapshots_different(&v1_snapshot, &v3_snapshot);
    print_success("All snapshot IDs are unique");

    // Verify log history
    print_info("Step 5: Verify commit history");
    let log = get_log(&env, None);
    print_checkpoint_log(&log);
    assert_log_entry_count(&env, 3);

    print_test_result(true, "test_file_content_evolution", None);
}

#[test]
fn test_empty_to_content_flow() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_empty_to_content_flow");

    print_info("Step 1: Initialize repository");
    init_repository(&env);
    print_success("Repository initialized");

    // Start with empty file
    print_info("Step 2: Create empty file");
    let empty_snapshot = apply_edit(&env, "empty.txt", "");
    commit_changes(&env, "Create empty file", "user-1");
    print_success("Empty file created");

    let empty_content =
        reconstruct_text(&env, &empty_snapshot).expect("Failed to reconstruct empty");
    assert!(empty_content.is_empty(), "Empty file should be empty");
    print_info("Empty file verified");

    // Add content
    print_info("Step 3: Add content to empty file");
    let content = "First line\nSecond line\nThird line";
    let content_snapshot = apply_edit(&env, "empty.txt", content);
    commit_changes(&env, "Add content", "user-1");
    print_success("Content added");

    let reconstructed_content =
        reconstruct_text(&env, &content_snapshot).expect("Failed to reconstruct content");
    print_file_content(&reconstructed_content, 5);
    assert_eq!(reconstructed_content, content, "Content mismatch");

    // Verify snapshots are different
    print_info("Step 4: Verify snapshots are different");
    assert_snapshots_different(&empty_snapshot, &content_snapshot);
    print_success("Snapshots are different");

    // Verify log
    print_info("Step 5: Verify log history");
    let log = get_log(&env, None);
    print_checkpoint_log(&log);
    assert_log_entry_count(&env, 2);

    print_test_result(true, "test_empty_to_content_flow", None);
}

fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}
