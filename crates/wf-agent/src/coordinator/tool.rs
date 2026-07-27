use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct ToolExecutionCoordinator;

impl ToolExecutionCoordinator {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute_tool_calls(
        &self,
        _entity: &AgentLoopEntity,
        _tool_calls: &[wf_types::message::LlmToolCall],
    ) -> AgentResult<Vec<wf_types::message::Message>> {
        Ok(Vec::new())
    }
}
