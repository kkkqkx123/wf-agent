use std::sync::Arc;

use async_trait::async_trait;

use wf_types::Id;

use crate::error::ExecutionSharedError;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExecutionStatus {
    Created,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Stopped,
    Timeout,
}

impl ExecutionStatus {
    /// Wire representation (matches the serde output of this type, which has
    /// no rename attribute — variant names verbatim).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Running => "Running",
            Self::Paused => "Paused",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
            Self::Stopped => "Stopped",
            Self::Timeout => "Timeout",
        }
    }

    /// Whether the execution has settled and will not transition again.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Stopped | Self::Timeout
        )
    }
}

impl From<ExecutionStatus> for wf_types::ExecutionStatus {
    /// Map the execution-engine status onto the persisted `wf-types` status.
    /// `Timeout` collapses onto `Failed`: the persisted contract has no
    /// timeout state, so an aborted-by-timeout execution reads as failed.
    fn from(status: ExecutionStatus) -> Self {
        match status {
            ExecutionStatus::Created => wf_types::ExecutionStatus::Created,
            ExecutionStatus::Running => wf_types::ExecutionStatus::Running,
            ExecutionStatus::Paused => wf_types::ExecutionStatus::Paused,
            ExecutionStatus::Completed => wf_types::ExecutionStatus::Completed,
            ExecutionStatus::Failed => wf_types::ExecutionStatus::Failed,
            ExecutionStatus::Cancelled => wf_types::ExecutionStatus::Cancelled,
            ExecutionStatus::Stopped => wf_types::ExecutionStatus::Stopped,
            ExecutionStatus::Timeout => wf_types::ExecutionStatus::Failed,
        }
    }
}

#[async_trait]
pub trait ExecutionEntity: Send + Sync {
    fn id(&self) -> &Id;
    fn status(&self) -> ExecutionStatus;
    fn is_running(&self) -> bool;
    fn is_paused(&self) -> bool;
    fn is_completed(&self) -> bool;
    fn is_failed(&self) -> bool;
    fn is_cancelled(&self) -> bool;

    async fn pause(&self) -> Result<(), ExecutionSharedError>;
    async fn resume(&self) -> Result<(), ExecutionSharedError>;
    async fn stop(&self) -> Result<(), ExecutionSharedError>;
    async fn abort(&self);

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken;
    fn get_hierarchy_depth(&self) -> u32;
    fn get_root_execution_id(&self) -> Option<Id>;
    /// Root-to-parent execution id chain (oldest first, excluding self).
    /// Empty when the run has no parent or the chain was not resolved at
    /// build time; used to propagate deep-hierarchy ancestry to children.
    fn get_ancestors(&self) -> Vec<Id> {
        Vec::new()
    }
}

/// Resolve the ancestor chain for a child of `parent`: the parent chain
/// plus the parent id, deduplicated at the tail.
pub fn child_ancestors(parent: &impl ExecutionEntity) -> Vec<Id> {
    let mut ancestors = parent.get_ancestors();
    let id = parent.id().clone();
    if ancestors.last() != Some(&id) {
        ancestors.push(id);
    }
    ancestors
}

/// Resolve the root id for a child of `parent`: the parent root when
/// known, otherwise the parent id itself.
pub fn child_root(parent: &impl ExecutionEntity) -> Id {
    parent
        .get_root_execution_id()
        .unwrap_or_else(|| parent.id().clone())
}

/// Resolve the hierarchy depth for a child of `parent`.
pub fn child_depth(parent: &impl ExecutionEntity) -> u32 {
    parent.get_hierarchy_depth().saturating_add(1)
}

/// Shared ownership forwards the control plane to the inner entity so
/// registries can store `Arc` handles without an extra wrapper type.
#[async_trait]
impl<T> ExecutionEntity for Arc<T>
where
    T: ExecutionEntity + ?Sized,
    Arc<T>: Send + Sync,
{
    fn id(&self) -> &Id {
        self.as_ref().id()
    }

    fn status(&self) -> ExecutionStatus {
        self.as_ref().status()
    }

    fn is_running(&self) -> bool {
        self.as_ref().is_running()
    }

    fn is_paused(&self) -> bool {
        self.as_ref().is_paused()
    }

    fn is_completed(&self) -> bool {
        self.as_ref().is_completed()
    }

    fn is_failed(&self) -> bool {
        self.as_ref().is_failed()
    }

    fn is_cancelled(&self) -> bool {
        self.as_ref().is_cancelled()
    }

    async fn pause(&self) -> Result<(), ExecutionSharedError> {
        self.as_ref().pause().await
    }

    async fn resume(&self) -> Result<(), ExecutionSharedError> {
        self.as_ref().resume().await
    }

    async fn stop(&self) -> Result<(), ExecutionSharedError> {
        self.as_ref().stop().await
    }

    async fn abort(&self) {
        self.as_ref().abort().await
    }

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken {
        self.as_ref().get_abort_signal()
    }

    fn get_hierarchy_depth(&self) -> u32 {
        self.as_ref().get_hierarchy_depth()
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        self.as_ref().get_root_execution_id()
    }

    fn get_ancestors(&self) -> Vec<Id> {
        self.as_ref().get_ancestors()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubEntity {
        id: Id,
        depth: u32,
        ancestors: Vec<Id>,
        root: Option<Id>,
    }

    #[async_trait]
    impl ExecutionEntity for StubEntity {
        fn id(&self) -> &Id {
            &self.id
        }

        fn status(&self) -> ExecutionStatus {
            ExecutionStatus::Running
        }

        fn is_running(&self) -> bool {
            true
        }

        fn is_paused(&self) -> bool {
            false
        }

        fn is_completed(&self) -> bool {
            false
        }

        fn is_failed(&self) -> bool {
            false
        }

        fn is_cancelled(&self) -> bool {
            false
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

        fn get_root_execution_id(&self) -> Option<Id> {
            self.root.clone()
        }

        fn get_ancestors(&self) -> Vec<Id> {
            self.ancestors.clone()
        }
    }

    fn stub(id: &str, depth: u32, ancestors: Vec<&str>, root: Option<&str>) -> StubEntity {
        StubEntity {
            id: Id::from(id.to_string()),
            depth,
            ancestors: ancestors
                .into_iter()
                .map(|a| Id::from(a.to_string()))
                .collect(),
            root: root.map(|r| Id::from(r.to_string())),
        }
    }

    #[test]
    fn child_link_helpers_extend_parent_chain() {
        let parent = stub("p", 1, vec!["root"], Some("root"));
        let ancestors = child_ancestors(&parent);
        let ids: Vec<String> = ancestors.iter().map(|id| id.as_str().to_string()).collect();
        assert_eq!(ids, vec!["root".to_string(), "p".to_string()]);
        assert_eq!(child_depth(&parent), 2);
        assert_eq!(child_root(&parent).as_str(), "root");
    }

    #[test]
    fn child_root_falls_back_to_parent_id() {
        let parent = stub("p", 0, vec![], None);
        assert_eq!(child_root(&parent).as_str(), "p");
    }
}
