use wf_execution_shared::error::ExecutionSharedResult;
use wf_execution_shared::hooks::types::{BaseHookContext, BaseHookDefinition, HookExecutionResult, HookExecutorConfig};

pub struct AgentHookHandler;

impl AgentHookHandler {
    pub async fn execute_hooks(
        _hooks: &[BaseHookDefinition],
        _ctx: &BaseHookContext,
        _config: &HookExecutorConfig,
    ) -> ExecutionSharedResult<Vec<HookExecutionResult>> {
        Ok(Vec::new())
    }
}
