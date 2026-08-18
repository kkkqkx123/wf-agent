use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::events::{BaseEvent, EventType};
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::interaction::register_interaction;
use crate::message_context;

fn emit_interaction_event(
    event_bus: Option<&EventBus>,
    event_type: EventType,
    ctx: &NodeExecutionContext,
    metadata: HashMap<String, Value>,
) {
    let Some(bus) = event_bus else {
        tracing::debug!(
            execution_id = %ctx.execution_id,
            node_id = %ctx.node_id,
            ?event_type,
            "no event bus, skipping interaction event"
        );
        return;
    };
    bus.publish_logged(
        BaseEvent {
            id: wf_types::Id::new(),
            r#type: event_type,
            timestamp: wf_common::now(),
            workflow_id: Some(ctx.execution_id.clone()),
            execution_id: Some(ctx.execution_id.clone()),
            agent_loop_id: None,

            event_name: None,
            metadata: Some(metadata),
        },
        &format!("workflow={} interaction={}", ctx.execution_id, ctx.node_id),
    )
    .ok();
}

/// Replace `{{input}}` placeholders with the user-provided input value.
fn replace_input_placeholder(template: &str, input: &Value) -> String {
    let input_str = match input {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    template.replace("{{input}}", &input_str)
}

/// Evaluate a variable expression:
/// `{{input}}` resolves to the user input, otherwise the expression is
/// returned as a constant string.
fn evaluate_expression(expression: &str, input: &Value) -> Value {
    if expression == "{{input}}" {
        input.clone()
    } else if expression.contains("{{input}}") {
        Value::String(replace_input_placeholder(expression, input))
    } else {
        Value::String(expression.to_string())
    }
}

pub struct UserInteractionHandler;

#[async_trait]
impl NodeHandler for UserInteractionHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::UserInteraction
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl UserInteractionHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.clone().unwrap_or_default();

        let interaction_type = config
            .get("operation_type")
            .and_then(|v| v.as_str())
            .unwrap_or("update_variables");

        let prompt = config
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(|s| {
                let resolved = crate::variable::VariableResolver::resolve_str(s, &ctx.variables);
                resolved
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or(s.to_string())
            })
            .unwrap_or_default();

        let timeout_ms = config
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000);

        let user_response_var = "__interaction_response__";

        // 1. Pre-set response (synchronous path, kept for compatibility).
        let existing_response = ctx
            .get_variable(user_response_var)
            .or_else(|| ctx.get_variable("__interaction_response__"));

        let (response, interaction_id, responded) = match existing_response {
            Some(value) => (Some(value), None, true),
            None => {
                // 2. Async path: register the interaction, publish the
                // request and wait for an external response.
                let (interaction_id, rx) = register_interaction();

                emit_interaction_event(
                    ctx.event_bus.as_deref(),
                    EventType::FollowupQuestionRequested,
                    ctx,
                    HashMap::from([
                        (
                            "interaction_id".to_string(),
                            Value::String(interaction_id.clone()),
                        ),
                        ("prompt".to_string(), Value::String(prompt.clone())),
                        (
                            "timeout".to_string(),
                            Value::Number(serde_json::Number::from(timeout_ms)),
                        ),
                        ("node_id".to_string(), Value::String(ctx.node_id.clone())),
                        (
                            "operation".to_string(),
                            Value::String(operation_name(&config)),
                        ),
                    ]),
                );

                ctx.set_internal_variable("__interaction_waiting__", Value::Bool(true));
                emit_interaction_event(
                    ctx.event_bus.as_deref(),
                    EventType::WorkflowExecutionPaused,
                    ctx,
                    HashMap::from([
                        (
                            "reason".to_string(),
                            Value::String("user_interaction".to_string()),
                        ),
                        (
                            "interaction_id".to_string(),
                            Value::String(interaction_id.clone()),
                        ),
                    ]),
                );

                let wait_result = tokio::time::timeout(Duration::from_millis(timeout_ms), rx).await;

                ctx.set_internal_variable("__interaction_waiting__", Value::Bool(false));
                emit_interaction_event(
                    ctx.event_bus.as_deref(),
                    EventType::WorkflowExecutionResumed,
                    ctx,
                    HashMap::from([
                        (
                            "reason".to_string(),
                            Value::String("user_interaction_completed".to_string()),
                        ),
                        (
                            "interaction_id".to_string(),
                            Value::String(interaction_id.clone()),
                        ),
                    ]),
                );

                match wait_result {
                    Ok(Ok(value)) => {
                        emit_interaction_event(
                            ctx.event_bus.as_deref(),
                            EventType::FollowupQuestionResponded,
                            ctx,
                            HashMap::from([(
                                "interaction_id".to_string(),
                                Value::String(interaction_id.clone()),
                            )]),
                        );
                        ctx.set_internal_variable(user_response_var.to_string(), value.clone());
                        (Some(value), Some(interaction_id), true)
                    }
                    Ok(Err(_)) | Err(_) => {
                        let reason = if wait_result.is_err() {
                            "timeout"
                        } else {
                            "cancelled"
                        };
                        emit_interaction_event(
                            ctx.event_bus.as_deref(),
                            EventType::FollowupQuestionFailed,
                            ctx,
                            HashMap::from([
                                (
                                    "interaction_id".to_string(),
                                    Value::String(interaction_id.clone()),
                                ),
                                ("reason".to_string(), Value::String(reason.to_string())),
                            ]),
                        );
                        return Err(WorkflowError::OperationError(format!(
                            "User interaction {} after {}ms",
                            reason, timeout_ms
                        )));
                    }
                }
            }
        };

        // 3. Apply the operation (UPDATE_VARIABLES / ADD_MESSAGE).
        let input_data = response.clone().unwrap_or(Value::Null);
        apply_operation(ctx, &config, &input_data, interaction_type)?;

        let (approved, responded) = match &response {
            Some(Value::String(s)) => (s == "approved" || s == "yes" || s == "true", true),
            Some(Value::Bool(b)) => (*b, true),
            _ => (false, responded),
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "operation_type".to_string(),
            Value::String(interaction_type.to_string()),
        );
        metadata.insert("responded".to_string(), Value::Bool(responded));
        metadata.insert("approved".to_string(), Value::Bool(approved));
        if let Some(id) = &interaction_id {
            metadata.insert("interaction_id".to_string(), Value::String(id.clone()));
        }

        let output = response.unwrap_or(Value::Null);

        Ok(NodeExecutionResult {
            output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

fn operation_name(config: &Value) -> String {
    config
        .get("operation_type")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN")
        .to_string()
}

/// Apply UPDATE_VARIABLES (array of `{variableName, expression}` entries or
/// an object map) and ADD_MESSAGE (append to the message context) operations.
fn apply_operation(
    ctx: &mut NodeExecutionContext,
    config: &Value,
    input_data: &Value,
    _interaction_type: &str,
) -> WorkflowResult<()> {
    let operation = operation_name(config);
    match operation.as_str() {
        "UPDATE_VARIABLES" | "update_variables" => {
            if let Some(entries) = config.get("variables").and_then(|v| v.as_array()) {
                for entry in entries {
                    let name = entry.get("variable_name").and_then(|v| v.as_str());
                    let expression = entry
                        .get("expression")
                        .and_then(|v| v.as_str())
                        .unwrap_or("{{input}}");
                    if let Some(name) = name {
                        let value = evaluate_expression(expression, input_data);
                        let resolved =
                            crate::variable::VariableResolver::resolve(&value, &ctx.variables);
                        ctx.set_variable(name.to_string(), resolved)?;
                    }
                }
            }
        }
        "ADD_MESSAGE" | "add_message" => {
            if let Some(message) = config.get("message").and_then(|v| v.as_object()) {
                let role = message
                    .get("role")
                    .and_then(|v| v.as_str())
                    .map(|r| match r {
                        "user" => MessageRole::User,
                        "assistant" => MessageRole::Assistant,
                        "system" => MessageRole::System,
                        _ => MessageRole::User,
                    })
                    .unwrap_or(MessageRole::User);
                let template = message
                    .get("content_template")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let content = replace_input_placeholder(template, input_data);

                message_context::append_context(
                    &ctx.variables,
                    message_context::DEFAULT_CONTEXT_ID,
                    vec![Message {
                        id: wf_types::Id::new(),
                        role,
                        content: MessageContentValue::Text(content),
                        timestamp: wf_common::now(),
                        tool_call_id: None,
                        tool_name: None,
                        tool_calls: None,
                        thinking: None,
                        metadata: None,
                    }],
                );
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::WorkflowCoordinator;
    use crate::entity::WorkflowExecutionEntity;
    use crate::handler::HandlerRegistry;
    use crate::interaction::complete_interaction;
    use std::collections::HashMap;
    use std::sync::Arc;
    use wf_execution_shared::context::ExecutorContext;
    use wf_tools::registry::ToolRegistry;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{
        WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
    };

    #[test]
    fn input_placeholder_replacement() {
        assert_eq!(
            replace_input_placeholder("Hello {{input}}!", &Value::from("world")),
            "Hello world!"
        );
        assert_eq!(
            evaluate_expression("{{input}}", &Value::from(42)),
            Value::from(42)
        );
        assert_eq!(
            evaluate_expression("count={{input}}", &Value::from("x")),
            Value::from("count=x")
        );
        assert_eq!(
            evaluate_expression("constant", &Value::Null),
            Value::from("constant")
        );
    }

    fn node(id: &str, node_type: &str, inner: Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn edge(source: &str, target: &str) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{}-{}", source, target),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
        }
    }

    fn build_graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        }
    }

    #[tokio::test]
    async fn async_wait_resolves_and_updates_variables() {
        let _registry_guard = crate::interaction::acquire_registry_test_lock().await;
        crate::interaction::interaction_registry().reset_for_tests();
        let bus = Arc::new(wf_core::EventBus::new(64));
        let mut sub = bus.subscribe();

        let graph = build_graph(
            vec![
                node("start", "START", Value::Null),
                node(
                    "interaction",
                    "USER_INTERACTION",
                    serde_json::json!({
                        "prompt": "enter value",
                        "timeout": 5000,
                        "operation_type": "update_variables",
                        "variables": [
                            {"variable_name": "user_choice", "expression": "{{input}}"}
                        ]
                    }),
                ),
                node("end", "END", Value::Null),
            ],
            vec![edge("start", "interaction"), edge("interaction", "end")],
        );

        let handlers = {
            let mut reg = HandlerRegistry::new();
            reg.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
            reg.into_arc()
        };
        let options = WorkflowExecutionOptions {
            input: Some(Value::Null),
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
            max_navigation_multiplier: None,
        };
        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            Some(bus.clone()),
            Arc::new(ToolRegistry::new()),
            options,
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
            .unwrap()
            .with_entity(entity);

        // Resolve the interaction as soon as its id is announced. Only
        // respond to this test's prompt: the process-global interaction
        // registry is shared with other tests running concurrently.
        let responder = tokio::spawn(async move {
            loop {
                match sub.recv().await {
                    Ok(event)
                        if event.r#type
                            == wf_types::events::EventType::FollowupQuestionRequested
                            && event
                                .metadata
                                .as_ref()
                                .and_then(|m| m.get("prompt"))
                                .and_then(|v| v.as_str())
                                == Some("enter value") =>
                    {
                        let id = event
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get("interaction_id"))
                            .and_then(|v| v.as_str())
                            .unwrap()
                            .to_string();
                        complete_interaction(&id, Value::from("banana"));
                        break;
                    }
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
        });

        let result = coordinator.execute().await;
        responder.await.unwrap();
        assert!(
            result.is_ok(),
            "workflow should complete: {:?}",
            result.err()
        );
        let snapshot = coordinator.state_snapshot().await.unwrap();
        assert_eq!(
            snapshot.status,
            wf_execution_shared::types::execution_entity::ExecutionStatus::Completed
        );
        assert!(snapshot
            .completed_nodes
            .contains(&"interaction".to_string()));
    }

    #[tokio::test]
    async fn timeout_fails_node_and_cleans_registry() {
        let _registry_guard = crate::interaction::acquire_registry_test_lock().await;
        crate::interaction::interaction_registry().reset_for_tests();
        let bus = Arc::new(wf_core::EventBus::new(64));
        let mut sub = bus.subscribe();
        let graph = build_graph(
            vec![
                node("start", "START", Value::Null),
                node(
                    "interaction",
                    "USER_INTERACTION",
                    serde_json::json!({
                        "prompt": "never answered",
                        "timeout": 50,
                    }),
                ),
                node("end", "END", Value::Null),
            ],
            vec![edge("start", "interaction"), edge("interaction", "end")],
        );

        let handlers = {
            let mut reg = HandlerRegistry::new();
            reg.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
            reg.into_arc()
        };
        let options = WorkflowExecutionOptions {
            input: Some(Value::Null),
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
            max_navigation_multiplier: None,
        };
        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            Some(bus),
            Arc::new(ToolRegistry::new()),
            options,
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
            .unwrap()
            .with_entity(entity);

        // Capture the interaction id from the request event; the timeout path
        // must clean the registry entry for exactly this id (B9: no leaked
        // registration / channel).
        let subscriber = tokio::spawn(async move {
            loop {
                match sub.recv().await {
                    Ok(event)
                        if event.r#type
                            == wf_types::events::EventType::FollowupQuestionRequested =>
                    {
                        break event
                            .metadata
                            .and_then(|m| m.get("interaction_id").cloned())
                            .and_then(|v| v.as_str().map(|s| s.to_string()));
                    }
                    Ok(_) => continue,
                    Err(_) => break None,
                }
            }
        });

        let result = coordinator.execute().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout"));

        let interaction_id = subscriber
            .await
            .unwrap()
            .expect("interaction request event must be published");

        // The timed-out interaction must have been removed from the registry.
        assert!(!crate::interaction::interaction_registry().is_pending(&interaction_id));
        assert_eq!(
            crate::interaction::interaction_registry().pending_count(),
            0
        );
        crate::interaction::interaction_registry().reset_for_tests();
    }

    /// B9 cancel path: cancelling a workflow while an interaction is pending
    /// leaves no registry residue once the run settles.
    #[tokio::test]
    async fn cancellation_while_waiting_cleans_registry() {
        let _registry_guard = crate::interaction::acquire_registry_test_lock().await;
        crate::interaction::interaction_registry().reset_for_tests();
        let bus = Arc::new(wf_core::EventBus::new(64));
        let mut sub = bus.subscribe();

        let graph = build_graph(
            vec![
                node("start", "START", Value::Null),
                node(
                    "interaction",
                    "USER_INTERACTION",
                    serde_json::json!({
                        "prompt": "never answered",
                        "timeout": 800,
                    }),
                ),
                node("end", "END", Value::Null),
            ],
            vec![edge("start", "interaction"), edge("interaction", "end")],
        );

        let handlers = {
            let mut reg = HandlerRegistry::new();
            reg.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
            reg.into_arc()
        };
        let options = WorkflowExecutionOptions {
            input: Some(Value::Null),
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
            max_navigation_multiplier: None,
        };
        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            Some(bus),
            Arc::new(ToolRegistry::new()),
            options,
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        // Keep a handle on the interruption state so the test can cancel the
        // workflow while the interaction node is still waiting.
        let interruption = entity.interruption().clone();
        let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
            .unwrap()
            .with_entity(entity);

        let run = tokio::spawn(async move { coordinator.execute().await });

        // Wait until the interaction request is announced and pending.
        let interaction_id = loop {
            match sub.recv().await {
                Ok(event) if event.r#type == EventType::FollowupQuestionRequested => {
                    break event
                        .metadata
                        .and_then(|m| m.get("interaction_id").cloned())
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .expect("interaction id must be present");
                }
                Ok(_) => continue,
                Err(_) => panic!("event bus closed before the interaction request"),
            }
        };
        assert!(crate::interaction::interaction_registry().is_pending(&interaction_id));

        // Cancel the workflow while the interaction is still waiting.
        interruption.stop().expect("stop signal must send");

        let result = run.await.unwrap();
        assert!(
            result.is_err(),
            "cancelled run must not complete: {:?}",
            result
        );

        // The pending entry must be gone after the run settles (B9: no leak).
        assert!(!crate::interaction::interaction_registry().is_pending(&interaction_id));
        assert_eq!(
            crate::interaction::interaction_registry().pending_count(),
            0
        );
        crate::interaction::interaction_registry().reset_for_tests();
    }
}
