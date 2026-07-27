use async_trait::async_trait;

use wf_types::message::Message;

use crate::types::state_manager::StateManager;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    pub token_usage: u64,
}

pub struct ConversationSession {
    state: ConversationState,
}

impl Default for ConversationSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversationSession {
    pub fn new() -> Self {
        Self {
            state: ConversationState {
                messages: Vec::new(),
                token_usage: 0,
            },
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.state.messages.push(message);
    }

    pub fn messages(&self) -> &[Message] {
        &self.state.messages
    }

    pub fn token_usage(&self) -> u64 {
        self.state.token_usage
    }

    pub fn add_token_usage(&mut self, tokens: u64) {
        self.state.token_usage += tokens;
    }
}

#[async_trait]
impl StateManager<ConversationState> for ConversationSession {
    async fn cleanup(&mut self) -> Result<(), crate::error::ExecutionSharedError> {
        self.state.messages.clear();
        self.state.token_usage = 0;
        Ok(())
    }

    async fn create_snapshot(&self) -> Result<ConversationState, crate::error::ExecutionSharedError> {
        Ok(self.state.clone())
    }

    async fn restore_from_snapshot(&mut self, snapshot: ConversationState) -> Result<(), crate::error::ExecutionSharedError> {
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
