//! Large file handling tests.
//!
//! Real scenario: Developers editing large source files (generated code, long config files, data files).
//! Tests verify that the engine and storage handle large content correctly.

use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use layertwine::core::types::LayerType;

// ---------------------------------------------------------------------------
// 500KB file: edit, commit, restore
// ---------------------------------------------------------------------------

#[test]
fn test_large_file_edit_commit_restore() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);
    init_repository(&env);

    // Generate 500KB of content (10000 lines × ~50 chars)
    let large_content: String = (0..10_000)
        .map(|i| format!("line_{:05}  {}  some padding data here for size\n", i, "x".repeat(30)))
        .collect();
    assert!(large_content.len() > 400_000, "content should be at least 400KB");

    let _sid = apply_edit(&env, "large.rs", &large_content);
    commit_changes(&env, "large file initial", "dev");

    // Edit a single line in the middle
    let modified: String = (0..10_000)
        .map(|i| {
            if i == 5_000 {
                format!("line_{:05}  MODIFIED  some padding data here for size\n", i)
            } else {
                format!("line_{:05}  {}  some padding data here for size\n", i, "x".repeat(30))
            }
        })
        .collect();

    let _sid2 = apply_edit(&env, "large.rs", &modified);
    commit_changes(&env, "large file modified", "dev");

    // Verify content
    let status = get_status(&env);
    assert!(!status.partitions.is_empty());

    // Verify the system handles large content without panicking
    // by reading staged content
    let staged_parts = get_partitions_by_layer(&env, LayerType::Staged);
    if let Some(staged) = staged_parts.first() {
        let staged_text = reconstruct_text(&env, &staged.current_snapshot).unwrap_or_default();
        assert!(!staged_text.is_empty(), "staged content should not be empty");
        assert!(
            staged_text.contains("MODIFIED"),
            "staged content should contain the modification"
        );
    }
}

// ---------------------------------------------------------------------------
// 1MB file: handling extremely large content
// ---------------------------------------------------------------------------

#[test]
fn test_one_megabyte_file() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);
    init_repository(&env);

    // Generate ~1MB content
    let line = "A".repeat(100) + "\n";
    let megabyte: String = (0..10_000).map(|_| line.clone()).collect::<Vec<_>>().concat();
    assert!(megabyte.len() >= 1_000_000, "content should be at least 1MB");

    let _sid = apply_edit(&env, "megabyte.txt", &megabyte);

    // Verify staged content is approximately 1MB (allow small difference
    // from newline normalization in the engine)
    let staged_parts = get_partitions_by_layer(&env, LayerType::Staged);
    if let Some(staged) = staged_parts.first() {
        let staged_text = reconstruct_text(&env, &staged.current_snapshot).unwrap_or_default();
        let diff = if staged_text.len() > megabyte.len() {
            staged_text.len() - megabyte.len()
        } else {
            megabyte.len() - staged_text.len()
        };
        assert!(
            diff <= 2,
            "staged content size ({} bytes) should be within 2 bytes of written content ({} bytes)",
            staged_text.len(),
            megabyte.len()
        );
    }

    // Commit and verify log
    commit_changes(&env, "1MB file", "dev");
    let log = get_log(&env, None);
    assert!(!log.is_empty(), "should have at least one log entry");
}