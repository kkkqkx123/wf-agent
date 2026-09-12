use std::collections::HashMap;

use wf_common::retry::RetryBudget;
use wf_core::EventBus;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::hooks::fire::FireSummary;
use wf_execution_shared::hooks::types::HookDefinition;
use wf_execution_shared::hooks::{HookContext, HookHandlerRegistry};
use wf_execution_shared::interruption::check_execution_interruption;
use wf_types::events::{BaseEvent, EventType};

use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct NodeCoordinator;

/// Node context attached to hook payloads (BEFORE_EXECUTE / AFTER_EXECUTE).
struct NodeHookPayload<'a> {
    node_id: &'a str,
    node_name: &'a str,
    node_type: &'a str,
    duration_ms: Option<i64>,
    error: Option<&'a str>,
}

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
        hooks: &[HookDefinition],
        hook_handler_registry: Option<&HookHandlerRegistry>,
    ) -> WorkflowResult<NodeExecutionResult> {
        let node_id = ctx.node_id.clone();
        let node_name = ctx.node_name.clone().unwrap_or_default();
        let node_type = format!("{:?}", ctx.node_type);
        let node_start = wf_common::now();

        Self::emit_event(
            event_bus,
            EventType::NodeStarted,
            entity,
            &node_id,
            &serde_json::json!({
                "node_name": node_name,
                "node_type": node_type,
            }),
        )
        .await;

        // BEFORE_EXECUTE is a gate point: a vetoed fire denies the node, so
        // the handler never runs (pre-execution checks such as environment
        // or input validation belong to hook handlers here, not to trigger
        // templates, which always run asynchronously after the fire). The
        // veto follows the node-failure path (ON_ERROR hook, NodeFailed
        // event, no AFTER_EXECUTE) and is never retried.
        let before = Self::execute_hooks(
            hooks,
            hook_handler_registry,
            event_bus,
            entity,
            "BEFORE_EXECUTE",
            &NodeHookPayload {
                node_id: &node_id,
                node_name: &node_name,
                node_type: &node_type,
                duration_ms: None,
                error: None,
            },
        )
        .await;
        if let Some(reason) = before.vetoed_reason() {
            return Self::fail_node(
                hooks,
                hook_handler_registry,
                event_bus,
                entity,
                &node_id,
                &node_name,
                &node_type,
                node_start,
                format!("hook veto at BEFORE_EXECUTE: {reason}"),
            )
            .await;
        }

        let check = check_execution_interruption(entity.interruption(), None);
        if !matches!(
            check,
            wf_execution_shared::types::interruption::ExecutionInterruptionCheckResult::Continue
        ) {
            return Err(WorkflowError::CoordinatorError(format!(
                "Execution interrupted before node {}: {:?}",
                node_id, check
            )));
        }

        let result = handler.execute(ctx).await;

        match &result {
            Ok(output) => {
                entity
                    .state
                    .write()
                    .await
                    .mark_node_completed(node_id.clone());

                Self::execute_hooks(
                    hooks,
                    hook_handler_registry,
                    event_bus,
                    entity,
                    "AFTER_EXECUTE",
                    &NodeHookPayload {
                        node_id: &node_id,
                        node_name: &node_name,
                        node_type: &node_type,
                        duration_ms: Some(wf_common::now() - node_start),
                        error: None,
                    },
                )
                .await;

                Self::emit_event(
                    event_bus,
                    EventType::NodeCompleted,
                    entity,
                    &node_id,
                    &serde_json::json!({
                        "has_next_nodes": !output.next_node_ids.is_empty(),
                        "node_name": node_name,
                        "node_type": node_type,
                        "duration_ms": wf_common::now() - node_start,
                    }),
                )
                .await;

                Ok(NodeExecutionResult {
                    output: output.output.clone(),
                    next_node_ids: output.next_node_ids.clone(),
                    metadata: output.metadata.clone(),
                })
            }
            Err(e) => {
                Self::fail_node(
                    hooks,
                    hook_handler_registry,
                    event_bus,
                    entity,
                    &node_id,
                    &node_name,
                    &node_type,
                    node_start,
                    e.to_string(),
                )
                .await
            }
        }
    }

    /// Shared node-failure path for handler errors and BEFORE_EXECUTE
    /// vetoes: fire ON_ERROR, emit NodeFailed, never AFTER_EXECUTE.
    async fn fail_node(
        hooks: &[HookDefinition],
        hook_handler_registry: Option<&HookHandlerRegistry>,
        event_bus: Option<&EventBus>,
        entity: &WorkflowExecutionEntity,
        node_id: &str,
        node_name: &str,
        node_type: &str,
        node_start: i64,
        reason: String,
    ) -> WorkflowResult<NodeExecutionResult> {
        Self::execute_hooks(
            hooks,
            hook_handler_registry,
            event_bus,
            entity,
            "ON_ERROR",
            &NodeHookPayload {
                node_id,
                node_name,
                node_type,
                duration_ms: Some(wf_common::now() - node_start),
                error: Some(&reason),
            },
        )
        .await;

        Self::emit_event(
            event_bus,
            EventType::NodeFailed,
            entity,
            node_id,
            &serde_json::json!({
                "error": reason,
                "node_name": node_name,
                "node_type": node_type,
                "duration_ms": wf_common::now() - node_start,
            }),
        )
        .await;

        Err(WorkflowError::NodeExecutionFailed {
            node_id: node_id.to_string(),
            reason,
        })
    }

    pub async fn execute_with_retry(
        &self,
        handler: &dyn NodeHandler,
        ctx: &mut NodeExecutionContext,
        retry_budget: Option<&RetryBudget>,
    ) -> WorkflowResult<NodeExecutionResult> {
        loop {
            match handler.execute(ctx).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let Some(budget) = retry_budget else {
                        return Err(e.into());
                    };
                    let delay = std::time::Duration::from_millis(
                        1000 * 2_u64.pow(budget.get_state().retries_consumed.min(10)),
                    );
                    let check = budget.consume_retry(delay.as_millis() as u64, None, 0);
                    if !check.allowed {
                        return Err(e.into());
                    }
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    async fn execute_hooks(
        hooks: &[HookDefinition],
        hook_handler_registry: Option<&HookHandlerRegistry>,
        event_bus: Option<&EventBus>,
        entity: &WorkflowExecutionEntity,
        hook_type: &str,
        payload: &NodeHookPayload<'_>,
    ) -> FireSummary {
        let mut data = std::collections::HashMap::new();
        data.insert(
            "entity_id".to_string(),
            serde_json::Value::String(entity.id().to_string()),
        );
        data.insert(
            "workflow_id".to_string(),
            serde_json::Value::String(entity.workflow_id().to_string()),
        );
        data.insert(
            "hook_type".to_string(),
            serde_json::Value::String(hook_type.to_string()),
        );
        let status = entity.state.read().await.status();
        data.insert(
            "status".to_string(),
            serde_json::Value::String(format!("{:?}", status)),
        );
        data.insert(
            "node_id".to_string(),
            serde_json::Value::String(payload.node_id.to_string()),
        );
        data.insert(
            "node_name".to_string(),
            serde_json::Value::String(payload.node_name.to_string()),
        );
        data.insert(
            "node_type".to_string(),
            serde_json::Value::String(payload.node_type.to_string()),
        );
        if let Some(duration) = payload.duration_ms {
            data.insert(
                "duration_ms".to_string(),
                serde_json::Value::Number(duration.into()),
            );
        }
        if let Some(err) = payload.error {
            data.insert(
                "error".to_string(),
                serde_json::Value::String(err.to_string()),
            );
        }

        crate::hook::WorkflowHookEmitter::fire_point(
            hooks,
            hook_type,
            &HookContext {
                execution_id: entity.id().clone(),
                hook_type: hook_type.to_string(),
                data,
            },
            hook_handler_registry,
            event_bus,
        )
        .await
    }

    async fn emit_event(
        event_bus: Option<&EventBus>,
        event_type: EventType,
        entity: &WorkflowExecutionEntity,
        node_id: &str,
        data: &serde_json::Value,
    ) {
        let Some(bus) = event_bus else {
            tracing::debug!(
                execution_id = %entity.id(),
                node_id,
                ?event_type,
                "no event bus attached, skipping event emission"
            );
            return;
        };
        let mut metadata: HashMap<String, serde_json::Value> = data
            .as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        metadata.insert(
            "node_id".to_string(),
            serde_json::Value::String(node_id.to_string()),
        );

        let event_type_label = format!("{:?}", event_type);
        let event = BaseEvent {
            id: wf_types::Id::new(),
            r#type: event_type,
            timestamp: wf_common::now(),
            workflow_id: Some(entity.workflow_id().clone()),
            execution_id: Some(entity.id().clone()),
            agent_loop_id: None,

            event_name: None,
            metadata: Some(metadata),
        };
        if let Err(err) =
            bus.publish_logged(event, &format!("workflow={} node={}", entity.id(), node_id))
        {
            tracing::error!(
                execution_id = %entity.id(),
                node_id,
                event_type = %event_type_label,
                error = ?err,
                "node lifecycle event publish failed"
            );
        }
    }
}
