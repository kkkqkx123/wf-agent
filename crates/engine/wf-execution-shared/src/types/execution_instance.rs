use wf_types::Id;

use super::execution_entity::{ExecutionEntity, ExecutionStatus};
use crate::error::ExecutionSharedError;

/// Which engine owns an [`ExecutionInstance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionKind {
    Agent,
    Workflow,
}

/// Control-plane handle over any live execution.
///
/// Both engines drive their own loops (iteration vs graph traversal) and keep
/// their own snapshots, so this handle never abstracts execution itself. It
/// only unifies what callers need to observe and control a run: identity,
/// status, pause/resume/stop/abort and hierarchy position.
///
/// The handle is generic over the two concrete entity types so the variant
/// tag cannot lie: an agent payload cannot be wrapped as a workflow and vice
/// versa. Each side dispatches statically to its concrete entity; no trait
/// object is involved. Snapshot and progress queries keep using the typed
/// entities; only the control plane goes through this handle.
#[derive(Clone)]
pub enum ExecutionInstance<A, W> {
    AgentLoop(A),
    Workflow(W),
}

impl<A, W> ExecutionInstance<A, W> {
    pub fn agent(entity: A) -> Self {
        Self::AgentLoop(entity)
    }

    pub fn workflow(entity: W) -> Self {
        Self::Workflow(entity)
    }

    pub fn kind(&self) -> ExecutionKind {
        match self {
            Self::AgentLoop(_) => ExecutionKind::Agent,
            Self::Workflow(_) => ExecutionKind::Workflow,
        }
    }

    pub fn is_agent(&self) -> bool {
        matches!(self, Self::AgentLoop(_))
    }

    pub fn is_workflow(&self) -> bool {
        matches!(self, Self::Workflow(_))
    }

    pub fn as_agent(&self) -> Option<&A> {
        match self {
            Self::AgentLoop(entity) => Some(entity),
            Self::Workflow(_) => None,
        }
    }

    pub fn as_workflow(&self) -> Option<&W> {
        match self {
            Self::AgentLoop(_) => None,
            Self::Workflow(entity) => Some(entity),
        }
    }

    pub fn is_terminal(&self) -> bool
    where
        A: ExecutionEntity,
        W: ExecutionEntity,
    {
        self.status().is_terminal()
    }
}

#[async_trait::async_trait]
impl<A, W> ExecutionEntity for ExecutionInstance<A, W>
where
    A: ExecutionEntity,
    W: ExecutionEntity,
{
    fn id(&self) -> &Id {
        match self {
            Self::AgentLoop(entity) => entity.id(),
            Self::Workflow(entity) => entity.id(),
        }
    }

    fn status(&self) -> ExecutionStatus {
        match self {
            Self::AgentLoop(entity) => entity.status(),
            Self::Workflow(entity) => entity.status(),
        }
    }

    fn is_running(&self) -> bool {
        match self {
            Self::AgentLoop(entity) => entity.is_running(),
            Self::Workflow(entity) => entity.is_running(),
        }
    }

    fn is_paused(&self) -> bool {
        match self {
            Self::AgentLoop(entity) => entity.is_paused(),
            Self::Workflow(entity) => entity.is_paused(),
        }
    }

    fn is_completed(&self) -> bool {
        match self {
            Self::AgentLoop(entity) => entity.is_completed(),
            Self::Workflow(entity) => entity.is_completed(),
        }
    }

    fn is_failed(&self) -> bool {
        match self {
            Self::AgentLoop(entity) => entity.is_failed(),
            Self::Workflow(entity) => entity.is_failed(),
        }
    }

    fn is_cancelled(&self) -> bool {
        match self {
            Self::AgentLoop(entity) => entity.is_cancelled(),
            Self::Workflow(entity) => entity.is_cancelled(),
        }
    }

    async fn pause(&self) -> Result<(), ExecutionSharedError> {
        match self {
            Self::AgentLoop(entity) => entity.pause().await,
            Self::Workflow(entity) => entity.pause().await,
        }
    }

    async fn resume(&self) -> Result<(), ExecutionSharedError> {
        match self {
            Self::AgentLoop(entity) => entity.resume().await,
            Self::Workflow(entity) => entity.resume().await,
        }
    }

    async fn stop(&self) -> Result<(), ExecutionSharedError> {
        match self {
            Self::AgentLoop(entity) => entity.stop().await,
            Self::Workflow(entity) => entity.stop().await,
        }
    }

    async fn abort(&self) {
        match self {
            Self::AgentLoop(entity) => entity.abort().await,
            Self::Workflow(entity) => entity.abort().await,
        }
    }

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken {
        match self {
            Self::AgentLoop(entity) => entity.get_abort_signal(),
            Self::Workflow(entity) => entity.get_abort_signal(),
        }
    }

    fn get_hierarchy_depth(&self) -> u32 {
        match self {
            Self::AgentLoop(entity) => entity.get_hierarchy_depth(),
            Self::Workflow(entity) => entity.get_hierarchy_depth(),
        }
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        match self {
            Self::AgentLoop(entity) => entity.get_root_execution_id(),
            Self::Workflow(entity) => entity.get_root_execution_id(),
        }
    }

    fn get_ancestors(&self) -> Vec<Id> {
        match self {
            Self::AgentLoop(entity) => entity.get_ancestors(),
            Self::Workflow(entity) => entity.get_ancestors(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    struct StubInner {
        id: Id,
        status: ExecutionStatus,
        depth: u32,
        ancestors: Vec<Id>,
    }

    struct AgentStub(StubInner);
    struct WorkflowStub(StubInner);

    fn inner(id: &str, ancestors: Vec<&str>) -> StubInner {
        StubInner {
            id: Id::from(id.to_string()),
            status: ExecutionStatus::Running,
            depth: ancestors.len() as u32,
            ancestors: ancestors
                .into_iter()
                .map(|a| Id::from(a.to_string()))
                .collect(),
        }
    }

    #[async_trait::async_trait]
    impl ExecutionEntity for AgentStub {
        fn id(&self) -> &Id {
            &self.0.id
        }

        fn status(&self) -> ExecutionStatus {
            self.0.status.clone()
        }

        fn is_running(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Running)
        }

        fn is_paused(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Paused)
        }

        fn is_completed(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Completed)
        }

        fn is_failed(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Failed)
        }

        fn is_cancelled(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Cancelled)
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
            self.0.depth
        }

        fn get_root_execution_id(&self) -> Option<Id> {
            self.0
                .ancestors
                .first()
                .cloned()
                .or_else(|| Some(self.0.id.clone()))
        }

        fn get_ancestors(&self) -> Vec<Id> {
            self.0.ancestors.clone()
        }
    }

    #[async_trait::async_trait]
    impl ExecutionEntity for WorkflowStub {
        fn id(&self) -> &Id {
            &self.0.id
        }

        fn status(&self) -> ExecutionStatus {
            self.0.status.clone()
        }

        fn is_running(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Running)
        }

        fn is_paused(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Paused)
        }

        fn is_completed(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Completed)
        }

        fn is_failed(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Failed)
        }

        fn is_cancelled(&self) -> bool {
            matches!(self.0.status, ExecutionStatus::Cancelled)
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
            self.0.depth
        }

        fn get_root_execution_id(&self) -> Option<Id> {
            self.0
                .ancestors
                .first()
                .cloned()
                .or_else(|| Some(self.0.id.clone()))
        }

        fn get_ancestors(&self) -> Vec<Id> {
            self.0.ancestors.clone()
        }
    }

    type Handle = ExecutionInstance<Arc<AgentStub>, Arc<WorkflowStub>>;

    fn agent_stub(id: &str, ancestors: Vec<&str>) -> Arc<AgentStub> {
        Arc::new(AgentStub(inner(id, ancestors)))
    }

    fn workflow_stub(id: &str, ancestors: Vec<&str>) -> Arc<WorkflowStub> {
        Arc::new(WorkflowStub(inner(id, ancestors)))
    }

    #[test]
    fn kind_tag_matches_typed_payload() {
        let agent = Handle::agent(agent_stub("a", vec![]));
        let workflow = Handle::workflow(workflow_stub("w", vec![]));
        assert_eq!(agent.kind(), ExecutionKind::Agent);
        assert_eq!(workflow.kind(), ExecutionKind::Workflow);
        assert!(agent.is_agent() && !agent.is_workflow());
        assert!(workflow.is_workflow() && !workflow.is_agent());
        assert!(agent.as_agent().is_some() && agent.as_workflow().is_none());
        assert!(workflow.as_workflow().is_some() && workflow.as_agent().is_none());
    }

    #[test]
    fn control_plane_delegates_to_inner() {
        let handle = Handle::workflow(workflow_stub("w", vec![]));
        assert_eq!(handle.id().as_str(), "w");
        assert!(handle.is_running());
        assert_eq!(handle.get_hierarchy_depth(), 0);
        assert_eq!(
            handle
                .get_root_execution_id()
                .map(|id| id.as_str().to_string()),
            Some("w".to_string())
        );
    }

    #[test]
    fn terminal_state_reads_through_handle() {
        let running = Handle::workflow(workflow_stub("w", vec![]));
        assert!(!running.is_terminal());
        let done = Arc::new(AgentStub(StubInner {
            id: Id::from("d".to_string()),
            status: ExecutionStatus::Completed,
            depth: 0,
            ancestors: Vec::new(),
        }));
        let handle = Handle::agent(done);
        assert!(handle.is_terminal());
    }

    #[tokio::test]
    async fn control_ops_forward_without_error() {
        let handle = Handle::agent(agent_stub("a", vec![]));
        handle.pause().await.expect("pause forwards");
        handle.resume().await.expect("resume forwards");
        handle.stop().await.expect("stop forwards");
        handle.abort().await;
    }
}
