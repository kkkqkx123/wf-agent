use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::error::ExecutionSharedError;
use wf_execution_shared::types::execution_entity::{
    child_ancestors, child_depth, child_root, ExecutionEntity, ExecutionStatus,
};

struct StubEntity {
    id: String,
    status: ExecutionStatus,
    depth: u32,
    ancestors: Vec<String>,
    root: Option<String>,
}

fn stub(
    id: &str,
    status: ExecutionStatus,
    depth: u32,
    ancestors: Vec<&str>,
    root: Option<&str>,
) -> StubEntity {
    StubEntity {
        id: id.to_string(),
        status,
        depth,
        ancestors: ancestors.into_iter().map(str::to_string).collect(),
        root: root.map(str::to_string),
    }
}

#[async_trait]
impl ExecutionEntity for StubEntity {
    fn id(&self) -> &String {
        &self.id
    }

    fn status(&self) -> ExecutionStatus {
        self.status.clone()
    }

    fn is_running(&self) -> bool {
        self.status == ExecutionStatus::Running
    }

    fn is_paused(&self) -> bool {
        self.status == ExecutionStatus::Paused
    }

    fn is_completed(&self) -> bool {
        self.status == ExecutionStatus::Completed
    }

    fn is_failed(&self) -> bool {
        self.status == ExecutionStatus::Failed
    }

    fn is_cancelled(&self) -> bool {
        self.status == ExecutionStatus::Cancelled
    }

    async fn pause(&self) -> Result<(), ExecutionSharedError> {
        Ok(())
    }

    async fn resume(&self) -> Result<(), ExecutionSharedError> {
        Ok(())
    }

    async fn stop(&self) -> Result<(), ExecutionSharedError> {
        Ok(())
    }

    async fn abort(&self) {}

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken {
        tokio_util::sync::CancellationToken::new()
    }

    fn get_hierarchy_depth(&self) -> u32 {
        self.depth
    }

    fn get_root_execution_id(&self) -> Option<String> {
        self.root.clone()
    }

    fn get_ancestors(&self) -> Vec<String> {
        self.ancestors.clone()
    }
}

#[test]
fn status_wire_names_match_serde_shape() {
    assert_eq!(ExecutionStatus::Created.as_str(), "Created");
    assert_eq!(ExecutionStatus::Running.as_str(), "Running");
    assert_eq!(ExecutionStatus::Paused.as_str(), "Paused");
    assert_eq!(ExecutionStatus::Completed.as_str(), "Completed");
    assert_eq!(ExecutionStatus::Failed.as_str(), "Failed");
    assert_eq!(ExecutionStatus::Cancelled.as_str(), "Cancelled");
    assert_eq!(ExecutionStatus::Stopped.as_str(), "Stopped");
    assert_eq!(ExecutionStatus::Timeout.as_str(), "Timeout");
}

#[test]
fn status_terminal_classification() {
    for status in [
        ExecutionStatus::Completed,
        ExecutionStatus::Failed,
        ExecutionStatus::Cancelled,
        ExecutionStatus::Stopped,
        ExecutionStatus::Timeout,
    ] {
        assert!(status.is_terminal(), "{status:?} is terminal");
    }
    for status in [
        ExecutionStatus::Created,
        ExecutionStatus::Running,
        ExecutionStatus::Paused,
    ] {
        assert!(!status.is_terminal(), "{status:?} is not terminal");
    }
}

#[test]
fn status_converts_to_persisted_shape_with_timeout_collapse() {
    assert_eq!(
        wf_types::ExecutionStatus::from(ExecutionStatus::Timeout),
        wf_types::ExecutionStatus::Failed
    );
    assert_eq!(
        wf_types::ExecutionStatus::from(ExecutionStatus::Completed),
        wf_types::ExecutionStatus::Completed
    );
    assert_eq!(
        wf_types::ExecutionStatus::from(ExecutionStatus::Running),
        wf_types::ExecutionStatus::Running
    );
}

#[test]
fn child_helpers_extend_parent_chain() {
    let parent = stub("p", ExecutionStatus::Running, 1, vec!["root"], Some("root"));
    assert_eq!(
        child_ancestors(&parent),
        vec!["root".to_string(), "p".to_string()]
    );
    assert_eq!(child_depth(&parent), 2);
    assert_eq!(child_root(&parent), "root".to_string());
}

#[test]
fn child_helpers_dedupe_tail_and_fall_back_to_parent_id() {
    let parent = stub("p", ExecutionStatus::Running, 0, vec!["p"], Some("root"));
    assert_eq!(child_ancestors(&parent), vec!["p".to_string()]);

    let orphan = stub("p", ExecutionStatus::Running, 0, vec![], None);
    assert_eq!(child_root(&orphan), "p".to_string());
}

#[tokio::test]
async fn arc_forwarding_delegates_control_plane() {
    let entity = Arc::new(stub("p", ExecutionStatus::Running, 0, vec![], None));
    assert_eq!(entity.id(), "p");
    assert!(entity.is_running());
    assert_eq!(entity.get_hierarchy_depth(), 0);
    entity.pause().await.expect("pause forwards");
    entity.resume().await.expect("resume forwards");
    entity.stop().await.expect("stop forwards");
    entity.abort().await;
}
