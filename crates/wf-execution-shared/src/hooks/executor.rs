use crate::error::ExecutionSharedResult;
use crate::hooks::types::{BaseHookContext, BaseHookDefinition, HookExecutionResult, HookExecutorConfig};

pub struct HookExecutor;

impl HookExecutor {
    pub fn filter_and_sort_hooks(
        hooks: &[BaseHookDefinition],
        hook_type: &str,
    ) -> Vec<BaseHookDefinition> {
        let mut filtered: Vec<_> = hooks
            .iter()
            .filter(|h| h.hook_type == hook_type && h.enabled)
            .cloned()
            .collect();
        filtered.sort_by_key(|h| -h.weight);
        filtered
    }

    pub async fn execute_hooks(
        _hooks: &[BaseHookDefinition],
        _ctx: &BaseHookContext,
        _config: &HookExecutorConfig,
    ) -> ExecutionSharedResult<Vec<HookExecutionResult>> {
        Ok(Vec::new())
    }
}
