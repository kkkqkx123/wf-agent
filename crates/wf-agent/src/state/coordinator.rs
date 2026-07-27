use std::sync::Arc;

use wf_execution_shared::messaging::conversation_session::{ConversationSession, ConversationState};
use wf_execution_shared::types::state_manager::StateManager;

use crate::error::AgentResult;

pub struct AgentStateCoordinator {
    session: Arc<tokio::sync::RwLock<ConversationSession>>,
}

impl AgentStateCoordinator {
    pub fn new(session: Arc<tokio::sync::RwLock<ConversationSession>>) -> Self {
        Self { session }
    }

    pub async fn add_message(&self, message: wf_types::message::Message) {
        self.session.write().await.add_message(message);
    }

    pub async fn messages(&self) -> Vec<wf_types::message::Message> {
        self.session.read().await.messages().to_vec()
    }

    pub async fn snapshot(&self) -> AgentResult<ConversationState> {
        self.session.read().await.create_snapshot().await.map_err(Into::into)
    }

    pub async fn restore(&self, state: ConversationState) -> AgentResult<()> {
        self.session.write().await.restore_from_snapshot(state).await.map_err(Into::into)
    }
}
