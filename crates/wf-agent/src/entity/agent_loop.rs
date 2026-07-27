use std::sync::Arc;

use wf_execution_shared::interruption::InterruptionState;
use wf_execution_shared::messaging::conversation_session::ConversationSession;
use wf_execution_shared::types::execution_entity::{ExecutionStatus, IExecutionEntity};
use wf_types::Id;

use crate::error::{AgentError, AgentResult};
use crate::state::agent_loop_state::AgentLoopState;

pub struct AgentLoopEntity {
    id: Id,
    pub state: Arc<tokio::sync::RwLock<AgentLoopState>>,
    interruption: InterruptionState,
    conversation: Arc<tokio::sync::RwLock<ConversationSession>>,
    cancellation: tokio_util::sync::CancellationToken,
}

impl AgentLoopEntity {
    pub fn new(id: Id) -> Self {
        Self {
            id,
            state: Arc::new(tokio::sync::RwLock::new(AgentLoopState::new())),
            interruption: InterruptionState::new(),
            conversation: Arc::new(tokio::sync::RwLock::new(ConversationSession::new())),
            cancellation: tokio_util::sync::CancellationToken::new(),
        }
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn conversation(&self) -> &Arc<tokio::sync::RwLock<ConversationSession>> {
        &self.conversation
    }

    pub fn interruption(&self) -> &InterruptionState {
        &self.interruption
    }
}

#[async_trait::async_trait]
impl IExecutionEntity for AgentLoopEntity {
    fn id(&self) -> &Id {
        &self.id
    }

    fn status(&self) -> ExecutionStatus {
        futures::executor::block_on(async {
            self.state.read().await.status()
        })
    }

    fn is_running(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_running()
        })
    }

    fn is_paused(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_paused()
        })
    }

    fn is_completed(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_completed()
        })
    }

    fn is_failed(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_failed()
        })
    }

    fn is_cancelled(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_cancelled()
        })
    }

    async fn pause(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.pause()?;
        self.state.write().await.pause();
        Ok(())
    }

    async fn resume(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.state.write().await.resume();
        Ok(())
    }

    async fn stop(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.stop()?;
        self.cancellation.cancel();
        self.state.write().await.cancel();
        Ok(())
    }

    async fn abort(&self) {
        self.cancellation.cancel();
    }

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken {
        self.cancellation.clone()
    }

    fn get_hierarchy_depth(&self) -> u32 {
        0
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        Some(self.id.clone())
    }
}
