use crate::coordinator::child_restore::collect_child_restore;
use crate::coordinator::workflow::WorkflowCheckpointCoordinator;
use checkpoint_base::delta::CheckpointLoader;
use checkpoint_base::error::CheckpointError;
use checkpoint_state::restore::hierarchy::ChildDiscoverySummary;
use checkpoint_state::state::{WorkflowCheckpoint, WorkflowCheckpointStateManager};
use std::sync::Arc;

impl WorkflowCheckpointCoordinator {
    /// Load a checkpoint blob and bring it to the current format version.
    pub(super) async fn load_migrated(
        &self,
        checkpoint_id: &str,
    ) -> Result<WorkflowCheckpoint, CheckpointError> {
        crate::coordinator::migration::load_migrated(
            &self.state_manager,
            &self.version_manager,
            checkpoint_id,
        )
        .await
    }

    /// Post-restore phase: discover child executions. Latest checkpoints of
    /// child executions are resolved from storage with bounded concurrency,
    /// discovered via `ChildDiscovery`, and (when a restore strategy is
    /// registered for the child execution type) fully restored through the
    /// strategy registry.
    pub(super) async fn restore_child_hierarchy(
        &self,
        checkpoint_id: &str,
        parent_entity_id: &str,
    ) -> Result<ChildDiscoverySummary, CheckpointError> {
        let latest_by_child = self
            .state_manager
            .list_latest_by_parent(parent_entity_id)
            .await?;
        let storage = self.state_manager.storage().clone();
        collect_child_restore(
            latest_by_child,
            checkpoint_id,
            parent_entity_id,
            self.restore_registry.clone(),
            move |id: String| {
                let storage = Arc::clone(&storage);
                async move {
                    WorkflowCheckpointStateManager::new(storage)
                        .load_checkpoint_data(&id)
                        .await
                }
            },
        )
        .await
    }
}
