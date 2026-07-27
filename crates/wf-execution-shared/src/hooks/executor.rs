use std::collections::HashMap;
use std::sync::Arc;

use futures::future::join_all;

use crate::condition::ConditionEvaluator;
use crate::error::{ExecutionSharedError, ExecutionSharedResult};
use crate::hooks::types::{BaseHookContext, BaseHookDefinition, HookExecutionResult, HookExecutorConfig};

pub type HookHandler =
    Arc<dyn Fn(BaseHookContext) -> futures::future::BoxFuture<'static, HookExecutionResult> + Send + Sync>;

pub struct HookExecutor {
    handlers: HashMap<String, HookHandler>,
}

impl HookExecutor {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register_handler(&mut self, hook_type: impl Into<String>, handler: HookHandler) {
        self.handlers.insert(hook_type.into(), handler);
    }

    pub fn filter_and_sort_hooks(
        hooks: &[BaseHookDefinition],
        hook_type: &str,
    ) -> Vec<BaseHookDefinition> {
        let mut filtered: Vec<_> = hooks
            .iter()
            .filter(|h| h.hook_type == hook_type && h.enabled)
            .cloned()
            .collect();
        filtered.sort_by_key(|h| std::cmp::Reverse(h.weight));
        filtered
    }

    pub fn evaluate_hook_condition(
        condition: Option<&str>,
        context: &HashMap<String, serde_json::Value>,
    ) -> ExecutionSharedResult<bool> {
        match condition {
            None => Ok(true),
            Some(cond) => ConditionEvaluator::evaluate(cond, context),
        }
    }

    pub async fn execute_single_hook(
        hook: &BaseHookDefinition,
        ctx: &BaseHookContext,
    ) -> HookExecutionResult {
        let handler_fn = Self::resolve_handler(&hook.hook_type);
        match handler_fn {
            Some(handler) => handler(ctx.clone()).await,
            None => HookExecutionResult {
                hook_id: hook.id.clone(),
                success: false,
                error: Some(format!("no handler registered for hook type '{}'", hook.hook_type)),
            },
        }
    }

    fn resolve_handler(hook_type: &str) -> Option<HookHandler> {
        let handlers: HashMap<String, HookHandler> = HashMap::new();
        handlers.get(hook_type).cloned()
    }

    pub async fn execute_hooks(
        hooks: &[BaseHookDefinition],
        ctx: &BaseHookContext,
        config: &HookExecutorConfig,
    ) -> ExecutionSharedResult<Vec<HookExecutionResult>> {
        let executable_hooks: Vec<_> = hooks
            .iter()
            .filter(|h| {
                Self::evaluate_hook_condition(h.condition.as_deref(), &ctx.data).unwrap_or(false)
            })
            .collect();

        if executable_hooks.is_empty() {
            return Ok(Vec::new());
        }

        let results = if config.parallel {
            let futures: Vec<_> = executable_hooks
                .iter()
                .map(|h| Self::execute_single_hook(h, ctx))
                .collect();
            join_all(futures).await
        } else {
            let mut results = Vec::with_capacity(executable_hooks.len());
            for hook in &executable_hooks {
                results.push(Self::execute_single_hook(hook, ctx).await);
            }
            results
        };

        if !config.continue_on_error {
            for result in &results {
                if !result.success {
                    return Err(ExecutionSharedError::HookError(format!(
                        "hook '{}' failed: {}",
                        result.hook_id,
                        result.error.as_deref().unwrap_or("unknown error")
                    )));
                }
            }
        }

        Ok(results)
    }
}

impl Default for HookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::Id;

    fn make_hook(id: &str, hook_type: &str, weight: i32, enabled: bool) -> BaseHookDefinition {
        BaseHookDefinition {
            id: id.to_string(),
            hook_type: hook_type.to_string(),
            weight,
            condition: None,
            enabled,
            parallel: false,
            continue_on_error: false,
        }
    }

    #[test]
    fn test_filter_by_type() {
        let hooks = vec![
            make_hook("1", "before_iteration", 10, true),
            make_hook("2", "after_iteration", 5, true),
            make_hook("3", "before_iteration", 1, true),
        ];
        let filtered = HookExecutor::filter_and_sort_hooks(&hooks, "before_iteration");
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].id, "1");
        assert_eq!(filtered[1].id, "3");
    }

    #[test]
    fn test_filter_disabled() {
        let hooks = vec![
            make_hook("1", "before_iteration", 10, false),
            make_hook("2", "before_iteration", 5, true),
        ];
        let filtered = HookExecutor::filter_and_sort_hooks(&hooks, "before_iteration");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "2");
    }

    #[test]
    fn test_sort_by_weight_desc() {
        let hooks = vec![
            make_hook("1", "before_iteration", 1, true),
            make_hook("2", "before_iteration", 100, true),
            make_hook("3", "before_iteration", 50, true),
        ];
        let filtered = HookExecutor::filter_and_sort_hooks(&hooks, "before_iteration");
        assert_eq!(filtered[0].id, "2");
        assert_eq!(filtered[1].id, "3");
        assert_eq!(filtered[2].id, "1");
    }

    #[test]
    fn test_evaluate_condition_none() {
        let ctx = HashMap::new();
        assert!(HookExecutor::evaluate_hook_condition(None, &ctx).unwrap());
    }

    #[test]
    fn test_evaluate_condition_true() {
        let mut ctx = HashMap::new();
        ctx.insert("flag".to_string(), serde_json::Value::Bool(true));
        assert!(HookExecutor::evaluate_hook_condition(Some("flag"), &ctx).unwrap());
    }

    #[test]
    fn test_evaluate_condition_false() {
        let ctx = HashMap::new();
        assert!(!HookExecutor::evaluate_hook_condition(Some("missing"), &ctx).unwrap());
    }

    #[tokio::test]
    async fn test_execute_hooks_empty() {
        let ctx = BaseHookContext {
            execution_id: Id::new(),
            data: HashMap::new(),
        };
        let config = HookExecutorConfig::default();
        let results = HookExecutor::execute_hooks(&[], &ctx, &config).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_execute_hooks_condition_filters() {
        let hooks = vec![BaseHookDefinition {
            id: Id::new(),
            hook_type: "test".to_string(),
            weight: 1,
            condition: Some("missing_var".to_string()),
            enabled: true,
            parallel: false,
            continue_on_error: false,
        }];
        let ctx = BaseHookContext {
            execution_id: Id::new(),
            data: HashMap::new(),
        };
        let config = HookExecutorConfig::default();
        let results = HookExecutor::execute_hooks(&hooks, &ctx, &config).await.unwrap();
        assert!(results.is_empty());
    }
}
