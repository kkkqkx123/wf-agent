use std::sync::Arc;

use wf_tools::callback::register_execution_callback;

use crate::executor::AgentLoopExecutor;

pub fn register_builtin_tools(
    gateway: Arc<wf_llm::LlmGateway>,
    registry: Arc<wf_tools::registry::ToolRegistry>,
) -> crate::error::AgentResult<()> {
    let executor = Arc::new(AgentLoopExecutor::new(gateway, registry));

    register_execution_callback(executor).map_err(|e| {
        crate::error::AgentError::Internal(format!("Failed to register callback: {}", e))
    })?;

    Ok(())
}
