use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::message::Message;
use wf_types::node::StaticNodeType;
use wf_types::trigger::TriggerAction;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::trigger::{TriggerContext, TriggerCoordinator};
use crate::handler::NodeHandler;
use crate::message_context;
use crate::trigger_internal;

/// Parse a TriggerAction from a node config value.
///
/// Accepted layouts:
/// - `config.trigger` / `config.action` holding the tagged action object
/// - the config itself holding the tagged action object
pub fn parse_trigger_action(config: &Value) -> Option<TriggerAction> {
    let candidate = config
        .get("trigger")
        .or_else(|| config.get("action"))
        .unwrap_or(config);
    candidate.get("action_type")?;
    serde_json::from_value(candidate.clone()).ok()
}

/// Build a TriggerContext from a NodeExecutionContext.
fn build_trigger_context(ctx: &NodeExecutionContext) -> WorkflowResult<TriggerContext> {
    let mut tctx = TriggerContext::new(
        ctx.execution_id.clone(),
        wf_types::Id::from(ctx.execution_id.clone()),
    )
    .with_variables(ctx.variables.clone());
    match &ctx.event_bus {
        Some(bus) => tctx = tctx.with_event_bus(bus.clone()),
        None => {
            tracing::debug!(
                execution_id = %ctx.execution_id,
                node_id = %ctx.node_id,
                "no event bus attached, trigger context built without an event bus"
            );
        }
    }
    // Wire the typed signal bus (replaces the `__`-prefixed variable protocol).
    if let Some(bus) = &ctx.signal_bus {
        tctx = tctx.with_signal_bus(bus.clone());
    }
    // Wire the session-level cache shared across this node visit's trigger
    // actions.
    if let Some(cache) = &ctx.session_cache {
        tctx = tctx.with_session_cache(cache.clone());
    }
    let handlers = crate::handler::resolve_handler_registry(ctx)?;
    tctx = tctx.with_handlers(handlers);
    if let Some(registry) = &ctx.tool_registry {
        tctx = tctx.with_tool_registry(registry.clone());
    }
    if let Some(metrics) = &ctx.metrics {
        tctx = tctx.with_metrics(metrics.clone());
    }
    if let Some(token) = &ctx.cancellation {
        tctx = tctx.with_cancellation(token.clone());
    }
    Ok(tctx)
}

/// Execute the trigger action and return the next node ids to navigate to.
async fn execute_action(
    action: &TriggerAction,
    ctx: &mut NodeExecutionContext,
) -> WorkflowResult<Vec<String>> {
    let tctx = build_trigger_context(ctx)?;
    let result = TriggerCoordinator::execute(action, "node_trigger", &tctx).await;
    if let Some(err) = &result.error {
        return Err(WorkflowError::TriggerError(err.clone()));
    }

    if let TriggerAction::SkipNode { node_id } = action {
        if let (Some(target), Some(graph)) = (node_id, ctx.graph_structure.as_ref()) {
            let next = graph
                .edges
                .iter()
                .find(|e| e.source_node_id == *target)
                .map(|e| e.target_node_id.clone())
                .or_else(|| Some(target.clone()));
            if let Some(next) = next {
                return Ok(vec![next]);
            }
        }
    }
    Ok(Vec::new())
}

/// Map `message_inputs` (or camelCase `messageInputs`) entries from the node
/// input object into workflow variables.
fn map_message_inputs(ctx: &mut NodeExecutionContext) -> WorkflowResult<()> {
    let Some(config) = &ctx.node_config else {
        return Ok(());
    };
    let Some(inputs) = config
        .get("message_inputs")
        .or_else(|| config.get("messageInputs"))
        .and_then(|v| v.as_array())
    else {
        return Ok(());
    };

    let input_obj = ctx.input.as_object().cloned().unwrap_or_default();
    for entry in inputs {
        let source = entry
            .get("source_context_id")
            .or_else(|| entry.get("sourceContextId"))
            .and_then(|v| v.as_str());
        let internal = entry
            .get("internal_name")
            .or_else(|| entry.get("internalName"))
            .and_then(|v| v.as_str());
        let required = entry
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let (Some(source), Some(internal)) = (source, internal) else {
            continue;
        };

        match input_obj.get(source) {
            Some(value) => {
                // Message inputs are registered as named message contexts
                // (not plain variables): LLM / downstream nodes read them via
                // `context_id`.
                if let Ok(messages) = serde_json::from_value::<Vec<Message>>(value.clone()) {
                    message_context::register_context(&ctx.variables, internal, messages);
                } else {
                    ctx.set_variable(internal, value.clone())?;
                }
            }
            None if required => {
                return Err(WorkflowError::TriggerError(format!(
                    "Required trigger input '{}' (mapped to '{}') is missing",
                    source, internal
                )));
            }
            None => {}
        }
    }
    Ok(())
}

/// Export `variable_outputs` (or camelCase) entries: copy the named values
/// from the node input object into workflow variables.
fn export_variable_outputs(ctx: &mut NodeExecutionContext) -> WorkflowResult<()> {
    let Some(config) = &ctx.node_config else {
        return Ok(());
    };
    let Some(outputs) = config
        .get("variable_outputs")
        .or_else(|| config.get("variableOutputs"))
        .and_then(|v| v.as_array())
    else {
        return Ok(());
    };

    let input_obj = ctx.input.as_object().cloned().unwrap_or_default();
    for entry in outputs {
        let internal = entry
            .get("internal_name")
            .or_else(|| entry.get("internalName"))
            .and_then(|v| v.as_str());
        let target = entry
            .get("target_variable")
            .or_else(|| entry.get("targetVariable"))
            .and_then(|v| v.as_str());
        if let (Some(internal), Some(target)) = (internal, target) {
            if let Some(value) = input_obj.get(internal) {
                ctx.set_variable(target, value.clone())?;
            }
        }
    }
    Ok(())
}

/// Export `message_outputs` (or camelCase) entries: expose the named message
/// context as the node output (serialized `Vec<Message>`), so a triggered
/// sub-workflow's final output is the message array (e.g. the compressed
/// summary) that the event-driven trigger listener writes back. The first
/// entry with a non-empty context wins.
fn export_message_outputs(ctx: &mut NodeExecutionContext) -> Option<Value> {
    let config = ctx.node_config.as_ref()?;
    let outputs = config
        .get("message_outputs")
        .or_else(|| config.get("messageOutputs"))
        .and_then(|v| v.as_array())?;

    for entry in outputs {
        let internal = entry
            .get("internal_name")
            .or_else(|| entry.get("internalName"))
            .and_then(|v| v.as_str())?;
        let messages = message_context::get_context(&ctx.variables, internal);
        if messages.is_empty() {
            continue;
        }
        return serde_json::to_value(&messages).ok();
    }
    None
}

/// START_FROM_MESSAGE: initializes workflow state from trigger input and
/// optionally applies a trigger action from the node config.
pub struct StartFromMessageHandler;

#[async_trait]
impl NodeHandler for StartFromMessageHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::StartFromMessage
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl StartFromMessageHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let already_executed = ctx
            .get_variable(&trigger_internal::completed_marker(&ctx.node_id))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if already_executed {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        }
        ctx.set_internal_variable(
            trigger_internal::completed_marker(&ctx.node_id),
            Value::from(true),
        );

        map_message_inputs(ctx)?;

        let next_node_ids = match ctx.node_config.as_ref().and_then(parse_trigger_action) {
            Some(action) => execute_action(&action, ctx).await?,
            None => Vec::new(),
        };

        Ok(NodeExecutionResult::with_next_nodes(
            ctx.input.clone(),
            next_node_ids,
        ))
    }
}

/// CONTINUE_FROM_MESSAGE: hands data back to the main workflow and applies
/// a trigger action from the node config when present.
pub struct ContinueFromMessageHandler;

#[async_trait]
impl NodeHandler for ContinueFromMessageHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::ContinueFromMessage
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl ContinueFromMessageHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let already_executed = ctx
            .get_variable(&trigger_internal::completed_marker(&ctx.node_id))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if already_executed {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        }
        ctx.set_internal_variable(
            trigger_internal::completed_marker(&ctx.node_id),
            Value::from(true),
        );

        export_variable_outputs(ctx)?;
        let message_output = export_message_outputs(ctx);

        let next_node_ids = match ctx.node_config.as_ref().and_then(parse_trigger_action) {
            Some(action) => execute_action(&action, ctx).await?,
            None => Vec::new(),
        };

        // The node output is the exported message array when present (the
        // compression sub-workflow's final output), the node input otherwise.
        let output = message_output.unwrap_or_else(|| ctx.input.clone());
        Ok(NodeExecutionResult::with_next_nodes(output, next_node_ids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use std::collections::HashMap;

    use dashmap::DashMap;
    use wf_execution_shared::context::ExecutorContext;
    use wf_tools::registry::ToolRegistry;
    use wf_types::message::Message;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{
        WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
    };

    use crate::coordinator::WorkflowCoordinator;
    use crate::entity::WorkflowExecutionEntity;
    use crate::handler::HandlerRegistry;

    fn options_with_input(input: serde_json::Value) -> WorkflowExecutionOptions {
        WorkflowExecutionOptions {
            input: Some(input),
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
        }
    }

    fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
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

    async fn run(
        graph: WorkflowGraphStructure,
        options: WorkflowExecutionOptions,
    ) -> WorkflowResult<Value> {
        let handlers = {
            let mut reg = HandlerRegistry::new();
            reg.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
            reg.into_arc()
        };
        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            None,
            Arc::new(ToolRegistry::new()),
            options,
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator =
            WorkflowCoordinator::new(exec_ctx, graph, handlers)?.with_entity(entity);
        coordinator.execute().await
    }

    fn msg(role: wf_types::message::MessageRole, text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role,
            content: wf_types::message::MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn test_parse_trigger_action() {
        let config = serde_json::json!({
            "trigger": {
                "action_type": "set_variable",
                "variable_name": "x",
                "value": 42
            }
        });
        let action = parse_trigger_action(&config).expect("action should parse");
        assert!(matches!(
            action,
            TriggerAction::SetVariable { ref variable_name, .. } if variable_name == "x"
        ));

        assert!(parse_trigger_action(&serde_json::json!({"id": "n1"})).is_none());
    }

    #[tokio::test]
    async fn continue_from_message_exports_message_outputs() {
        let vars = std::sync::Arc::new(DashMap::new());
        crate::message_context::append_context(
            &vars,
            "compressed",
            vec![msg(wf_types::message::MessageRole::Assistant, "summary")],
        );

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm-summary-end".to_string(),
            StaticNodeType::ContinueFromMessage,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "messageOutputs": [{
                "internalName": "compressed",
                "targetContextId": "current"
            }]
        }));

        let handler = ContinueFromMessageHandler;
        let result = handler.execute(&mut ctx).await.unwrap();
        let messages: Vec<Message> = serde_json::from_value(result.output).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].content,
            wf_types::message::MessageContentValue::Text("summary".to_string())
        );
    }

    #[tokio::test]
    async fn continue_from_message_falls_back_to_input_without_message_outputs() {
        let vars = std::sync::Arc::new(DashMap::new());
        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "end-node".to_string(),
            StaticNodeType::ContinueFromMessage,
            Value::String("plain".to_string()),
            vars,
        )
        .with_node_config(serde_json::json!({}));

        let handler = ContinueFromMessageHandler;
        let result = handler.execute(&mut ctx).await.unwrap();
        assert_eq!(result.output, Value::String("plain".to_string()));
    }

    #[tokio::test]
    async fn test_start_from_message_maps_message_inputs() {
        let graph = build_graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "trigger",
                    "START_FROM_MESSAGE",
                    serde_json::json!({
                        "message_inputs": [{
                            "source_context_id": "user_name",
                            "internal_name": "greeting",
                            "required": true
                        }]
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![edge("start", "trigger"), edge("trigger", "end")],
        );

        let result = run(
            graph.clone(),
            options_with_input(serde_json::json!({"user_name": "alice"})),
        )
        .await;
        assert!(result.is_ok(), "workflow should run: {:?}", result.err());

        let missing = run(graph, options_with_input(serde_json::json!({}))).await;
        assert!(missing.is_err(), "missing required input must fail");
    }

    #[tokio::test]
    async fn test_trigger_action_pause_and_stop() {
        let graph = build_graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "trigger",
                    "CONTINUE_FROM_MESSAGE",
                    serde_json::json!({
                        "trigger": { "action_type": "pause_workflow_execution" }
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![edge("start", "trigger"), edge("trigger", "end")],
        );

        let paused = run(graph, options_with_input(Value::Null)).await;
        assert!(paused.is_err());
        assert!(paused.unwrap_err().to_string().contains("paused"));

        let stop_graph = build_graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "trigger",
                    "CONTINUE_FROM_MESSAGE",
                    serde_json::json!({
                        "trigger": { "action_type": "stop_workflow_execution" }
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![edge("start", "trigger"), edge("trigger", "end")],
        );

        let stopped = run(stop_graph, options_with_input(Value::Null)).await;
        assert!(stopped.is_err());
        assert!(stopped.unwrap_err().to_string().contains("stopped"));
    }
}
