use std::collections::HashMap;

use serde_json::Value;

use wf_execution_shared::error::ExecutionSharedResult;
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_execution_shared::hooks::types::{BaseHookContext, BaseHookDefinition, HookExecutorConfig, HookExecutionResult};
use wf_types::Id;

use crate::entity::WorkflowExecutionEntity;

pub struct WorkflowHookHandler;

impl WorkflowHookHandler {
    pub async fn execute_hooks(
        hook_executor: &HookExecutor,
        hooks: &[BaseHookDefinition],
        hook_type: &str,
        ctx: &BaseHookContext,
        config: &HookExecutorConfig,
    ) -> ExecutionSharedResult<Vec<HookExecutionResult>> {
        let matching = HookExecutor::filter_and_sort_hooks(hooks, hook_type);
        if matching.is_empty() {
            return Ok(Vec::new());
        }
        hook_executor.execute_hooks(&matching, ctx, config).await
    }

    pub fn build_base_hook_context(execution_id: Id, data: HashMap<String, Value>) -> BaseHookContext {
        BaseHookContext { execution_id, data }
    }

    pub async fn execute_workflow_hook(
        hook_executor: &HookExecutor,
        entity: &WorkflowExecutionEntity,
        hooks: &[BaseHookDefinition],
        hook_type: &str,
        extra_data: HashMap<String, Value>,
    ) -> ExecutionSharedResult<Vec<HookExecutionResult>> {
        let mut data = HashMap::new();
        data.insert("execution_id".to_string(), Value::String(entity.id().clone()));
        data.insert("workflow_id".to_string(), Value::String(entity.workflow_id().clone()));
        data.insert("status".to_string(), Value::String(format!("{:?}", entity.state.read().await.status())));
        data.extend(extra_data);

        let ctx = Self::build_base_hook_context(entity.id().clone(), data);
        let config = HookExecutorConfig {
            parallel: true,
            continue_on_error: true,
            warn_on_condition_failure: true,
        };
        Self::execute_hooks(hook_executor, hooks, hook_type, &ctx, &config).await
    }
}
