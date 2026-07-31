use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use wf_core::EventBus;
use wf_types::events::{BaseEvent, EventType};
use wf_types::trigger::{TriggerAction, TriggerExecutionResult};
use wf_types::Id;

use crate::error::{WorkflowError, WorkflowResult};

// EventType does not have NodeSkipped or NotificationSent variants,
// so we use NodeCustomEvent / VariableChanged as closest alternatives.

pub struct TriggerContext {
    pub execution_id: Id,
    pub workflow_id: Id,
    pub variables: Arc<DashMap<String, Value>>,
    pub event_bus: Option<Arc<EventBus>>,
}

impl TriggerContext {
    pub fn new(execution_id: Id, workflow_id: Id) -> Self {
        Self {
            execution_id,
            workflow_id,
            variables: Arc::new(DashMap::new()),
            event_bus: None,
        }
    }

    pub fn with_variables(mut self, variables: Arc<DashMap<String, Value>>) -> Self {
        self.variables = variables;
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }
}

pub struct TriggerCoordinator;

impl TriggerCoordinator {
    pub async fn execute(
        action: &TriggerAction,
        trigger_id: &str,
        ctx: &TriggerContext,
    ) -> TriggerExecutionResult {
        let start = wf_common::now();
        let result = match action {
            TriggerAction::StopWorkflowExecution { .. } => Self::handle_stop_workflow(ctx).await,
            TriggerAction::PauseWorkflowExecution { .. } => Self::handle_pause_workflow(ctx).await,
            TriggerAction::ResumeWorkflowExecution { .. } => {
                Self::handle_resume_workflow(ctx).await
            }
            TriggerAction::SkipNode { node_id } => {
                Self::handle_skip_node(node_id.as_deref().unwrap_or(""), ctx).await
            }
            TriggerAction::SetVariable {
                variable_name,
                value,
            } => Self::handle_set_variable(variable_name, value.clone(), ctx).await,
            TriggerAction::SendNotification { message } => {
                Self::handle_send_notification(message, ctx).await
            }
            TriggerAction::ExecuteTriggeredSubworkflow { .. } => {
                Self::handle_execute_subworkflow(action, ctx).await
            }
            TriggerAction::ExecuteScript { .. } => Self::handle_execute_script(action, ctx).await,
        };

        let (result_val, error_val) = match result {
            Ok(val) => (Some(val), None),
            Err(e) => (None, Some(e.to_string())),
        };

        TriggerExecutionResult {
            trigger_id: Id::from(trigger_id),
            success: error_val.is_none(),
            execution_id: Some(ctx.execution_id.clone()),
            result: result_val,
            error: error_val,
            execution_time: wf_common::now() - start,
        }
    }

    async fn handle_stop_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
        ctx.variables
            .insert("__trigger_stop".to_string(), Value::Bool(true));
        Self::emit(
            ctx,
            EventType::ExecutionStopped,
            "workflow_stopped_by_trigger",
        )
        .await;
        Ok(Value::String("workflow_stopped".to_string()))
    }

    async fn handle_pause_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
        ctx.variables
            .insert("__trigger_pause".to_string(), Value::Bool(true));
        Self::emit(
            ctx,
            EventType::WorkflowExecutionPaused,
            "workflow_paused_by_trigger",
        )
        .await;
        Ok(Value::String("workflow_paused".to_string()))
    }

    async fn handle_resume_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
        ctx.variables.remove("__trigger_pause");
        Self::emit(
            ctx,
            EventType::WorkflowExecutionResumed,
            "workflow_resumed_by_trigger",
        )
        .await;
        Ok(Value::String("workflow_resumed".to_string()))
    }

    async fn handle_skip_node(node_id: &str, ctx: &TriggerContext) -> WorkflowResult<Value> {
        ctx.variables
            .insert(format!("__skipped_{}", node_id), Value::Bool(true));
        Self::emit(
            ctx,
            EventType::NodeCustomEvent,
            &format!("node_skipped:{}", node_id),
        )
        .await;
        Ok(serde_json::json!({"skipped_node": node_id}))
    }

    async fn handle_set_variable(
        var_name: &str,
        var_value: Value,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        ctx.variables
            .insert(var_name.to_string(), var_value.clone());
        Self::emit(
            ctx,
            EventType::VariableChanged,
            &format!("variable_set:{}", var_name),
        )
        .await;
        Ok(serde_json::json!({"variable": var_name, "value": var_value}))
    }

    async fn handle_send_notification(
        message: &str,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        Self::emit(
            ctx,
            EventType::NodeCustomEvent,
            &format!("notification:{}", message),
        )
        .await;
        Ok(serde_json::json!({"sent": true, "message": message}))
    }

    async fn handle_execute_subworkflow(
        action: &TriggerAction,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        let triggered_workflow_id = match action {
            TriggerAction::ExecuteTriggeredSubworkflow {
                triggered_workflow_id,
                ..
            } => triggered_workflow_id.clone(),
            _ => return Err(WorkflowError::Internal("Invalid action type".to_string())),
        };

        ctx.variables.insert(
            "__trigger_subworkflow".to_string(),
            serde_json::json!({"workflow_id": triggered_workflow_id}),
        );

        Self::emit(
            ctx,
            EventType::SubgraphStarted,
            &format!("triggered_subworkflow:{}", triggered_workflow_id),
        )
        .await;
        Ok(serde_json::json!({"submitted": true, "workflow_id": triggered_workflow_id}))
    }

    async fn handle_execute_script(
        action: &TriggerAction,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        let script_name = match action {
            TriggerAction::ExecuteScript { script_name, .. } => script_name.clone(),
            _ => return Err(WorkflowError::Internal("Invalid action type".to_string())),
        };

        ctx.variables.insert(
            "__trigger_script".to_string(),
            serde_json::json!({"script_name": script_name}),
        );

        Self::emit(
            ctx,
            EventType::ScriptStarted,
            &format!("trigger_script:{}", script_name),
        )
        .await;
        Ok(serde_json::json!({"submitted": true, "script_name": script_name}))
    }

    async fn emit(ctx: &TriggerContext, event_type: EventType, message: &str) {
        if let Some(ref bus) = ctx.event_bus {
            let event = BaseEvent {
                id: Id::new(),
                r#type: event_type,
                timestamp: wf_common::now(),
                workflow_id: Some(ctx.workflow_id.clone()),
                execution_id: Some(ctx.execution_id.clone()),
                agent_loop_id: None,
                metadata: Some(std::collections::HashMap::from([(
                    "trigger_message".to_string(),
                    Value::String(message.to_string()),
                )])),
            };
            let _ = bus.publish(event);
        }
    }
}
