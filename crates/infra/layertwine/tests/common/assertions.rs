//! Custom assertions for E2E tests
#![allow(dead_code, unused_imports, unused_variables)]

use crate::common::fixture::TestEnvironment;
use layertwine::core::types::{ContentId, LayerType, SnapshotId};

/// Assert that a snapshot ID is valid
pub fn assert_valid_snapshot_id(snapshot_id: &SnapshotId) {
    assert!(
        !snapshot_id.0.iter().all(|&b| b == 0),
        "Snapshot ID should not be all zeros"
    );
}

/// Assert that a partition exists
pub fn assert_partition_exists(env: &TestEnvironment, name: &str) {
    let exists = crate::common::helpers::partition_exists(env, name);
    assert!(exists, "Partition '{}' should exist", name);
}

/// Assert that a partition does not exist
pub fn assert_partition_not_exists(env: &TestEnvironment, name: &str) {
    let exists = crate::common::helpers::partition_exists(env, name);
    assert!(!exists, "Partition '{}' should not exist", name);
}

/// Assert that a layer has a specific number of partitions
pub fn assert_layer_partition_count(env: &TestEnvironment, layer_type: LayerType, expected: usize) {
    let layer_name = layer_type.name();
    let count = crate::common::helpers::count_partitions(env, layer_type.clone());
    assert_eq!(
        count, expected,
        "Layer '{}' should have {} partitions, but has {}",
        layer_name, expected, count
    );
}

/// Assert that log has expected number of entries (excluding root checkpoint)
pub fn assert_log_entry_count(env: &TestEnvironment, expected: usize) {
    let log = crate::common::helpers::get_log_excluding_root(env, None);
    assert_eq!(
        log.len(),
        expected,
        "Log should have {} entries (excluding root), but has {}",
        expected,
        log.len()
    );
}

/// Assert that log contains a specific message (excluding root checkpoint)
pub fn assert_log_contains(env: &TestEnvironment, message: &str) {
    let log = crate::common::helpers::get_log_excluding_root(env, None);
    let contains = log.iter().any(|entry| entry.contains(message));
    assert!(contains, "Log should contain message '{}'", message);
}

/// Assert that file content matches expected
pub fn assert_file_content(env: &TestEnvironment, snapshot_id: &SnapshotId, expected: &str) {
    let content = crate::common::helpers::reconstruct_text(env, snapshot_id);
    assert!(
        content.is_some(),
        "Failed to reconstruct text from snapshot"
    );

    let actual = content.unwrap();
    assert_eq!(
        actual, expected,
        "File content mismatch.\nExpected:\n{}\n\nActual:\n{}",
        expected, actual
    );
}

/// Assert that two snapshot IDs are different
pub fn assert_snapshots_different(id1: &SnapshotId, id2: &SnapshotId) {
    assert_ne!(id1, id2, "Snapshot IDs should be different");
}

/// Assert that two snapshot IDs are the same
pub fn assert_snapshots_equal(id1: &SnapshotId, id2: &SnapshotId) {
    assert_eq!(id1, id2, "Snapshot IDs should be equal");
}

/// Assert that Git repository exists
pub fn assert_git_repo_exists(env: &TestEnvironment) {
    assert!(
        env.git_repo.is_some(),
        "Git repository should be configured"
    );
    assert!(
        env.git_repo.as_ref().unwrap().exists(),
        "Git repository path should exist"
    );
}

/// Macro for running test with timing
#[macro_export]
macro_rules! run_e2e_test {
    ($name:expr, $test:block) => {{
        let start = std::time::Instant::now();
        let result = std::panic::catch_unwind(|| $test);
        let duration = start.elapsed();

        match result {
            Ok(_) => {
                $crate::common::output::print_test_result(true, $name, Some(duration));
                true
            }
            Err(_) => {
                $crate::common::output::print_test_result(false, $name, Some(duration));
                false
            }
        }
    }};
}

/// Macro for test steps with output
#[macro_export]
macro_rules! test_step {
    ($step_num:expr, $description:expr, $action:block) => {{
        $crate::common::output::print_info(&format!("Step {}: {}", $step_num, $description));
        let result = std::panic::catch_unwind(|| $action);

        match result {
            Ok(inner_result) => match inner_result {
                Ok(_) => {
                    $crate::common::output::print_success(&format!("Step {} completed", $step_num));
                    Ok(())
                }
                Err(e) => {
                    $crate::common::output::print_error(&format!(
                        "Step {} failed: {}",
                        $step_num, e
                    ));
                    Err(e)
                }
            },
            Err(_) => {
                $crate::common::output::print_error(&format!("Step {} panicked", $step_num));
                Err("Test panicked".into())
            }
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixture::TestConfig;

    #[test]
    fn test_assert_valid_snapshot_id() {
        let valid_id = ContentId([1u8; 32]);
        assert_valid_snapshot_id(&valid_id);

        let invalid_id = ContentId([0u8; 32]);
    }

    #[test]
    #[should_panic]
    fn test_assert_valid_snapshot_id_panics() {
        let invalid_id = ContentId([0u8; 32]);
        assert_valid_snapshot_id(&invalid_id);
    }
}
