use crate::error::ExecutionSharedError;
use crate::types::state_manager::StateManager;
use wf_llm::messaging::conversation_session::{ConversationSession, ConversationState};

impl StateManager<ConversationState> for ConversationSession {
    async fn cleanup(&mut self) -> Result<(), ExecutionSharedError> {
        self.state.messages.clear();
        self.state.token_usage = 0;
        Ok(())
    }

    async fn create_snapshot(&self) -> Result<ConversationState, ExecutionSharedError> {
        Ok(self.state.clone())
    }

    async fn restore_from_snapshot(
        &mut self,
        snapshot: ConversationState,
    ) -> Result<(), ExecutionSharedError> {
        self.state = snapshot;
        Ok(())
    }

    fn size(&self) -> usize {
        self.state.messages.len()
    }

    fn is_empty(&self) -> bool {
        self.state.messages.is_empty()
    }
}
