//! Helper functions for E2E tests
#![allow(dead_code, unused_imports, unused_variables)]

use crate::common::fixture::TestEnvironment;
use layertwine::api::{
    AgentEditRequest, AgentSubmitRequest, ApiService, ApproveAgentRequest, CommitRequest,
    EditRequest, InitRequest, LogRequest, StatusResponse,
};
use layertwine::core::partition::Partition;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::LayerType;
use layertwine::core::types::SnapshotId;
use layertwine::storage::repository::{PartitionStore, SnapshotStore};
use std::path::PathBuf;

/// Initialize a layertwine repository
pub fn init_repository(env: &TestEnvironment) {
    env.api
        .init(InitRequest {
            db_path: Some(env.db_path_str()),
            git_repo: env.git_repo_path(),
            git_ref: None,
        })
        .expect("Failed to initialize repository");
}

/// Apply a manual edit
pub fn apply_edit(env: &TestEnvironment, file: &str, content: &str) -> SnapshotId {
    let response = env
        .api
        .edit(EditRequest {
            file: file.to_string(),
            content: Some(content.to_string()),
        })
        .expect("Failed to apply edit");

    SnapshotId::from_hex(&response.snapshot_id).expect("Invalid snapshot ID")
}

/// Apply an agent edit
pub fn apply_agent_edit(
    env: &TestEnvironment,
    agent_id: &str,
    file: &str,
    content: &str,
) -> SnapshotId {
    let response = env
        .api
        .agent_edit(AgentEditRequest {
            agent_id: agent_id.to_string(),
            file: file.to_string(),
            content: Some(content.to_string()),
        })
        .expect("Failed to apply agent edit");

    SnapshotId::from_hex(&response.snapshot_id).expect("Invalid snapshot ID")
}

/// Submit agent changes
pub fn submit_agent(env: &TestEnvironment, agent_id: &str) -> SnapshotId {
    let response = env
        .api
        .agent_submit(AgentSubmitRequest {
            agent_id: agent_id.to_string(),
        })
        .expect("Failed to submit agent");

    SnapshotId::from_hex(&response.snapshot_id).expect("Invalid snapshot ID")
}

/// Approve an agent
pub fn approve_agent(env: &TestEnvironment, agent_id: &str, feature_name: &str) -> SnapshotId {
    let response = env
        .api
        .approve_agent(ApproveAgentRequest {
            agent_id: agent_id.to_string(),
            integrated_name: Some(feature_name.to_string()),
        })
        .expect("Failed to approve agent");

    SnapshotId::from_hex(&response.integrated_snapshot_id).expect("Invalid snapshot ID")
}

/// Merge integrated layers to unified layer
pub fn merge_to_unified(
    env: &TestEnvironment,
    integration_names: Option<Vec<String>>,
) -> SnapshotId {
    use layertwine::api::MergeToUnifiedRequest;
    let response = env
        .api
        .merge_to_unified(MergeToUnifiedRequest { integration_names })
        .expect("Failed to merge to unified");

    SnapshotId::from_hex(&response.unified_snapshot_id).expect("Invalid snapshot ID")
}

/// Merge unified layer to staged layer
pub fn merge_to_staged(env: &TestEnvironment) -> SnapshotId {
    use layertwine::api::MergeToStagedRequest;
    let response = env
        .api
        .merge_to_staged(MergeToStagedRequest {})
        .expect("Failed to merge to staged");

    SnapshotId::from_hex(&response.staged_snapshot_id).expect("Invalid snapshot ID")
}

/// Commit staged changes
pub fn commit_changes(env: &TestEnvironment, message: &str, author: &str) -> SnapshotId {
    let response = env
        .api
        .commit(CommitRequest {
            message: message.to_string(),
            author: Some(author.to_string()),
        })
        .expect("Failed to commit");

    SnapshotId::from_hex(&response.checkpoint_id).expect("Invalid snapshot ID")
}

/// Get log history
pub fn get_log(env: &TestEnvironment, count: Option<usize>) -> Vec<String> {
    let response = env
        .api
        .log(LogRequest { count })
        .expect("Failed to get log");

    response
        .checkpoints
        .iter()
        .map(|cp| format!("{}: {}", cp.id, cp.message))
        .collect()
}

/// Get log history excluding root checkpoint
pub fn get_log_excluding_root(env: &TestEnvironment, count: Option<usize>) -> Vec<String> {
    let response = env
        .api
        .log(LogRequest { count })
        .expect("Failed to get log");

    response
        .checkpoints
        .iter()
        .filter(|cp| !cp.message.eq_ignore_ascii_case("root checkpoint"))
        .map(|cp| format!("{}: {}", cp.id, cp.message))
        .collect()
}

/// Get current status
pub fn get_status(env: &TestEnvironment) -> StatusResponse {
    env.api.status().expect("Failed to get status")
}

/// Wait for a condition with timeout
pub fn wait_for_condition<F>(condition: F, timeout_ms: u64) -> bool
where
    F: Fn() -> bool,
{
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_millis(timeout_ms) {
        if condition() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

/// Create initial commit with content
pub fn setup_initial_content(env: &TestEnvironment, file: &str, content: &str) {
    init_repository(env);
    apply_edit(env, file, content);
    commit_changes(env, "Initial commit", "test-user");
}

/// Get partitions by layer type
pub fn get_partitions_by_layer(env: &TestEnvironment, layer_type: LayerType) -> Vec<Partition> {
    let all_partitions = env.storage.list_partitions().unwrap_or_default();

    all_partitions
        .into_iter()
        .filter(|p| p.partition_type.to_layer() == layer_type)
        .collect()
}

/// Count partitions in a layer
pub fn count_partitions(env: &TestEnvironment, layer_type: LayerType) -> usize {
    get_partitions_by_layer(env, layer_type).len()
}

/// Check if a partition exists
pub fn partition_exists(env: &TestEnvironment, name: &str) -> bool {
    env.storage
        .list_partitions()
        .map(|partitions| partitions.iter().any(|p| p.name == name))
        .unwrap_or(false)
}

/// Get a partition by name
pub fn get_partition(env: &TestEnvironment, name: &str) -> Option<Partition> {
    env.storage.get_partition_by_name(name).ok()
}

/// Reconstruct text from snapshot ID
pub fn reconstruct_text(env: &TestEnvironment, snapshot_id: &SnapshotId) -> Option<String> {
    let snapshot = env.storage.get_snapshot(snapshot_id).ok()?;
    crate::common::helpers::reconstruct_from_snapshot(&env.storage, &snapshot).ok()
}

/// Helper function to reconstruct text from snapshot
fn reconstruct_from_snapshot(
    storage: &layertwine::storage::SqliteStorage,
    snapshot: &Snapshot,
) -> Result<String, Box<dyn std::error::Error>> {
    use layertwine::layered::transition::reconstruct_text;

    let text = reconstruct_text(storage, snapshot)?;
    Ok(text)
}

/// Create a test file with content
pub fn create_test_file(env: &TestEnvironment, file_path: &str, content: &str) {
    let full_path = env.temp_dir.path().join(file_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create parent directory");
    }
    std::fs::write(&full_path, content).expect("Failed to write test file");
}

/// Read a test file
pub fn read_test_file(env: &TestEnvironment, file_path: &str) -> Option<String> {
    let full_path = env.temp_dir.path().join(file_path);
    std::fs::read_to_string(full_path).ok()
}

/// Run Git command in the test Git repository
pub fn run_git_command(env: &TestEnvironment, args: &[&str]) -> Result<String, String> {
    use std::process::Command;

    let git_repo = env
        .git_repo
        .as_ref()
        .ok_or_else(|| "No git repository configured".to_string())?;

    let output = Command::new("git")
        .args(args)
        .current_dir(git_repo)
        .output()
        .map_err(|e| format!("Git command failed: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Commit to Git repository
pub fn git_commit(env: &TestEnvironment, message: &str) -> Result<(), String> {
    run_git_command(env, &["add", "."])?;
    run_git_command(env, &["commit", "-m", message])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixture::TestConfig;

    #[test]
    fn test_init_repository() {
        let config = TestConfig::default();
        let env = TestEnvironment::new(config);

        init_repository(&env);

        let status = get_status(&env);
        assert!(!status.partitions.is_empty());
    }

    #[test]
    fn test_apply_edit() {
        let config = TestConfig::default();
        let env = TestEnvironment::new(config);

        init_repository(&env);
        let snapshot_id = apply_edit(&env, "test.txt", "Hello, World!\n");

        assert!(!snapshot_id.0.iter().all(|&b| b == 0));
    }
}
