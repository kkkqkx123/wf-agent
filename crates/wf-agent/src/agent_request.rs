use wf_tools::registry::ToolRegistry;
use wf_types::llm::LlmRequest;

use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

/// Build the LLM request for one agent iteration.
///
/// Single request construction point shared by blocking and streaming
/// iteration modes. Sampling parameters are deliberately not hardcoded
/// here: they come from the referenced profile's `parameters` and are
/// merged by the gateway.
pub async fn build_agent_request(
    entity: &AgentLoopEntity,
    tool_registry: &ToolRegistry,
    stream: bool,
) -> AgentResult<LlmRequest> {
    let messages = entity.conversation().read().await.messages().to_vec();
    let available_tools = entity.get_available_tools(tool_registry);
    let tools = if available_tools.is_empty() {
        None
    } else {
        Some(available_tools)
    };

    Ok(LlmRequest {
        profile_id: entity.model().to_string(),
        messages,
        parameters: None,
        tools,
        tool_call_format: entity
            .tool_call_format()
            .map(|config| config.format.clone()),
        locked_tool_call_format: entity.tool_call_format().cloned(),
        violation_policy: None,
        execution_id: Some(entity.id().to_string()),
        stream: Some(stream),
        dead_loop_detection: None,
        protocol_auto_converted: None,
    })
}
