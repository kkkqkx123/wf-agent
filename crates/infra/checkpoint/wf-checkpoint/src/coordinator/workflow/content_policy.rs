use crate::coordinator::workflow::WorkflowCheckpointCoordinator;
use checkpoint_base::common::content::ContentFilter;
use checkpoint_base::error::CheckpointError;
use checkpoint_base::strategy::CheckpointStrategy;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;

impl WorkflowCheckpointCoordinator {
    /// Strip the snapshot domains the unified content policy excludes. Runs
    /// before any storage type decision so a filtered checkpoint still
    /// dedups identically: the progress coordinates are computed from the
    /// pre-policy snapshot on purpose.
    pub(super) fn apply_content_policy(
        &self,
        state: &mut WorkflowExecutionStateSnapshot,
    ) -> Result<(), CheckpointError> {
        if let Some(strategy) = &self.strategy {
            let filter = ContentFilter::new();
            let config = strategy.content_config();
            if !filter.should_include_state(config)? {
                state.input = None;
                state.output = None;
                state.node_results = None;
                state.messages = None;
                state.fork_join_context = None;
                state.active_operations = None;
                state.error_records = None;
                state.interruption_records = None;
                state.event_records = None;
                state.fork_join_aggregation_state = None;
                state.hook_execution_context = None;
                state.execution_config = None;
                state.conversation_state = None;
                state.trigger_states = None;
            }
            if !filter.should_include_history(config)? {
                state.messages = None;
            }
        }
        Ok(())
    }
}
