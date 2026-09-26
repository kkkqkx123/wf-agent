//! `TriggerCoordinator`: synchronous dispatch of one `TriggerAction` and
//! result shaping into `TriggerExecutionResult`.

use serde_json::Value;
use wf_types::trigger::{TriggerAction, TriggerExecutionResult};
use wf_types::Id;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::trigger::context::TriggerContext;
use crate::handler::trigger::events::{emit, emit_with_metadata};
use crate::handler::trigger::script_exec::handle_execute_script;
use crate::handler::trigger::subworkflow::handle_execute_subworkflow;
use crate::trigger::internal;
use wf_types::events::EventType;

/// Synchronous in-graph executor for a `TriggerAction` set.
///
/// Runs inside the owning node and returns inline; unrelated to the
/// event-driven `crate::trigger::TriggerEventListener` async side effects.
pub struct TriggerCoordinator;

impl TriggerCoordinator {
    pub async fn execute(
        action: &TriggerAction,
        trigger_id: &str,
        ctx: &TriggerContext,
    ) -> TriggerExecutionResult {
        let start = wf_common::now();
        let result = match action {
            TriggerAction::StopWorkflowExecution { .. } => handle_stop_workflow(ctx).await,
            TriggerAction::PauseWorkflowExecution { .. } => handle_pause_workflow(ctx).await,
            TriggerAction::ResumeWorkflowExecution { .. } => handle_resume_workflow(ctx).await,
            TriggerAction::SkipNode { node_id } => {
                handle_skip_node(node_id.as_deref().unwrap_or(""), ctx).await
            }
            TriggerAction::SetVariable {
                variable_name,
                value,
            } => handle_set_variable(variable_name, value.clone(), ctx).await,
            TriggerAction::SendNotification { message } => {
                handle_send_notification(message, ctx).await
            }
            TriggerAction::ExecuteTriggeredSubworkflow { .. } => {
                handle_execute_subworkflow(action, ctx).await
            }
            TriggerAction::ExecuteScript { .. } => handle_execute_script(action, ctx).await,
            TriggerAction::SetMessageContext {
                context_id,
                messages,
            } => handle_set_message_context(context_id, messages.clone(), ctx).await,
            TriggerAction::AppendMessageContext {
                context_id,
                messages,
            } => handle_append_message_context(context_id, messages.clone(), ctx).await,
            TriggerAction::FilterMessageContext {
                context_id,
                role,
                exclude,
                custom_filter,
            } => {
                handle_filter_message_context(
                    context_id,
                    role.clone(),
                    exclude.unwrap_or(false),
                    custom_filter.clone(),
                    ctx,
                )
                .await
            }
            // Nested agent executions are an event-driven trigger feature
            // (wf-runtime `AgentTriggerRunner`): message nodes have no parent
            // `AgentLoopEntity` / conversation session / `AgentLoopRegistry`,
            // so no anchored input context or write-back target can be
            // constructed for the child. Kept rejected with the unified
            // matrix error (never silently degraded); the support matrix on
            // `TriggerAction` documents this as unsupported in message nodes.
            TriggerAction::ExecuteTriggeredAgentExecution { .. } => {
                let message = action
                    .rejection_message(wf_types::trigger::TriggerExecutionContext::MessageNode)
                    .unwrap_or_else(|| {
                        format!(
                            "{} is not executable in message nodes",
                            action.action_name()
                        )
                    });
                Err(WorkflowError::ConfigError {
                    node_id: ctx.node_id.clone(),
                    field: "action".to_string(),
                    detail: message,
                })
            }
            // Cold-start actions need the triggering event (or rather its
            // absence): message nodes always run inside an execution and
            // cannot start a fresh run. Same unified-matrix rejection.
            TriggerAction::ExecuteWorkflow { .. } | TriggerAction::ExecuteAgent { .. } => {
                let message = action
                    .rejection_message(wf_types::trigger::TriggerExecutionContext::MessageNode)
                    .unwrap_or_else(|| {
                        format!(
                            "{} is not executable in message nodes",
                            action.action_name()
                        )
                    });
                Err(WorkflowError::ConfigError {
                    node_id: ctx.node_id.clone(),
                    field: "action".to_string(),
                    detail: message,
                })
            }
        };

        let (result_val, error_val, error_category) = match result {
            Ok(val) => (Some(val), None, None),
            // Keep the routing category next to the message so callers
            // (e.g. the message-node handler) can rebuild a typed failure
            // instead of collapsing every error into a trigger string.
            Err(e) => (
                None,
                Some(e.to_string()),
                Some(crate::error_branch::classify_error(&e)),
            ),
        };

        TriggerExecutionResult {
            trigger_id: Id::from(trigger_id),
            success: error_val.is_none(),
            execution_id: Some(ctx.execution_id.clone()),
            result: result_val,
            error: error_val,
            error_category,
            execution_time: wf_common::now() - start,
        }
    }
}

async fn handle_stop_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
    // Publish a typed stop signal; the coordinator loop consumes it.
    if let Some(bus) = &ctx.signal_bus {
        internal::publish_stop_signal(
            bus,
            ctx.execution_id.clone(),
            ctx.execution_id.clone(),
            None,
        );
    }
    emit(
        ctx,
        EventType::ExecutionStopped,
        "workflow_stopped_by_trigger",
    )
    .await;
    Ok(Value::String("workflow_stopped".to_string()))
}

async fn handle_pause_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
    // Publish a typed pause signal; the coordinator loop consumes it.
    if let Some(bus) = &ctx.signal_bus {
        internal::publish_pause_signal(
            bus,
            ctx.execution_id.clone(),
            ctx.execution_id.clone(),
            None,
        );
    }
    emit(
        ctx,
        EventType::WorkflowExecutionPaused,
        "workflow_paused_by_trigger",
    )
    .await;
    Ok(Value::String("workflow_paused".to_string()))
}

async fn handle_resume_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
    // Publish a typed resume signal; the coordinator loop consumes it.
    if let Some(bus) = &ctx.signal_bus {
        internal::publish_resume_signal(bus, ctx.execution_id.clone(), ctx.execution_id.clone());
    }
    emit(
        ctx,
        EventType::WorkflowExecutionResumed,
        "workflow_resumed_by_trigger",
    )
    .await;
    Ok(Value::String("workflow_resumed".to_string()))
}

async fn handle_skip_node(node_id: &str, ctx: &TriggerContext) -> WorkflowResult<Value> {
    // Publish a typed skip signal; the coordinator loop records the
    // node for skipping at dispatch time.
    if let Some(bus) = &ctx.signal_bus {
        internal::publish_skip_signal(
            bus,
            ctx.execution_id.clone(),
            ctx.execution_id.clone(),
            node_id.to_string(),
        );
    }
    emit_with_metadata(
        ctx,
        EventType::NodeSkipped,
        &format!("node_skipped:{}", node_id),
        &[("node_id", Value::String(node_id.to_string()))],
    )
    .await;
    Ok(serde_json::json!({"skipped_node": node_id}))
}

async fn handle_set_variable(
    var_name: &str,
    var_value: Value,
    ctx: &TriggerContext,
) -> WorkflowResult<Value> {
    // Engine-internal state (loop stacks, message contexts, fork
    // handovers, interaction markers) lives under the reserved `__`
    // prefix; refusing it here keeps SetVariable from corrupting that
    // state (e.g. writing `__msg_ctx__*` directly bypasses the token
    // ledger). Message-context updates must use the dedicated
    // `SetMessageContext` / `AppendMessageContext` actions instead.
    if var_name.starts_with("__") {
        return Err(WorkflowError::TriggerError(format!(
            "SetVariable refuses to write internal variable '{}' (reserved '__' prefix); use SetMessageContext/AppendMessageContext for message contexts",
            var_name
        )));
    }
    ctx.variables
        .insert(var_name.to_string(), var_value.clone());
    emit(
        ctx,
        EventType::VariableChanged,
        &format!("variable_set:{}", var_name),
    )
    .await;
    Ok(serde_json::json!({"variable": var_name, "value": var_value}))
}

/// Switch the active view of a named message context (ledger-safe:
/// goes through `message_context::register_context`, which archives the
/// superseded active messages append-only and marks the token ledger
/// dirty so the next read recomputes the estimate).
async fn handle_set_message_context(
    context_id: &str,
    messages: Vec<wf_types::message::Message>,
    ctx: &TriggerContext,
) -> WorkflowResult<Value> {
    crate::message_context::register_context(&ctx.variables, context_id, messages.clone());
    emit_with_metadata(
        ctx,
        EventType::MessageContextUpdated,
        &format!("message_context_set:{}", context_id),
        &[
            ("context_id", Value::String(context_id.to_string())),
            (
                "message_count",
                Value::Number(serde_json::Number::from(messages.len() as u64)),
            ),
        ],
    )
    .await;
    Ok(serde_json::json!({
        "context_id": context_id,
        "message_count": messages.len(),
    }))
}

/// Append messages to a named message context, creating it when absent
/// (ledger-safe incremental append).
async fn handle_append_message_context(
    context_id: &str,
    messages: Vec<wf_types::message::Message>,
    ctx: &TriggerContext,
) -> WorkflowResult<Value> {
    if messages.is_empty() {
        return Ok(serde_json::json!({
            "context_id": context_id,
            "message_count": 0,
        }));
    }
    crate::message_context::append_context(&ctx.variables, context_id, messages.clone());
    emit_with_metadata(
        ctx,
        EventType::MessageContextUpdated,
        &format!("message_context_appended:{}", context_id),
        &[
            ("context_id", Value::String(context_id.to_string())),
            (
                "message_count",
                Value::Number(serde_json::Number::from(messages.len() as u64)),
            ),
        ],
    )
    .await;
    Ok(serde_json::json!({
        "context_id": context_id,
        "appended": messages.len(),
    }))
}

/// Filter a named message context by role and/or text content through
/// the shared stateless operation, with the same ledger-safe write-back.
/// Filtered-out messages leave the active view but stay in the
/// append-only archive, so the operation is undoable via history
/// restore and never loses checkpointed history.
async fn handle_filter_message_context(
    context_id: &str,
    role: Option<wf_types::message::MessageRole>,
    exclude: bool,
    custom_filter: Option<String>,
    ctx: &TriggerContext,
) -> WorkflowResult<Value> {
    let messages = crate::message_context::get_context(&ctx.variables, context_id);
    let operation = wf_types::message::MessageOperationConfig::Filter(
        wf_types::message::FilterMessageOperation {
            role,
            exclude: Some(exclude),
            custom_filter,
        },
    );
    let (result, stats) = wf_execution_shared::message_ops::apply(&messages, &operation);
    crate::message_context::register_context(&ctx.variables, context_id, result);
    emit_with_metadata(
        ctx,
        EventType::MessageContextUpdated,
        &format!("message_context_filtered:{}", context_id),
        &[
            ("context_id", Value::String(context_id.to_string())),
            (
                "message_count",
                Value::Number(serde_json::Number::from(stats.total_after as u64)),
            ),
        ],
    )
    .await;
    Ok(serde_json::json!({
        "context_id": context_id,
        "removed": stats.removed,
        "message_count": stats.total_after,
    }))
}

async fn handle_send_notification(message: &str, ctx: &TriggerContext) -> WorkflowResult<Value> {
    emit_with_metadata(
        ctx,
        EventType::NotificationSent,
        &format!("notification:{}", message),
        &[("message", Value::String(message.to_string()))],
    )
    .await;
    Ok(serde_json::json!({"sent": true, "message": message}))
}
