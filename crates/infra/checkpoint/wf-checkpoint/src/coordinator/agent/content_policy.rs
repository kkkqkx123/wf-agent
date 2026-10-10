use crate::coordinator::agent::AgentCheckpointCoordinator;
use checkpoint_base::common::content::ContentFilter;
use checkpoint_base::error::CheckpointError;
use checkpoint_base::strategy::CheckpointStrategy;
use wf_types::checkpoint::agent::AgentStateSnapshot;

impl AgentCheckpointCoordinator {
    /// Strip the snapshot domains the unified content policy excludes. Runs
    /// before any storage type decision so a filtered checkpoint still
    /// dedups identically: the progress coordinates are computed from the
    /// pre-policy snapshot on purpose.
    ///
    /// Conversation history is never silently dropped: even when the content
    /// policy excludes state or history, the message log stays so a restore
    /// never yields an empty dialogue. Only auxiliary fields are filtered.
    pub(super) fn apply_content_policy(
        &self,
        state: &mut AgentStateSnapshot,
    ) -> Result<(), CheckpointError> {
        if let Some(strategy) = &self.strategy {
            let filter = ContentFilter::new();
            let config = strategy.content_config();
            if !filter.should_include_state(config)? {
                state.tool_call_history = None;
                state.variable_snapshots = None;
                state.error = None;
                state.error_records = None;
                state.interruption_records = None;
                state.event_records = None;
                state.iteration_history = None;
                state.current_iteration_record = None;
                state.stream_message = None;
                state.pending_tool_call_ids = None;
                state.trigger_state = None;
            }
            if !filter.should_include_history(config)? {
                state.iteration_history = None;
            }
        }
        Ok(())
    }
}
