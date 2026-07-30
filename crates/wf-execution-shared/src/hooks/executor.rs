use std::collections::HashMap;
use std::sync::Arc;

use futures::future::join_all;
use tokio_util::sync::CancellationToken;
use wf_core::condition::ConditionEvaluator;
use wf_core::event::EventBus;
use wf_types::events::{BaseEvent, EventType};

use crate::error::{ExecutionSharedError, ExecutionSharedResult};
use crate::hooks::context_builder::HookContextBuilder;
use crate::hooks::template::resolve_payload_template;
use crate::hooks::types::{BaseHookContext, BaseHookDefinition, HookExecutionResult, HookExecutorConfig};

pub type HookHandler =
    Arc<dyn Fn(BaseHookContext) -> futures::future::BoxFuture<'static, HookExecutionResult> + Send + Sync>;

pub struct HookExecutor {
    handlers: HashMap<String, HookHandler>,
    context_builders: HashMap<String, Arc<dyn HookContextBuilder>>,
    event_bus: Option<Arc<EventBus>>,
}

impl HookExecutor {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            context_builders: HashMap::new(),
            event_bus: None,
        }
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub fn register_handler(&mut self, hook_type: impl Into<String>, handler: HookHandler) {
        self.handlers.insert(hook_type.into(), handler);
    }

    pub fn register_context_builder(
        &mut self,
        hook_type: impl Into<String>,
        builder: Arc<dyn HookContextBuilder>,
    ) {
        self.context_builders.insert(hook_type.into(), builder);
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
            Some(cond) => ConditionEvaluator::evaluate(cond, context)
                .map_err(|e| ExecutionSharedError::ConditionError(e.to_string())),
        }
    }

    pub async fn execute_single_hook(
        &self,
        hook: &BaseHookDefinition,
        ctx: &BaseHookContext,
    ) -> HookExecutionResult {
        let handler = self.resolve_handler(&hook.hook_type);
        match handler {
            Some(h) => h(ctx.clone()).await,
            None => HookExecutionResult {
                hook_id: hook.id.clone(),
                success: false,
                error: Some(format!("no handler registered for hook type '{}'", hook.hook_type)),
            },
        }
    }

    pub async fn execute_single_hook_with_context(
        &self,
        hook: &BaseHookDefinition,
        ctx: &BaseHookContext,
    ) -> ExecutionSharedResult<HookExecutionResult> {
        if let Err(e) = Self::evaluate_hook_condition(hook.condition.as_deref(), &ctx.data) {
            if hook.continue_on_error {
                return Ok(HookExecutionResult {
                    hook_id: hook.id.clone(),
                    success: false,
                    error: Some(format!("condition evaluation failed: {}", e)),
                });
            }
            return Err(e);
        }

        let eval_context = if let Some(builder) = self.context_builders.get(&hook.hook_type) {
            builder.build_context(ctx)
        } else {
            ctx.data.clone()
        };

        let handler = self.resolve_handler(&hook.hook_type);
        match handler {
            Some(h) => {
                let enriched_ctx = BaseHookContext {
                    execution_id: ctx.execution_id.clone(),
                    data: eval_context,
                };
                Ok(h(enriched_ctx).await)
            }
            None => Ok(HookExecutionResult {
                hook_id: hook.id.clone(),
                success: false,
                error: Some(format!("no handler registered for hook type '{}'", hook.hook_type)),
            }),
        }
    }

    pub async fn execute_hooks(
        &self,
        hooks: &[BaseHookDefinition],
        ctx: &BaseHookContext,
        config: &HookExecutorConfig,
    ) -> ExecutionSharedResult<Vec<HookExecutionResult>> {
        self.execute_hooks_with_cancellation(hooks, ctx, config, None).await
    }

    pub async fn execute_hooks_with_cancellation(
        &self,
        hooks: &[BaseHookDefinition],
        ctx: &BaseHookContext,
        config: &HookExecutorConfig,
        cancel_token: Option<&CancellationToken>,
    ) -> ExecutionSharedResult<Vec<HookExecutionResult>> {
        let executable_hooks: Vec<_> = hooks
            .iter()
            .filter(|h| Self::evaluate_hook_condition(h.condition.as_deref(), &ctx.data).unwrap_or(false))
            .collect();

        if executable_hooks.is_empty() {
            return Ok(Vec::new());
        }

        let results = if config.parallel {
            let futures: Vec<_> = executable_hooks
                .iter()
                .map(|h| self.execute_single_hook_with_context(h, ctx))
                .collect();
            let results = join_all(futures).await;
            let mut collected = Vec::with_capacity(results.len());
            for r in results {
                collected.push(r?);
            }
            collected
        } else {
            let mut results = Vec::with_capacity(executable_hooks.len());
            for hook in &executable_hooks {
                if let Some(token) = cancel_token {
                    if token.is_cancelled() {
                        return Err(ExecutionSharedError::InterruptionError(
                            "hook execution cancelled".to_string(),
                        ));
                    }
                }
                results.push(self.execute_single_hook_with_context(hook, ctx).await?);
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

        self.emit_hook_executed_event(&results);

        Ok(results)
    }

    pub fn resolve_payload_template(
        payload: &serde_json::Value,
        ctx: &BaseHookContext,
    ) -> ExecutionSharedResult<serde_json::Value> {
        resolve_payload_template_fn(payload, &ctx.data)
    }

    fn resolve_handler(&self, hook_type: &str) -> Option<HookHandler> {
        self.handlers.get(hook_type).cloned()
    }

    fn emit_hook_executed_event(&self, results: &[HookExecutionResult]) {
        if let Some(bus) = &self.event_bus {
            let all_success = results.iter().all(|r| r.success);
            let metadata_value = serde_json::json!({
                "hook_count": results.len(),
                "all_success": all_success,
                "results": results.iter().map(|r| serde_json::json!({
                    "hook_id": r.hook_id,
                    "success": r.success,
                    "error": r.error,
                })).collect::<Vec<_>>()
            });
            let metadata = metadata_value.as_object().map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<HashMap<_, _>>()
            });
            let event = BaseEvent {
                id: wf_types::Id::new(),
                r#type: EventType::AgentHookTriggered,
                timestamp: wf_common::now(),
                workflow_id: None,
                execution_id: None,
                agent_loop_id: None,
                metadata,
            };
            let _ = bus.publish(event);
        }
    }
}

impl Default for HookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_payload_template_fn(
    payload: &serde_json::Value,
    context: &HashMap<String, serde_json::Value>,
) -> ExecutionSharedResult<serde_json::Value> {
    resolve_payload_template(payload, context)
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
        let executor = HookExecutor::new();
        let ctx = BaseHookContext {
            execution_id: Id::new(),
            data: HashMap::new(),
        };
        let config = HookExecutorConfig::default();
        let results = executor.execute_hooks(&[], &ctx, &config).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_execute_hooks_condition_filters() {
        let executor = HookExecutor::new();
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
        let results = executor.execute_hooks(&hooks, &ctx, &config).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_register_and_execute_handler() {
        let mut executor = HookExecutor::new();
        executor.register_handler("test", Arc::new(|ctx| {
            Box::pin(async move {
                HookExecutionResult {
                    hook_id: ctx.execution_id.clone(),
                    success: true,
                    error: None,
                }
            })
        }));

        let hooks = vec![BaseHookDefinition {
            id: "hook-1".to_string(),
            hook_type: "test".to_string(),
            weight: 1,
            condition: None,
            enabled: true,
            parallel: false,
            continue_on_error: false,
        }];

        let ctx = BaseHookContext {
            execution_id: "exec-1".to_string(),
            data: HashMap::new(),
        };
        let config = HookExecutorConfig::default();
        let results = executor.execute_hooks(&hooks, &ctx, &config).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }

    #[test]
    fn test_resolve_payload_template_from_executor() {
        let mut ctx_data = HashMap::new();
        ctx_data.insert("name".to_string(), serde_json::Value::String("world".to_string()));
        let ctx = BaseHookContext {
            execution_id: Id::new(),
            data: ctx_data,
        };
        let payload = serde_json::Value::String("hello {{name}}".to_string());
        let result = HookExecutor::resolve_payload_template(&payload, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("hello world".to_string()));
    }

    #[tokio::test]
    async fn test_cancellation_stops_sequential_hooks() {
        let mut executor = HookExecutor::new();
        executor.register_handler("test", Arc::new(|_| {
            Box::pin(async move {
                HookExecutionResult {
                    hook_id: Id::new(),
                    success: true,
                    error: None,
                }
            })
        }));

        let hooks = vec![
            make_hook("1", "test", 3, true),
            make_hook("2", "test", 2, true),
            make_hook("3", "test", 1, true),
        ];

        let ctx = BaseHookContext {
            execution_id: Id::new(),
            data: HashMap::new(),
        };
        let config = HookExecutorConfig::default();
        let token = CancellationToken::new();
        token.cancel();

        let result = executor
            .execute_hooks_with_cancellation(&hooks, &ctx, &config, Some(&token))
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ExecutionSharedError::InterruptionError(_)
        ));
    }

    #[tokio::test]
    async fn test_event_bus_emits_on_hook_execution() {
        let bus = Arc::new(EventBus::new(16));
        let mut executor = HookExecutor::new();
        executor.set_event_bus(bus.clone());
        executor.register_handler("test", Arc::new(|_| {
            Box::pin(async move {
                HookExecutionResult {
                    hook_id: Id::new(),
                    success: true,
                    error: None,
                }
            })
        }));

        let hooks = vec![make_hook("1", "test", 1, true)];
        let ctx = BaseHookContext {
            execution_id: Id::new(),
            data: HashMap::new(),
        };
        let config = HookExecutorConfig::default();

        let mut sub = bus.subscribe();

        executor.execute_hooks(&hooks, &ctx, &config).await.unwrap();

        let event = sub.try_recv().unwrap();
        assert_eq!(event.r#type, EventType::AgentHookTriggered);
    }
}
