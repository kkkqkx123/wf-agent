use std::sync::Arc;

use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_execution_shared::interruption::InterruptionState;
use wf_execution_shared::messaging::conversation_session::ConversationSession;
use wf_execution_shared::types::execution_entity::{ExecutionStatus, IExecutionEntity};
use wf_types::llm::ToolCallFormatConfig;
use wf_types::Id;

use crate::state::AgentLoopState;

pub struct AgentLoopEntity {
    id: Id,
    pub state: Arc<tokio::sync::RwLock<AgentLoopState>>,
    interruption: InterruptionState,
    conversation: Arc<tokio::sync::RwLock<ConversationSession>>,
    cancellation: tokio_util::sync::CancellationToken,
    parent_execution_id: Option<Id>,
    child_execution_ids: Arc<tokio::sync::RwLock<Vec<Id>>>,
    hooks: Vec<BaseHookDefinition>,
    model: Option<String>,
    tool_call_format: Option<ToolCallFormatConfig>,
    available_tool_names: Vec<String>,
}

impl AgentLoopEntity {
    pub fn new(id: Id) -> Self {
        Self {
            id,
            state: Arc::new(tokio::sync::RwLock::new(AgentLoopState::new())),
            interruption: InterruptionState::new(),
            conversation: Arc::new(tokio::sync::RwLock::new(ConversationSession::new())),
            cancellation: tokio_util::sync::CancellationToken::new(),
            parent_execution_id: None,
            child_execution_ids: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            hooks: Vec::new(),
            model: None,
            tool_call_format: None,
            available_tool_names: Vec::new(),
        }
    }

    pub fn with_parent_execution_id(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }

    pub fn with_hooks(mut self, hooks: Vec<BaseHookDefinition>) -> Self {
        self.hooks = hooks;
        self
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = Some(model);
        self
    }

    pub fn with_tool_call_format(mut self, format: ToolCallFormatConfig) -> Self {
        self.tool_call_format = Some(format);
        self
    }

    pub fn with_available_tool_names(mut self, names: Vec<String>) -> Self {
        self.available_tool_names = names;
        self
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

    pub fn hooks(&self) -> &[BaseHookDefinition] {
        &self.hooks
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn tool_call_format(&self) -> Option<&ToolCallFormatConfig> {
        self.tool_call_format.as_ref()
    }

    pub fn available_tool_names(&self) -> &[String] {
        &self.available_tool_names
    }

    pub fn parent_execution_id(&self) -> Option<&Id> {
        self.parent_execution_id.as_ref()
    }

    pub fn child_execution_ids(&self) -> &Arc<tokio::sync::RwLock<Vec<Id>>> {
        &self.child_execution_ids
    }

    pub async fn register_child(&self, child_id: Id) {
        self.child_execution_ids.write().await.push(child_id);
    }

    pub async fn unregister_child(&self, child_id: &Id) {
        self.child_execution_ids.write().await.retain(|id| id != child_id);
    }

    pub fn get_available_tools(&self, registry: &wf_tools::registry::ToolRegistry) -> Vec<wf_types::tool::Tool> {
        let all_tools = registry.list_tools();
        if self.available_tool_names.is_empty() {
            return all_tools;
        }
        all_tools
            .into_iter()
            .filter(|t| self.available_tool_names.contains(&t.name))
            .collect()
    }
}

#[async_trait::async_trait]
impl IExecutionEntity for AgentLoopEntity {
    fn id(&self) -> &Id {
        &self.id
    }

    fn status(&self) -> ExecutionStatus {
        use tokio::runtime::Handle;
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { self.state.read().await.status() }))
        } else {
            ExecutionStatus::Running
        }
    }

    fn is_running(&self) -> bool {
        use tokio::runtime::Handle;
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { self.state.read().await.is_running() }))
        } else {
            false
        }
    }

    fn is_paused(&self) -> bool {
        use tokio::runtime::Handle;
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { self.state.read().await.is_paused() }))
        } else {
            false
        }
    }

    fn is_completed(&self) -> bool {
        use tokio::runtime::Handle;
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { self.state.read().await.is_completed() }))
        } else {
            false
        }
    }

    fn is_failed(&self) -> bool {
        use tokio::runtime::Handle;
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { self.state.read().await.is_failed() }))
        } else {
            false
        }
    }

    fn is_cancelled(&self) -> bool {
        use tokio::runtime::Handle;
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { self.state.read().await.is_cancelled() }))
        } else {
            false
        }
    }

    async fn pause(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.pause()?;
        self.state.write().await.pause();
        Ok(())
    }

    async fn resume(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.resume()?;
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
