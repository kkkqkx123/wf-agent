//! `ExecuteTriggeredSubworkflow` action execution for message nodes.
//!
//! Runs a registered graph either inline (racing the parent's cancellation)
//! or fire-and-forget (abandoned, with logging, when the parent cancels).

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use wf_tools::registry::ToolRegistry;
use wf_types::events::EventType;
use wf_types::node::StaticNodeType;
use wf_types::trigger::TriggerAction;

use crate::coordinator::WorkflowCoordinator;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::trigger::context::TriggerContext;
use crate::handler::trigger::events::emit;
use crate::handler::{variable_mapping, HandlerRegistry};
use crate::registry::lookup_graph;
use crate::trigger::internal;
use crate::WorkflowExecutionEntity;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::error::ExecutionSharedError;
use wf_types::workflow_execution::WorkflowExecutionOptions;

/// Everything needed to run a triggered sub-workflow.
pub(crate) struct TriggeredSubworkflowRun {
    pub triggered_workflow_id: String,
    pub graph: wf_types::workflow_execution::WorkflowGraphStructure,
    pub handlers: Arc<HashMap<StaticNodeType, Box<dyn crate::handler::NodeHandler>>>,
    pub tool_registry: Arc<ToolRegistry>,
    pub input_mapping: HashMap<String, Value>,
    pub output_mapping: HashMap<String, Value>,
    pub timeout: u64,
}

pub(crate) async fn handle_execute_subworkflow(
    action: &TriggerAction,
    ctx: &TriggerContext,
) -> WorkflowResult<Value> {
    let (triggered_workflow_id, wait_for_completion, input_mapping, output_mapping, timeout) =
        match action {
            TriggerAction::ExecuteTriggeredSubworkflow {
                triggered_workflow_id,
                wait_for_completion,
                input_mapping,
                output_mapping,
                timeout,
            } => (
                triggered_workflow_id.clone(),
                wait_for_completion.unwrap_or(true),
                input_mapping.clone().unwrap_or_default(),
                output_mapping.clone().unwrap_or_default(),
                timeout.unwrap_or(0),
            ),
            _ => return Err(WorkflowError::Internal("Invalid action type".to_string())),
        };

    let graph = lookup_graph(&triggered_workflow_id).ok_or_else(|| {
        WorkflowError::TriggerError(format!(
            "Triggered workflow '{}' not found in graph registry",
            triggered_workflow_id
        ))
    })?;

    emit(
        ctx,
        EventType::TriggeredSubgraphStarted,
        &format!("triggered_subworkflow:{}", triggered_workflow_id),
    )
    .await;

    let handlers = ctx.handlers.clone().unwrap_or_else(|| {
        let mut registry = HandlerRegistry::new();
        registry.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
        registry.into_arc()
    });
    let tool_registry = ctx
        .tool_registry
        .clone()
        .unwrap_or_else(|| Arc::new(ToolRegistry::new()));

    if !wait_for_completion {
        let tctx = ctx.clone();
        let cancellation = tctx.cancellation.clone();
        let execution_id = wf_common::generate_id();
        let run = TriggeredSubworkflowRun {
            triggered_workflow_id: triggered_workflow_id.clone(),
            graph,
            handlers,
            tool_registry,
            input_mapping: input_mapping.clone(),
            output_mapping: output_mapping.clone(),
            timeout,
        };
        let exec_id = execution_id.clone();
        tokio::spawn(async move {
            let subworkflow = run_triggered_subworkflow(&tctx, run);
            tokio::pin!(subworkflow);
            let outcome = match cancellation {
                Some(token) => {
                    // A cancelled parent aborts the background
                    // sub-workflow; the abandonment is recorded instead
                    // of silently dropping the child.
                    tokio::select! {
                        res = &mut subworkflow => res,
                        _ = token.cancelled() => {
                            tracing::warn!(
                                execution_id = %execution_id,
                                "fire-and-forget triggered sub-workflow abandoned: parent execution cancelled"
                            );
                            return;
                        }
                    }
                }
                None => subworkflow.await,
            };
            if let Err(e) = outcome {
                tracing::warn!(
                    execution_id = %execution_id,
                    error = %e,
                    "fire-and-forget triggered sub-workflow ended with failure"
                );
            }
        });
        return Ok(serde_json::json!({
            "submitted": true,
            "workflow_id": triggered_workflow_id,
            "execution_id": exec_id,
        }));
    }

    let execution_id = wf_common::generate_id();
    let run = TriggeredSubworkflowRun {
        triggered_workflow_id: triggered_workflow_id.clone(),
        graph,
        handlers,
        tool_registry,
        input_mapping,
        output_mapping,
        timeout,
    };
    // Synchronous wait races the parent's cancellation so an external
    // stop abandons the child instead of blocking the node until the
    // sub-workflow finishes. The interruption keeps its typed category
    // so the node failure routes as cancelled, not business.
    let outcome = {
        let subworkflow = run_triggered_subworkflow(ctx, run);
        tokio::pin!(subworkflow);
        match &ctx.cancellation {
            Some(token) => {
                tokio::select! {
                    res = &mut subworkflow => res,
                    _ = token.cancelled() => Err(WorkflowError::SharedError(
                        ExecutionSharedError::InterruptionError(
                            "triggered sub-workflow abandoned: parent execution cancelled"
                                .to_string(),
                        ),
                    )),
                }
            }
            None => subworkflow.await,
        }
    };
    match outcome {
        Ok(result) => Ok(serde_json::json!({
            "submitted": true,
            "workflow_id": triggered_workflow_id,
            "execution_id": execution_id,
            "result": result,
        })),
        Err(err) => Err(err),
    }
}

async fn run_triggered_subworkflow(
    ctx: &TriggerContext,
    run: TriggeredSubworkflowRun,
) -> WorkflowResult<Value> {
    let triggered_workflow_id = run.triggered_workflow_id;
    let variables = Arc::new(DashMap::new());
    variable_mapping::inherit_all_variables(&ctx.variables, &variables);
    for (key, value) in &run.input_mapping {
        variables.insert(key.clone(), value.clone());
    }

    let options = WorkflowExecutionOptions {
        input: Some(Value::Object(
            run.input_mapping.clone().into_iter().collect(),
        )),
        max_steps: None,
        timeout: None,
        // An explicit action timeout wins; otherwise inherit the parent's
        // resolved budget so the child shares the entry/limits source like
        // the listener path instead of falling through to unlimited.
        max_execution_time: (run.timeout > 0)
            .then_some(run.timeout)
            .or(ctx.parent_max_execution_time_ms),
        enable_checkpoints: Some(false),
        // Inherit the parent's node budget (entry/limits derived) rather
        // than resetting to the engine per-node fallback.
        node_timeout: ctx.parent_node_timeout_ms,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    };

    let execution_id = wf_common::generate_id();
    let sub_workflow_id = wf_common::generate_id();
    let entity = WorkflowExecutionEntity::new(execution_id.clone(), sub_workflow_id.clone());
    let mut exec_ctx = ExecutorContext::new(
        execution_id,
        sub_workflow_id,
        ctx.event_bus.clone(),
        run.tool_registry,
        options,
    )
    .with_parent_execution(ctx.execution_id.clone());
    if let Some(metrics) = &ctx.metrics {
        exec_ctx = exec_ctx.with_metrics(metrics.clone());
    }
    exec_ctx.variables = variables.clone();

    let mut coordinator = match WorkflowCoordinator::new(exec_ctx, run.graph, run.handlers) {
        Ok(coordinator) => coordinator.with_entity(entity),
        Err(err) => {
            emit(
                ctx,
                EventType::TriggeredSubgraphFailed,
                &format!("triggered_subworkflow_failed:{}", triggered_workflow_id),
            )
            .await;
            return Err(err);
        }
    };

    match coordinator.execute().await {
        Ok(output) => {
            apply_subworkflow_output_mapping(
                &run.output_mapping,
                &variables,
                &output,
                &ctx.variables,
            );
            ctx.variables
                .insert(internal::SUBWORKFLOW_RESULT.to_string(), output.clone());
            if let Some(bus) = &ctx.signal_bus {
                internal::publish_subworkflow_result(
                    bus,
                    ctx.execution_id.clone(),
                    ctx.execution_id.clone(),
                    output.clone(),
                );
            }
            emit(
                ctx,
                EventType::TriggeredSubgraphCompleted,
                &format!("triggered_subworkflow_completed:{}", triggered_workflow_id),
            )
            .await;
            Ok(output)
        }
        Err(err) => {
            emit(
                ctx,
                EventType::TriggeredSubgraphFailed,
                &format!("triggered_subworkflow_failed:{}", triggered_workflow_id),
            )
            .await;
            Err(err)
        }
    }
}

fn apply_subworkflow_output_mapping(
    output_mapping: &HashMap<String, Value>,
    sub_variables: &DashMap<String, Value>,
    result: &Value,
    parent_variables: &DashMap<String, Value>,
) {
    for (target, source) in output_mapping {
        let value = match source {
            Value::String(name) => sub_variables
                .get(name)
                .map(|entry| entry.value().clone())
                .unwrap_or_else(|| result.clone()),
            _ => source.clone(),
        };
        parent_variables.insert(target.clone(), value);
    }
}
