use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use wf_core::EventBus;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};

use wf_llm::LlmGateway;
use wf_types::events::{BaseEvent, EventType};
use wf_types::llm::{LlmRequest, MessageStreamEvent, ToolCallFormatConfig};
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::message_context;

fn emit_llm_event(
    event_bus: Option<&EventBus>,
    event_type: EventType,
    ctx: &NodeExecutionContext,
    metadata: HashMap<String, Value>,
) {
    let Some(bus) = event_bus else { return };
    let _ = bus.publish(BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: Some(ctx.execution_id.clone()),
        execution_id: Some(ctx.execution_id.clone()),
        agent_loop_id: Some(ctx.node_id.clone()),
        metadata: Some(metadata),
    });
}

/// Emit token usage events: single-shot warning and limit exceeded are driven
/// by the execution-scoped tracker (P2 semantics); the compression requested
/// event is driven by the *named message array* read by this node — it fires
/// only when that array's estimated token count exceeds the node-level limit,
/// carrying the array name and its message snapshot so the trigger can
/// compress exactly that array.
async fn emit_token_usage_events(ctx: &NodeExecutionContext, warning_threshold: u64) {
    let (Some(ref tracker), Some(ref bus)) = (&ctx.token_tracker, &ctx.event_bus) else {
        return;
    };
    let mut tracker = tracker.lock().await;
    let token_limit = tracker.token_limit();
    if token_limit == 0 {
        return;
    }
    let tokens_used = tracker.cumulative_usage().total_tokens as u64;
    if tracker.consume_warning(warning_threshold as f64) {
        let percentage = tracker.usage_percentage().unwrap_or(0.0);
        let _ = bus.publish(wf_llm::build_token_usage_warning_event(
            &ctx.execution_id,
            Some(&ctx.node_id),
            tokens_used,
            token_limit,
            percentage,
        ));
    }
    if tracker.is_token_limit_exceeded() {
        let _ = bus.publish(wf_llm::build_token_limit_exceeded_event(
            &ctx.execution_id,
            Some(&ctx.node_id),
            tokens_used,
            token_limit,
        ));
    }

    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let context_id = config
        .get("context_id")
        .or_else(|| config.get("contextId"))
        .and_then(|v| v.as_str())
        .unwrap_or(message_context::DEFAULT_CONTEXT_ID);
    let context_messages = message_context::get_context(&ctx.variables, context_id);
    if context_messages.is_empty() {
        // No named array to compress: nothing to write back to.
        return;
    }
    let estimated = wf_llm::estimate_messages(&context_messages) as u64;
    if estimated > token_limit {
        let _ = bus.publish(wf_llm::build_context_compression_requested_event(
            &ctx.execution_id,
            Some(&ctx.node_id),
            context_id,
            estimated,
            token_limit,
            context_messages.len(),
            Some(&context_messages),
        ));
    }
}

fn text_message(role: MessageRole, content: String) -> Message {
    Message {
        id: wf_types::Id::new(),
        role,
        content: MessageContentValue::Text(content),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

fn message_to_text(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(parts) => {
            let mut out = String::new();
            for part in parts {
                if let wf_types::message::MessageContent::Text { text } = part {
                    out.push_str(text);
                }
            }
            out
        }
    }
}

fn tool_result_message(
    tool_call_id: &str,
    tool_name: &str,
    content: String,
    is_error: bool,
) -> Message {
    Message {
        id: wf_types::Id::new(),
        role: MessageRole::Tool,
        content: MessageContentValue::Text(content),
        timestamp: wf_common::now(),
        tool_call_id: Some(tool_call_id.to_string()),
        tool_name: Some(tool_name.to_string()),
        tool_calls: None,
        thinking: None,
        metadata: Some(HashMap::from([(
            "is_error".to_string(),
            Value::Bool(is_error),
        )])),
    }
}

/// Collect the initial message list for the request:
/// system prompt, optional transform-context injection, messages from the
/// named context (default `current`), inline `messages` config, and finally
/// the node input when nothing else produced messages.
fn build_messages(ctx: &NodeExecutionContext) -> WorkflowResult<Vec<Message>> {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let mut messages: Vec<Message> = Vec::new();

    if let Some(system) = config.get("system_prompt").and_then(|v| v.as_str()) {
        messages.push(text_message(MessageRole::System, system.to_string()));
    }

    // transform_context: basic injection of extra messages before the context
    // (dynamic context injection; compression strategies are not implemented).
    if let Some(transform) = config.get("transform_context") {
        if let Some(injected) = transform.get("messages").and_then(|v| v.as_array()) {
            for msg_val in injected {
                if let Ok(msg) = serde_json::from_value::<Message>(msg_val.clone()) {
                    messages.push(msg);
                }
            }
        }
    }

    let context_id = config
        .get("context_id")
        .and_then(|v| v.as_str())
        .unwrap_or(message_context::DEFAULT_CONTEXT_ID);
    let context_messages = message_context::get_context(&ctx.variables, context_id);
    if !context_messages.is_empty() {
        messages.extend(context_messages);
    }

    if let Some(msgs) = config.get("messages").and_then(|v| v.as_array()) {
        for msg_val in msgs {
            if let Ok(msg) = serde_json::from_value::<Message>(msg_val.clone()) {
                messages.push(msg);
            }
        }
    }

    if messages.is_empty() {
        let text = if let Value::String(s) = &ctx.input {
            s.clone()
        } else {
            ctx.input.to_string()
        };
        messages.push(text_message(MessageRole::User, text));
    }

    Ok(messages)
}

/// Resolve declared tool names against the tool registry.
fn resolve_tools(ctx: &NodeExecutionContext) -> WorkflowResult<Vec<wf_types::tool::Tool>> {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let Some(names) = config.get("tools").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let names: Vec<String> = names
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let registry = ctx.tool_registry.as_ref().ok_or_else(|| {
        WorkflowError::OperationError(
            "LLM node declares tools but no tool registry is available".to_string(),
        )
    })?;

    let mut tools = Vec::new();
    for name in names {
        match registry.get_tool(&name) {
            Some(tool) => tools.push(tool),
            None => {
                return Err(WorkflowError::OperationError(format!(
                    "Tool '{}' declared by LLM node is not registered",
                    name
                )))
            }
        }
    }
    Ok(tools)
}

/// Single LLM call. Returns the model response and the messages that must be
/// appended to the conversation (assistant message + any tool results).
async fn call_llm(
    gateway: &LlmGateway,
    request: &LlmRequest,
) -> WorkflowResult<wf_types::llm::LlmResult> {
    gateway
        .generate(request)
        .await
        .map_err(|e| WorkflowError::Internal(format!("LLM call failed: {}", e)))
}

/// Execute one tool call through the registry, returning a Tool message.
async fn execute_tool_call(
    ctx: &NodeExecutionContext,
    call: &wf_types::message::LlmToolCall,
) -> Message {
    let tool_name = call.function.name.clone();
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
    let tool_ctx =
        wf_tools::executor::trait_def::ToolExecutionContext::new(ctx.execution_id.clone())
            .with_node_id(ctx.node_id.clone());
    let options = wf_types::tool::ToolExecutionOptions {
        timeout: None,
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    };

    let result = match &ctx.tool_registry {
        Some(registry) => {
            registry
                .execute_tool(&tool_name, &args, &options, &tool_ctx)
                .await
        }
        None => Err(wf_tools::error::ToolError::NotFound(tool_name.clone())),
    };

    match result {
        Ok(exec_result) => {
            let is_error = !exec_result.success;
            let content = exec_result
                .result
                .map(|v| v.to_string())
                .unwrap_or_else(|| "".to_string());
            tool_result_message(&call.id, &tool_name, content, is_error)
        }
        Err(e) => tool_result_message(&call.id, &tool_name, format!("Error: {}", e), true),
    }
}

pub struct LlmHandler {
    gateway: Arc<LlmGateway>,
}

impl LlmHandler {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl NodeHandler for LlmHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Llm
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let profile_id = config
            .get("profile_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                WorkflowError::OperationError("LLM node requires a profile_id".to_string())
            })?;

        // Node-level tool call format (canonical string) is applied at runtime
        // so validation and execution agree on the effective protocol.
        let tool_call_format = config
            .get("tool_call_format")
            .and_then(|v| v.as_str())
            .and_then(ToolCallFormatConfig::from_format_str);
        let violation_policy = config
            .get("violation_policy")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let stream_enabled = config
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_tool_calls = config
            .get("max_tool_calls_per_request")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);

        // Token tracking: apply the node-level limit (0 disables checks);
        // the shared execution tracker is updated after every LLM call.
        let token_warning_threshold = config
            .get("token_warning_threshold")
            .and_then(|v| v.as_u64())
            .unwrap_or(wf_llm::DEFAULT_TOKEN_WARNING_THRESHOLD as u64);
        if let Some(ref tracker) = ctx.token_tracker {
            if let Some(token_limit) = config.get("token_limit").and_then(|v| v.as_u64()) {
                let mut tracker = tracker.lock().await;
                if tracker.token_limit() == 0 && token_limit > 0 {
                    tracker.set_token_limit(token_limit);
                }
            }
        }

        let mut messages = build_messages(ctx)?;
        let tools = resolve_tools(ctx)?;

        let mut executed_tool_calls: Vec<Value> = Vec::new();
        let mut final_response: Option<wf_types::llm::LlmResult> = None;
        let mut aggregated_content: Option<String> = None;

        // Multi-round tool loop: keep calling the model while it emits tool
        // calls, feeding tool results back, up to max_tool_calls_per_request.
        for _round in 0..max_tool_calls {
            let request = LlmRequest {
                profile_id: profile_id.clone(),
                messages: messages.clone(),
                parameters: config.get("parameters").cloned(),
                tools: if tools.is_empty() {
                    None
                } else {
                    Some(tools.clone())
                },
                tool_call_format: tool_call_format
                    .as_ref()
                    .map(|config| config.format.clone()),
                locked_tool_call_format: tool_call_format.clone(),
                violation_policy: violation_policy.clone(),
                execution_id: Some(ctx.execution_id.to_string()),
                stream: None,
                dead_loop_detection: None,
                protocol_auto_converted: None,
            };

            // Pre-request token budget check: estimation is approximate, so
            // this is a warning only (never blocks the request); the provider's
            // real count takes precedence. Fires at most once per execution.
            if let Some(ref tracker) = ctx.token_tracker {
                let mut tracker = tracker.lock().await;
                let token_limit = tracker.token_limit();
                if token_limit > 0 {
                    let estimated = u64::from(wf_llm::estimate_request_tokens(&request));
                    if estimated > token_limit && tracker.consume_preflight_warning() {
                        if let Some(ref bus) = ctx.event_bus {
                            let _ = bus.publish(wf_llm::build_token_usage_warning_event(
                                &ctx.execution_id,
                                Some(&ctx.node_id),
                                estimated,
                                token_limit,
                                estimated as f64 / token_limit as f64 * 100.0,
                            ));
                        }
                    }
                }
            }

            if stream_enabled {
                let mut stream =
                    self.gateway.generate_stream(&request).await.map_err(|e| {
                        WorkflowError::Internal(format!("LLM stream failed: {}", e))
                    })?;
                let mut content_parts: Vec<String> = Vec::new();
                let mut stream_usage_seen = false;
                loop {
                    match stream.next().await {
                        Some(Ok(MessageStreamEvent::Stream(chunk))) => {
                            content_parts.push(chunk.content.clone());
                            emit_llm_event(
                                ctx.event_bus.as_deref(),
                                EventType::LlmStreamChunk,
                                ctx,
                                HashMap::from([(
                                    "delta".to_string(),
                                    Value::String(chunk.content.clone()),
                                )]),
                            );
                        }
                        Some(Ok(MessageStreamEvent::Text(text))) => {
                            content_parts.push(text.text.clone());
                            emit_llm_event(
                                ctx.event_bus.as_deref(),
                                EventType::LlmStreamChunk,
                                ctx,
                                HashMap::from([(
                                    "delta".to_string(),
                                    Value::String(text.text.clone()),
                                )]),
                            );
                        }
                        Some(Ok(MessageStreamEvent::Usage(u))) => {
                            stream_usage_seen = true;
                            if let Some(ref tracker) = ctx.token_tracker {
                                tracker
                                    .lock()
                                    .await
                                    .accumulate_stream_usage(&wf_llm::RequestUsage::from(&u.usage));
                            }
                        }
                        Some(Ok(MessageStreamEvent::FinalMessage(final_msg))) => {
                            let tool_calls = final_msg.message.tool_calls.clone();
                            if let Some(usage) = &final_msg.usage {
                                stream_usage_seen = true;
                                if let Some(ref tracker) = ctx.token_tracker {
                                    tracker.lock().await.accumulate_stream_usage(
                                        &wf_llm::RequestUsage::from(usage),
                                    );
                                }
                            }
                            final_response = Some(wf_types::llm::LlmResult {
                                id: None,
                                model: profile_id.clone(),
                                content: Some(message_to_text(&final_msg.message)),
                                message: final_msg.message,
                                tool_calls,
                                usage: final_msg.usage,
                                finish_reason: Some("stop".to_string()),
                                duration: 0,
                                reasoning_content: None,
                                reasoning_tokens: None,
                                metadata: None,
                                stream_stats: None,
                                warnings: None,
                            });
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            return Err(WorkflowError::Internal(format!("LLM stream error: {}", e)))
                        }
                        None => break,
                    }
                }
                if let Some(ref tracker) = ctx.token_tracker {
                    let mut tracker = tracker.lock().await;
                    if !stream_usage_seen {
                        // Provider streamed no usage: estimate as fallback.
                        let completion = content_parts.concat();
                        tracker.update_estimated_usage(
                            wf_llm::estimate_request_tokens(&request),
                            wf_llm::estimate_tokens(&completion) as u32,
                        );
                    }
                    tracker.finalize_current_request();
                }
                emit_token_usage_events(ctx, token_warning_threshold).await;
                aggregated_content = Some(content_parts.concat());
                break;
            }

            let response = call_llm(&self.gateway, &request).await?;
            if let Some(ref tracker) = ctx.token_tracker {
                let mut tracker = tracker.lock().await;
                if let Some(usage) = &response.usage {
                    tracker.update_api_usage(usage);
                } else {
                    // Provider reported no usage: estimate as fallback so
                    // limit checks and history stay populated.
                    let completion = response.content.as_deref().unwrap_or_default();
                    tracker.update_estimated_usage(
                        wf_llm::estimate_request_tokens(&request),
                        wf_llm::estimate_tokens(completion) as u32,
                    );
                }
                tracker.finalize_current_request();
            }
            emit_token_usage_events(ctx, token_warning_threshold).await;
            let has_tool_calls = response
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty());

            messages.push(response.message.clone());
            final_response = Some(response.clone());
            aggregated_content = response.content.clone();

            if !has_tool_calls {
                break;
            }

            let calls = response.tool_calls.unwrap_or_default();
            let mut any_result = false;
            for call in &calls {
                let result_msg = execute_tool_call(ctx, call).await;
                let is_error = result_msg
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("is_error"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                executed_tool_calls.push(serde_json::json!({
                    "id": call.id,
                    "name": call.function.name,
                    "success": !is_error,
                }));
                messages.push(result_msg);
                any_result = true;
            }
            if !any_result {
                break;
            }
        }

        let output = aggregated_content
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null);

        let mut metadata = HashMap::new();
        if let Some(response) = &final_response {
            if !response.model.is_empty() {
                metadata.insert("model".to_string(), Value::String(response.model.clone()));
            }
            if let Some(finish_reason) = &response.finish_reason {
                metadata.insert(
                    "finish_reason".to_string(),
                    Value::String(finish_reason.clone()),
                );
            }
            if let Some(usage) = &response.usage {
                metadata.insert(
                    "prompt_tokens".to_string(),
                    Value::Number(usage.prompt_tokens.into()),
                );
                metadata.insert(
                    "completion_tokens".to_string(),
                    Value::Number(usage.completion_tokens.into()),
                );
            }
        }
        let executed_tool_count = executed_tool_calls.len();
        if !executed_tool_calls.is_empty() {
            metadata.insert("tool_calls".to_string(), Value::Array(executed_tool_calls));
        }
        metadata.insert("stream".to_string(), Value::Bool(stream_enabled));

        // Write the assistant response to the configured output context
        // (mirrors the TS outputContext semantics; the read context is left
        // untouched so the compression chain can replace it as a unit).
        if let (Some(response), Some(content)) = (&final_response, &aggregated_content) {
            if !content.is_empty() {
                let mut out_msg = response.message.clone();
                if out_msg.content == MessageContentValue::Text(String::new()) {
                    out_msg.content = MessageContentValue::Text(content.clone());
                }
                let output_context_id = config
                    .get("output_context")
                    .or_else(|| config.get("outputContext"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(id) = output_context_id {
                    message_context::append_context(&ctx.variables, &id, vec![out_msg]);
                }
            }
        }

        emit_llm_event(
            ctx.event_bus.as_deref(),
            EventType::NodeCustomEvent,
            ctx,
            HashMap::from([
                (
                    "event".to_string(),
                    Value::String("llm_node_completed".to_string()),
                ),
                (
                    "tool_call_count".to_string(),
                    Value::Number(serde_json::Number::from(executed_tool_count as u64)),
                ),
            ]),
        );

        Ok(NodeExecutionResult {
            output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::HandlerRegistry;

    fn msg(role: MessageRole, text: &str) -> Message {
        text_message(role, text.to_string())
    }

    #[test]
    fn builds_system_and_context_messages() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        message_context::append_context(&vars, "chat", vec![msg(MessageRole::User, "hello")]);

        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "system_prompt": "be brief",
            "context_id": "chat"
        }));

        let messages = build_messages(&ctx).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, MessageRole::System);
        assert_eq!(messages[1].role, MessageRole::User);
    }

    #[test]
    fn transform_context_injects_messages() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "transform_context": {
                "messages": [{"role": "user", "content": "injected", "id": "m1", "timestamp": 1}]
            }
        }));

        // transform-injected message must deserialize through the Message type.
        let messages = build_messages(&ctx).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::User);
    }

    #[test]
    fn resolves_no_tools_without_config() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({}));
        assert!(resolve_tools(&ctx).unwrap().is_empty());
    }

    #[test]
    fn unknown_tool_errors() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let registry = std::sync::Arc::new(wf_tools::registry::ToolRegistry::new());
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "tools": ["missing_tool"]
        }));
        let mut ctx = ctx;
        ctx.tool_registry = Some(registry);
        let err = resolve_tools(&ctx).unwrap_err();
        assert!(err.to_string().contains("missing_tool"));
    }

    #[test]
    fn message_serialization_roundtrip() {
        let m = msg(MessageRole::Assistant, "hi there");
        let json = serde_json::to_value(&m).unwrap();
        let back: Message = serde_json::from_value(json).unwrap();
        assert_eq!(back.role, MessageRole::Assistant);
        assert_eq!(
            back.content,
            MessageContentValue::Text("hi there".to_string())
        );
    }

    #[allow(dead_code)]
    fn _registry_check() {
        let mut reg = HandlerRegistry::new();
        reg.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
    }
}
