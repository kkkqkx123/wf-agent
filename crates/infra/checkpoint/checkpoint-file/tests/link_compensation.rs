use checkpoint_base::error::CheckpointError;
use checkpoint_file::file::{
    FileCheckpointManager, FileCheckpointOptions, StateFileLinkStatus, StateFileRestorePolicy,
};

fn manager() -> FileCheckpointManager {
    FileCheckpointManager::new_in_memory().expect("in-memory manager")
}

#[test]
fn retry_recovers_multiple_pending_links() {
    let manager = manager();
    manager
        .record_pending_state_file_link("state-1", "file-1")
        .unwrap();
    manager
        .record_pending_state_file_link("state-2", "file-2")
        .unwrap();

    assert_eq!(manager.retry_pending_state_file_links().unwrap(), 2);
    assert_eq!(
        manager.state_file_link_status("state-1").unwrap(),
        StateFileLinkStatus::Linked {
            file_commit_id: "file-1".to_string()
        }
    );
    assert_eq!(
        manager.state_file_link_status("state-2").unwrap(),
        StateFileLinkStatus::Linked {
            file_commit_id: "file-2".to_string()
        }
    );
    assert_eq!(
        manager.retry_pending_state_file_links().unwrap(),
        0,
        "recovered links leave no pending work behind"
    );
}

#[test]
fn lenient_policy_returns_none_for_missing_link() {
    let manager = manager();
    let dir = tempfile::tempdir().unwrap();
    let outcome = manager
        .materialize_state_files_with_policy(
            "state-unknown",
            dir.path(),
            &FileCheckpointOptions::default(),
            StateFileRestorePolicy::Lenient,
        )
        .unwrap();
    assert!(
        outcome.is_none(),
        "lenient policy preserves the historical Ok(None) contract"
    );
}

#[test]
fn strict_policy_rejects_missing_link() {
    let manager = manager();
    let dir = tempfile::tempdir().unwrap();
    let err = manager
        .materialize_state_files_with_policy(
            "state-unknown",
            dir.path(),
            &FileCheckpointOptions::default(),
            StateFileRestorePolicy::Strict,
        )
        .unwrap_err();
    assert!(
        matches!(err, CheckpointError::Validation { .. }),
        "strict policy turns a missing link into an explicit error: {err:?}"
    );
}

#[test]
fn strict_policy_rejects_pending_link() {
    let manager = manager();
    let dir = tempfile::tempdir().unwrap();
    manager
        .record_pending_state_file_link("state-pending", "file-9")
        .unwrap();
    let err = manager
        .materialize_state_files_with_policy(
            "state-pending",
            dir.path(),
            &FileCheckpointOptions::default(),
            StateFileRestorePolicy::Strict,
        )
        .unwrap_err();
    assert!(
        matches!(err, CheckpointError::Validation { .. }),
        "a not-yet-durable link is not a restore source under Strict: {err:?}"
    );
}
