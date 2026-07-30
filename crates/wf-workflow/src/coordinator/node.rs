use std::collections::HashMap;

use wf_core::EventBus;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_execution_shared::hooks::types::{BaseHookContext, BaseHookDefinition, HookExecutorConfig};
use wf_core::interruption::check_execution_interruption;
use wf_common::retry::RetryBudget;
use wf_types::events::{BaseEvent, EventType};

use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct NodeCoordinator;

impl Default for NodeCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeCoordinator {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute_node(
        &self,
        entity: &WorkflowExecutionEntity,
        handler: &dyn NodeHandler,
        ctx: &mut NodeExecutionContext,
        event_bus: Option<&EventBus>,
        hooks: &[BaseHookDefinition],
        hook_executor: Option<&HookExecutor>,
    ) -> WorkflowResult<NodeExecutionResult> {
        let node_id = ctx.node_id.clone();
        let node_name = ctx.node_name.clone().unwrap_or_default();

        Self::emit_event(event_bus, EventType::NodeStarted, entity, &node_id, &serde_json::json!({
            "node_name": node_name,
            "node_type": format!("{:?}", ctx.node_type),
        })).await;

        Self::execute_hooks(hooks, hook_executor, entity, "BEFORE_EXECUTE").await;

        let check = check_execution_interruption(entity.interruption(), None);
        if !matches!(check, wf_core::types::interruption::ExecutionInterruptionCheckResult::Continue) {
            return Err(WorkflowError::CoordinatorError(
                format!("Execution interrupted before node {}: {:?}", node_id, check)
            ));
        }

        let result = handler.execute(ctx).await;

        match &result {
            Ok(output) => {
                entity.state.write().await.mark_node_completed(node_id.clone());

                Self::execute_hooks(hooks, hook_executor, entity, "AFTER_EXECUTE").await;

                Self::emit_event(event_bus, EventType::NodeCompleted, entity, &node_id, &serde_json::json!({
                    "has_next_nodes": !output.next_node_ids.is_empty(),
                    "node_name": node_name,
                })).await;

                Ok(NodeExecutionResult {
                    output: output.output.clone(),
                    next_node_ids: output.next_node_ids.clone(),
                    metadata: output.metadata.clone(),
                })
            }
            Err(e) => {
                Self::emit_event(event_bus, EventType::NodeFailed, entity, &node_id, &serde_json::json!({
                    "error": e.to_string(),
                    "node_name": node_name,
                })).await;

                Err(WorkflowError::NodeExecutionFailed {
                    node_id: node_id.clone(),
                    reason: e.to_string(),
                })
            }
        }
    }

    pub async fn execute_with_retry(
        &self,
        handler: &dyn NodeHandler,
        ctx: &mut NodeExecutionContext,
        retry_budget: &mut RetryBudget,
    ) -> WorkflowResult<NodeExecutionResult> {
        loop {
            match handler.execute(ctx).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !retry_budget.can_retry() {
                        return Err(e);
                    }
                    let delay = std::time::Duration::from_millis(1000 * 2_u64.pow(retry_budget.attempts()));
                    retry_budget.record_attempt(delay);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    async fn execute_hooks(
        hooks: &[BaseHookDefinition],
        hook_executor: Option<&HookExecutor>,
        entity: &WorkflowExecutionEntity,
        hook_type: &str,
    ) {
        let filtered = HookExecutor::filter_and_sort_hooks(hooks, hook_type);
        if filtered.is_empty() {
            return;
        }

        let executor = match hook_executor {
            Some(e) => e,
            None => return,
        };

        let mut data = std::collections::HashMap::new();
        data.insert("entity_id".to_string(), serde_json::Value::String(entity.id().to_string()));
        data.insert("hook_type".to_string(), serde_json::Value::String(hook_type.to_string()));
        let status = entity.state.read().await.status();
        data.insert("status".to_string(), serde_json::Value::String(format!("{:?}", status)));

        let hook_ctx = BaseHookContext {
            execution_id: entity.id().to_string(),
            data,
        };

        let config = HookExecutorConfig {
            parallel: false,
            continue_on_error: true,
            warn_on_condition_failure: false,
        };
        let _ = executor.execute_hooks(&filtered, &hook_ctx, &config).await;
    }

    async fn emit_event(event_bus: Option<&EventBus>, event_type: EventType, entity: &WorkflowExecutionEntity, node_id: &str, data: &serde_json::Value) {
        let Some(bus) = event_bus else { return };
        let mut metadata: HashMap<String, serde_json::Value> = data.as_object().map(|obj| {
            obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        }).unwrap_or_default();
        metadata.insert("node_id".to_string(), serde_json::Value::String(node_id.to_string()));

        let event = BaseEvent {
            id: wf_types::Id::new(),
            r#type: event_type,
            timestamp: wf_common::now(),
            workflow_id: Some(entity.workflow_id().clone()),
            execution_id: Some(entity.id().clone()),
            agent_loop_id: None,
            metadata: Some(metadata),
        };
        let _ = bus.publish(event);
    }
}
