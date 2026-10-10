use crate::coordinator::agent::AgentCheckpointCoordinator;
use crate::coordinator::child_restore::collect_child_restore;
use checkpoint_base::delta::CheckpointLoader;
use checkpoint_base::error::CheckpointError;
use checkpoint_state::restore::hierarchy::ChildDiscoverySummary;
use checkpoint_state::state::{AgentCheckpoint, AgentCheckpointStateManager};
use std::sync::Arc;

impl AgentCheckpointCoordinator {
    /// Load the checkpoint blob and bring it to the current format version.
    pub(super) async fn load_migrated(
        &self,
        checkpoint_id: &str,
    ) -> Result<AgentCheckpoint, CheckpointError> {
        crate::coordinator::migration::load_migrated(
            &self.state_manager,
            &self.version_manager,
            checkpoint_id,
        )
        .await
    }

    /// Post-restore phase: discover child executions through BFS
    /// `ChildDiscovery` plus the registered restore strategies.
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
                    AgentCheckpointStateManager::new(storage)
                        .load_checkpoint_data(&id)
                        .await
                }
            },
        )
        .await
    }
}
