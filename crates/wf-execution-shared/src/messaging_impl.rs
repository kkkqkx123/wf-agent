use crate::error::ExecutionSharedError;
use crate::types::state_manager::StateManager;
use wf_llm::messaging::conversation_session::{ConversationSession, ConversationState};

impl StateManager<ConversationState> for ConversationSession {
    async fn cleanup(&mut self) -> Result<(), ExecutionSharedError> {
        self.reset();
        Ok(())
    }

    async fn create_snapshot(&self) -> Result<ConversationState, ExecutionSharedError> {
        Ok(self.snapshot_state())
    }

    async fn restore_from_snapshot(
        &mut self,
        snapshot: ConversationState,
    ) -> Result<(), ExecutionSharedError> {
        self.restore_state(snapshot);
        Ok(())
    }

    fn size(&self) -> usize {
        self.state.messages.len()
    }

    fn is_empty(&self) -> bool {
        self.state.messages.is_empty()
    }
}
