use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use wf_agent::VariableBackedVisibilityStore;
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
    let Some(bus) = event_bus else {
        tracing::debug!(
            execution_id = %ctx.execution_id,
            node_id = %ctx.node_id,
            ?event_type,
            "no event bus, skipping llm event"
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
            // A plain workflow LLM node is not an agent loop; the id stays None
            // so listeners can tell agent-owned targets (agent_loop_id present)
            // from workflow variable-map targets.
            agent_loop_id: None,

            event_name: None,
            metadata: Some(metadata),
        },
        &format!("workflow={} llm={}", ctx.execution_id, ctx.node_id),
    )
    .ok();
}

/// Publish the stream termination event (error vs abort) for the LLM node's
/// streaming path. Consumer-layer publishing keeps wf-llm free of the event
/// bus dependency; the builders live in wf-llm.
fn publish_stream_termination(
    event_bus: Option<&EventBus>,
    ctx: &NodeExecutionContext,
    profile_id: &str,
    aborted: bool,
    message: &str,
) {
    let Some(bus) = event_bus else {
        tracing::debug!(
            execution_id = %ctx.execution_id,
            node_id = %ctx.node_id,
            "no event bus, skipping llm stream termination event"
        );
        return;
    };
    if aborted {
        bus.publish_logged(
            wf_llm::build_llm_stream_aborted_event(&ctx.execution_id, None, message, profile_id),
            &format!(
                "workflow={} llm={} stream-aborted",
                ctx.execution_id, ctx.node_id
            ),
        )
        .ok();
    } else {
        bus.publish_logged(
            wf_llm::build_llm_stream_error_event(&ctx.execution_id, None, message, profile_id),
            &format!(
                "workflow={} llm={} stream-error",
                ctx.execution_id, ctx.node_id
            ),
        )
        .ok();
    }
}

/// Safety-net path: the provider rejected the *actual* request with a
/// context-length-exceeded error. Emit a forced CONTEXT_COMPRESSION_REQUESTED
/// (audit copy) and dispatch the compression signal over the real request
/// messages so the chain fires even though the (undercounting) estimate
/// never crossed the threshold.
async fn publish_forced_compression(ctx: &NodeExecutionContext, request: &LlmRequest) {
    let Some(ref bus) = ctx.event_bus else {
        return;
    };
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let target = declared_contexts(config)
        .first()
        .cloned()
        .unwrap_or_else(|| message_context::DEFAULT_CONTEXT_ID.to_string());
    let tokens_used = u64::from(wf_llm::estimate_request_tokens(request));
    let message_count = request.messages.len();
    let array_version = message_context::array_version(&ctx.variables, &target);
    bus.publish_logged(
        wf_llm::build_context_compression_requested_event(
            &ctx.execution_id,
            None,
            &target,
            tokens_used,
            u64::MAX,
            message_count,
            array_version,
            true,
            Some(&request.messages),
        ),
        &format!(
            "workflow={} llm={} forced-compression",
            ctx.execution_id, ctx.node_id
        ),
    )
    .ok();
    dispatch_compression_signal(
        ctx,
        &target,
        tokens_used,
        u64::MAX,
        message_count,
        array_version,
        true,
        &request.messages,
    )
    .await;
}

/// Dispatch the `CONTEXT_COMPRESSION_REQUESTED` engine signal: registered
/// receivers (the compression service) are notified synchronously so the
/// summary sub-workflow takes over immediately. Workflow targets have no
/// `agent_loop_id`: the write-back goes through the execution registry.
#[allow(clippy::too_many_arguments)]
async fn dispatch_compression_signal(
    ctx: &NodeExecutionContext,
    target_context_id: &str,
    tokens_used: u64,
    token_limit: u64,
    message_count: usize,
    array_version: u64,
    forced: bool,
    messages: &[Message],
) {
    use wf_execution_shared::hooks::HookContext;
    use wf_llm::token_events::{
        KEY_ARRAY_VERSION, KEY_FORCED, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_TARGET_CONTEXT_ID,
        KEY_TOKENS_USED, KEY_TOKEN_LIMIT,
    };
    let Some(registry) = &ctx.hook_registry else {
        return;
    };
    let mut data = HashMap::new();
    data.insert(
        KEY_TARGET_CONTEXT_ID.to_string(),
        Value::String(target_context_id.to_string()),
    );
    data.insert(
        KEY_TOKENS_USED.to_string(),
        Value::Number(tokens_used.into()),
    );
    data.insert(
        KEY_TOKEN_LIMIT.to_string(),
        Value::Number(token_limit.into()),
    );
    data.insert(
        KEY_MESSAGE_COUNT.to_string(),
        Value::Number(serde_json::Number::from(message_count as u64)),
    );
    data.insert(
        KEY_ARRAY_VERSION.to_string(),
        Value::Number(serde_json::Number::from(array_version)),
    );
    if forced {
        data.insert(KEY_FORCED.to_string(), Value::Bool(true));
    }
    if let Ok(value) = serde_json::to_value(messages) {
        data.insert(KEY_MESSAGES.to_string(), value);
    }
    wf_execution_shared::hooks::dispatch(
        registry,
        &[],
        wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        &HookContext {
            execution_id: ctx.execution_id.clone(),
            hook_type: wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
            data,
        },
        ctx.event_bus.as_deref(),
    )
    .await;
}

/// Variable-map key carrying the execution-scoped token tracker state so
/// checkpoints restore the guards and accumulations with the execution state.
const TRACKER_STATE_KEY: &str = "__token_tracker__";

/// Metadata key of the preflight warning carrying per-array budget details.
const KEY_ARRAY_DETAILS: &str = "array_details";

/// Declared message arrays of this LLM node (2.1.2): the `contexts` check
/// list when configured, otherwise the single `context_id` array.
fn declared_contexts(config: &Value) -> Vec<String> {
    if let Some(contexts) = config.get("contexts").and_then(|v| v.as_array()) {
        let ids: Vec<String> = contexts
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        if !ids.is_empty() {
            return ids;
        }
    }
    let context_id = config
        .get("context_id")
        .or_else(|| config.get("contextId"))
        .and_then(|v| v.as_str())
        .unwrap_or(message_context::DEFAULT_CONTEXT_ID);
    vec![context_id.to_string()]
}

/// Messages injected by `transform_context` (part of every request; their
/// estimate participates in the per-array budget of the declared arrays).
fn injected_messages(config: &Value) -> Vec<Message> {
    config
        .get("transform_context")
        .and_then(|t| t.get("messages"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| serde_json::from_value::<Message>(m.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Restore the execution-scoped tracker from the variable map once (checkpoint
/// symmetry): a fresh execution tracker (no accumulated state) picks up
/// the checkpointed guards and accumulations together with the variables.
fn restore_tracker_from_variables(
    ctx: &NodeExecutionContext,
    tracker: &mut wf_llm::TokenUsageTracker,
) {
    if tracker.cumulative_usage().total_tokens > 0 || tracker.estimated_total() > 0 {
        return;
    }
    let Some(value) = ctx.variables.get(TRACKER_STATE_KEY) else {
        return;
    };
    if let Ok(state) = serde_json::from_value(value.clone()) {
        tracker.restore(state);
    }
}

/// Persist the execution-scoped tracker state into the variable map so
/// checkpoints (which snapshot the variables) restore the guards too.
fn persist_tracker_state(ctx: &NodeExecutionContext, tracker: &wf_llm::TokenUsageTracker) {
    if let Ok(value) = serde_json::to_value(tracker.state()) {
        ctx.variables.insert(TRACKER_STATE_KEY.to_string(), value);
    }
}

/// Emit token usage events (v2 dual-track semantics):
///
/// - warning: decision track (estimated cumulative), single-shot guard;
/// - limit exceeded: decision track, one emission per 50% tier band
///   (100%, 150%, 200%, ...);
/// - compression requested: per declared named array (2.1.2), driven by the
///   incremental ledger estimate + transform-context injections, guarded by
///   the array version (single-shot per version, checkpointed in the ledger).
///
/// No provider usage participates in any of these decisions.
async fn emit_token_usage_events(ctx: &NodeExecutionContext, warning_threshold: u64) {
    let (Some(ref tracker), Some(ref bus)) = (&ctx.token_tracker, &ctx.event_bus) else {
        return;
    };
    let mut tracker = tracker.lock().await;
    let token_limit = tracker.token_limit();
    if token_limit == 0 {
        return;
    }
    let tokens_used = tracker.estimated_total();
    if tracker.consume_warning(warning_threshold as f64) {
        let percentage = tracker.estimated_usage_percentage().unwrap_or(0.0);
        bus.publish_logged(
            wf_llm::build_token_usage_warning_event(
                &ctx.execution_id,
                Some(&ctx.node_id),
                tokens_used,
                token_limit,
                percentage,
            ),
            &format!(
                "workflow={} llm={} token-warning",
                ctx.execution_id, ctx.node_id
            ),
        )
        .ok();
    }
    if tracker.consume_limit_exceeded_tier().is_some() {
        bus.publish_logged(
            wf_llm::build_token_limit_exceeded_event(
                &ctx.execution_id,
                Some(&ctx.node_id),
                tokens_used,
                token_limit,
            ),
            &format!(
                "workflow={} llm={} token-exceeded",
                ctx.execution_id, ctx.node_id
            ),
        )
        .ok();
    }

    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let injected = injected_messages(config);
    let injected_estimate = u64::from(wf_llm::estimate_messages(&injected));
    let injected_count = injected.len();
    for context_id in declared_contexts(config) {
        let context_messages = message_context::get_context(&ctx.variables, &context_id);
        if context_messages.is_empty() {
            // No named array to compress: nothing to write back to.
            continue;
        }
        // Array budget = ledger estimate (recomputed lazily after
        // replacements) + this request's injected messages.
        let estimated = message_context::ledger_estimated_tokens(&ctx.variables, &context_id)
            + injected_estimate;
        let version = message_context::array_version(&ctx.variables, &context_id);
        if estimated > token_limit
            && message_context::should_emit_compression(&ctx.variables, &context_id, version)
        {
            let mut event = wf_llm::build_context_compression_requested_event(
                &ctx.execution_id,
                None,
                &context_id,
                estimated,
                token_limit,
                context_messages.len(),
                version,
                false,
                Some(&context_messages),
            );
            if injected_count > 0 {
                if let Some(meta) = event.metadata.as_mut() {
                    meta.insert(
                        wf_llm::KEY_INJECTED_MESSAGE_COUNT.to_string(),
                        Value::Number(serde_json::Number::from(injected_count as u64)),
                    );
                }
            }
            bus.publish_logged(
                event,
                &format!(
                    "workflow={} llm={} compression-requested",
                    ctx.execution_id, ctx.node_id
                ),
            )
            .ok();
            // Synchronous signal delivery: the compression service registered
            // as a receiver takes over immediately.
            dispatch_compression_signal(
                ctx,
                &context_id,
                estimated,
                token_limit,
                context_messages.len(),
                version,
                false,
                &context_messages,
            )
            .await;
            message_context::mark_compression_emitted(&ctx.variables, &context_id, version);
        }
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
///
/// Safety-net path: when the provider rejects the *actual* request with
/// a context-length-exceeded error (the local estimate undercounted), a
/// forced CONTEXT_COMPRESSION_REQUESTED is published over the real request
/// messages so the compression chain still fires.
async fn call_llm(
    ctx: &NodeExecutionContext,
    gateway: &LlmGateway,
    request: &LlmRequest,
) -> WorkflowResult<wf_types::llm::LlmResult> {
    match gateway.generate(request, ctx.cancellation.clone()).await {
        Ok(result) => Ok(result),
        Err(e) if e.is_context_length_exceeded() => {
            publish_forced_compression(ctx, request).await;
            Err(WorkflowError::Internal(format!("LLM call failed: {}", e)))
        }
        Err(e) => Err(WorkflowError::Internal(format!("LLM call failed: {}", e))),
    }
}

/// Execute one tool call through the registry, returning a Tool message.
async fn execute_tool_call(
    ctx: &NodeExecutionContext,
    call: &wf_types::message::LlmToolCall,
) -> Message {
    let tool_name = call.function.name.clone();
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);

    // Runtime visibility gate (aligned with the AGENT_LOOP path): tools
    // blocked by TOOL_VISIBILITY nodes are rejected here, before the
    // approval gate, so plain LLM nodes cannot bypass the block. The
    // model-visible schema stays unchanged (KV-cache friendly); blocked
    // calls are intercepted at execution time only.
    let visibility_store = VariableBackedVisibilityStore::new(ctx.variables.clone());
    if visibility_store.is_blocked(&tool_name) {
        return tool_result_message(
            &call.id,
            &tool_name,
            format!("Tool \"{tool_name}\" is not visible in this execution"),
            true,
        );
    }

    let tool_ctx =
        wf_tools::executor::trait_def::ToolExecutionContext::new(ctx.execution_id.clone())
            .with_node_id(ctx.node_id.clone());
    let options = wf_types::tool::ToolExecutionOptions {
        timeout: None,
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    };

    // Tool-level approval gate (pre-execution side-effect guard, aligned
    // with the agent path): an external handler decides; otherwise the
    // policy engine (auto-approval presets / patterns / risk rules) decides;
    // with neither the call is auto-approved.
    let approved = if let Some(handler) = &ctx.tool_approval_handler {
        let interaction_id = format!("approval-{}-{}", wf_common::now(), call.id);
        let request = wf_execution_shared::approval::ToolApprovalRequest {
            tool_call_id: call.id.clone(),
            tool_name: tool_name.clone(),
            arguments: args.clone(),
            interaction_id,
            batch_id: None,
            tool_index: None,
            total_tools: None,
            pending_queue: None,
        };
        let result = handler.request_approval(&request).await;
        match result.approved {
            true => Ok(result.edited_parameters),
            false => Err(result
                .rejection_reason
                .unwrap_or_else(|| "Rejected by user".to_string())),
        }
    } else {
        match &ctx.tool_approval_options {
            Some(approval_options) => {
                let risk_level = ctx
                    .tool_registry
                    .as_ref()
                    .and_then(|registry| registry.get_tool(&tool_name))
                    .and_then(|tool| tool.metadata)
                    .and_then(|m| m.risk_level)
                    .map(|level| serde_json::to_string(&level).unwrap_or_default());
                let request = wf_types::interaction::tool_approval::ToolApprovalRequestData {
                    tool_call_id: call.id.clone(),
                    tool_name: tool_name.clone(),
                    tool_description: None,
                    parameters: args.clone(),
                    risk_level: risk_level.map(|s| s.trim_matches('"').to_string()),
                    pending_queue: None,
                    batch_id: None,
                    tool_index: None,
                    total_tools: None,
                    timeout: None,
                    security_preset: None,
                };
                let mut approval_options = approval_options.clone();
                if approval_options.file_permissions.is_none() {
                    approval_options.file_permissions = Some(
                        wf_types::tool::file_permission::FilePermissionSettings::default_rules(),
                    );
                }
                let coordinator =
                    wf_tools::approval::ToolApprovalCoordinator::new(approval_options);
                let batch = coordinator.process_batch(vec![request]);
                if batch.auto_approved.contains(&0) {
                    Ok(None)
                } else {
                    Err(format!(
                        "No approval handler configured. Tool \"{tool_name}\" requires manual approval but no handler is registered."
                    ))
                }
            }
            None => Ok(None),
        }
    };

    let effective_args = match approved {
        Ok(edited) => edited.unwrap_or(args),
        Err(reason) => {
            return tool_result_message(
                &call.id,
                &tool_name,
                format!("Tool \"{tool_name}\" execution rejected: {reason}"),
                true,
            );
        }
    };

    let result = match &ctx.tool_registry {
        Some(registry) => {
            registry
                .execute_tool(&tool_name, &effective_args, &options, &tool_ctx)
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

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl LlmHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
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
        let violation_policy = match config.get("violation_policy") {
            None => None,
            Some(v) => crate::config_parse::parse_node_config_or_warn(
                &ctx.node_id,
                "inner.violation_policy",
                v,
                None,
            ),
        };

        let stream_enabled = config
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_tool_calls = config
            .get("max_tool_calls_per_request")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);

        // Token tracking: the node-level settings consolidate through
        // `LlmExecutionConfig` (typed extraction from the node config JSON);
        // a parse failure degrades to defaults (tracking on, default
        // threshold, no limit) with an explicit warning. The shared execution
        // tracker is updated after every LLM call unless tracking is
        // explicitly disabled.
        let exec_config: wf_types::llm::LlmExecutionConfig =
            crate::config_parse::parse_node_config_or_warn(
                &ctx.node_id,
                "inner (LlmExecutionConfig)",
                config,
                wf_types::llm::LlmExecutionConfig::default(),
            );
        let token_tracking_enabled = exec_config.enable_token_tracking.unwrap_or(true);
        let token_warning_threshold = exec_config
            .token_warning_threshold
            .map(u64::from)
            .unwrap_or(wf_llm::DEFAULT_TOKEN_WARNING_THRESHOLD as u64);
        if token_tracking_enabled {
            if let Some(ref tracker) = ctx.token_tracker {
                // A single lock acquisition covers both the token-limit setup
                // and the checkpoint restore.
                let mut tracker = tracker.lock().await;
                if let Some(token_limit) = exec_config.token_limit.map(u64::from) {
                    if tracker.token_limit() == 0 && token_limit > 0 {
                        tracker.set_token_limit(token_limit);
                    }
                }
                // Checkpoint symmetry: a restored execution picks up the
                // checkpointed guards and accumulations from the variables.
                restore_tracker_from_variables(ctx, &mut tracker);
            }
        }

        let mut messages = build_messages(ctx)?;
        let tools = resolve_tools(ctx)?;

        // Dead-loop detection config: parsed once from the node config,
        // carried on every request in the multi-round tool loop. The gateway
        // detects repeated output and errors out (wf-llm `DeadLoopDetector`).
        let dead_loop_detection: Option<wf_types::llm::DeadLoopDetectionConfig> =
            match config.get("dead_loop_detection") {
                None => None,
                Some(v) => crate::config_parse::parse_node_config_or_warn(
                    &ctx.node_id,
                    "inner.dead_loop_detection",
                    v,
                    None,
                ),
            };

        let mut executed_tool_calls: Vec<Value> = Vec::new();
        let mut final_response: Option<wf_types::llm::LlmResult> = None;
        let mut aggregated_content: Option<String> = None;

        // Multi-round tool loop: keep calling the model while it emits tool
        // calls, feeding tool results back, up to max_tool_calls_per_request.
        // A loop that exhausts the budget while the model keeps emitting tool
        // calls is an error, not a silent truncation.
        let mut tool_loop_exhausted = false;
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
                dead_loop_detection: dead_loop_detection.clone(),
                protocol_auto_converted: None,
            };

            // Pre-request token budget check (2.1.3): the whole-request
            // estimate (and, near the threshold, the provider count-tokens
            // API as a higher-precision estimate) is compared against the
            // limit. Estimation is approximate, so this is a warning only
            // (never blocks the request); the warning carries per-array
            // budget details so listeners can route per-array strategies.
            if token_tracking_enabled {
                if let Some(ref tracker) = ctx.token_tracker {
                    let token_limit = {
                        let tracker = tracker.lock().await;
                        tracker.token_limit()
                    };
                    if token_limit > 0 {
                        let mut estimated = u64::from(wf_llm::estimate_request_tokens(&request));
                        if estimated as f64 > token_limit as f64 * 0.8 {
                            if let Ok(count) = self
                                .gateway
                                .count_tokens(&request, ctx.cancellation.clone())
                                .await
                            {
                                estimated = u64::from(count.input_tokens);
                            }
                        }
                        let mut tracker = tracker.lock().await;
                        if estimated > token_limit && tracker.consume_preflight_warning() {
                            if let Some(ref bus) = ctx.event_bus {
                                let mut event = wf_llm::build_token_usage_warning_event(
                                    &ctx.execution_id,
                                    Some(&ctx.node_id),
                                    estimated,
                                    token_limit,
                                    estimated as f64 / token_limit as f64 * 100.0,
                                );
                                let array_details: Vec<Value> = declared_contexts(config)
                                    .iter()
                                    .map(|id| {
                                        let msgs =
                                            message_context::get_context(&ctx.variables, id);
                                        serde_json::json!({
                                            "context_id": id,
                                            "tokens": message_context::ledger_estimated_tokens(&ctx.variables, id),
                                            "message_count": msgs.len(),
                                        })
                                    })
                                    .collect();
                                if let Some(meta) = event.metadata.as_mut() {
                                    meta.insert(
                                        KEY_ARRAY_DETAILS.to_string(),
                                        Value::Array(array_details),
                                    );
                                }
                                bus.publish_logged(
                                    event,
                                    &format!(
                                        "workflow={} llm={} compression-requested",
                                        ctx.execution_id, ctx.node_id
                                    ),
                                )
                                .ok();
                            }
                        }
                    }
                }
            }

            if stream_enabled {
                let mut stream = match self
                    .gateway
                    .generate_stream(&request, ctx.cancellation.clone())
                    .await
                {
                    Ok(stream) => stream,
                    Err(e) => {
                        if e.is_context_length_exceeded() {
                            publish_forced_compression(ctx, &request).await;
                        }
                        return Err(WorkflowError::Internal(format!("LLM stream failed: {}", e)));
                    }
                };
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
                            if token_tracking_enabled {
                                if let Some(ref tracker) = ctx.token_tracker {
                                    tracker.lock().await.accumulate_stream_usage(
                                        &wf_llm::RequestUsage::from(&u.usage),
                                    );
                                }
                            }
                        }
                        Some(Ok(MessageStreamEvent::FinalMessage(final_msg))) => {
                            let tool_calls = final_msg.message.tool_calls.clone();
                            if let Some(usage) = &final_msg.usage {
                                stream_usage_seen = true;
                                if token_tracking_enabled {
                                    if let Some(ref tracker) = ctx.token_tracker {
                                        tracker.lock().await.accumulate_stream_usage(
                                            &wf_llm::RequestUsage::from(usage),
                                        );
                                    }
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
                        Some(Ok(MessageStreamEvent::Error(err))) => {
                            publish_stream_termination(
                                ctx.event_bus.as_deref(),
                                ctx,
                                &request.profile_id,
                                false,
                                &err.error,
                            );
                            if wf_llm::LlmError::StreamError(err.error.clone())
                                .is_context_length_exceeded()
                            {
                                publish_forced_compression(ctx, &request).await;
                            }
                            return Err(WorkflowError::Internal(format!(
                                "LLM stream error: {}",
                                err.error
                            )));
                        }
                        Some(Ok(MessageStreamEvent::Abort(abort))) => {
                            publish_stream_termination(
                                ctx.event_bus.as_deref(),
                                ctx,
                                &request.profile_id,
                                true,
                                &abort.reason,
                            );
                            return Err(WorkflowError::Internal(format!(
                                "LLM stream aborted: {}",
                                abort.reason
                            )));
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            publish_stream_termination(
                                ctx.event_bus.as_deref(),
                                ctx,
                                &request.profile_id,
                                wf_llm::is_stream_abort(&e),
                                &e.to_string(),
                            );
                            if e.is_context_length_exceeded() {
                                publish_forced_compression(ctx, &request).await;
                            }
                            return Err(WorkflowError::Internal(format!(
                                "LLM stream error: {}",
                                e
                            )));
                        }
                        None => break,
                    }
                }
                if token_tracking_enabled {
                    if let Some(ref tracker) = ctx.token_tracker {
                        let mut tracker = tracker.lock().await;
                        // Decision track: every request is estimated locally,
                        // regardless of provider usage reporting.
                        let completion = content_parts.concat();
                        let prompt_est = wf_llm::estimate_request_tokens(&request);
                        let completion_est = wf_llm::estimate_tokens(&completion) as u32;
                        if !stream_usage_seen {
                            // Cost track fallback: the provider streamed no
                            // usage; the history entry carries the estimated
                            // marker (cost track, never drives decisions).
                            tracker.update_estimated_usage(prompt_est, completion_est);
                        } else {
                            tracker.accumulate_estimated_usage(prompt_est, completion_est);
                        }
                        tracker.finalize_current_request();
                        persist_tracker_state(ctx, &tracker);
                    }
                }
                if token_tracking_enabled {
                    emit_token_usage_events(ctx, token_warning_threshold).await;
                }
                aggregated_content = Some(content_parts.concat());
                tool_loop_exhausted = false;
                break;
            }

            let response = call_llm(ctx, &self.gateway, &request).await?;
            if token_tracking_enabled {
                if let Some(ref tracker) = ctx.token_tracker {
                    let mut tracker = tracker.lock().await;
                    // Dual-track: the decision track always sees the local
                    // estimate of this request; the cost track records the
                    // provider usage (or an estimated marker when absent).
                    let completion = response.content.as_deref().unwrap_or_default();
                    let prompt_est = wf_llm::estimate_request_tokens(&request);
                    let completion_est = wf_llm::estimate_tokens(completion) as u32;
                    if let Some(usage) = &response.usage {
                        tracker.update_api_usage(usage);
                        tracker.accumulate_estimated_usage(prompt_est, completion_est);
                    } else {
                        tracker.update_estimated_usage(prompt_est, completion_est);
                    }
                    tracker.finalize_current_request();
                    persist_tracker_state(ctx, &tracker);
                }
            }
            if token_tracking_enabled {
                emit_token_usage_events(ctx, token_warning_threshold).await;
            }
            let has_tool_calls = response
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty());

            messages.push(response.message.clone());
            final_response = Some(response.clone());
            aggregated_content = response.content.clone();

            if !has_tool_calls {
                tool_loop_exhausted = false;
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
                tool_loop_exhausted = false;
                break;
            }
            // The iteration ended with tool calls executed and no break: if
            // the `max_tool_calls_per_request` budget is now spent, the model
            // still emitting tool calls is an error, not a silent truncation.
            tool_loop_exhausted = true;
        }

        if tool_loop_exhausted {
            return Err(WorkflowError::OperationError(format!(
                "LLM node '{}' exceeded max_tool_calls_per_request ({}) with the model still emitting tool calls",
                ctx.node_id, max_tool_calls
            )));
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
        // (the read context is left untouched so the compression chain can
        // replace it as a unit).
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

    #[tokio::test]
    async fn blocked_tool_is_intercepted_before_execution() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        vars.insert(
            format!("{}{}", wf_agent::BLOCKED_VARIABLE_PREFIX, "shell"),
            serde_json::json!(true),
        );
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        );

        // The blocked tool is not even registered: interception must happen
        // before the registry lookup, so the result is the visibility
        // rejection, not a "not found" error.
        let call = wf_types::message::LlmToolCall {
            id: "call-1".to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: "shell".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let result = execute_tool_call(&ctx, &call).await;
        assert_eq!(result.role, MessageRole::Tool);
        assert_eq!(result.tool_call_id.as_deref(), Some("call-1"));
        let is_error = result
            .metadata
            .as_ref()
            .and_then(|m| m.get("is_error"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(is_error, "blocked call must surface as an error");
        assert!(
            matches!(&result.content, MessageContentValue::Text(t) if t.contains("not visible"))
        );
    }

    #[tokio::test]
    async fn unblocked_tool_executes_but_blocked_same_tool_is_rejected() {
        struct EchoBuiltin;
        #[async_trait]
        impl wf_tools::executor::BuiltinToolHandler for EchoBuiltin {
            fn tool_name(&self) -> &'static str {
                "echo_tool"
            }
            async fn handle(
                &self,
                parameters: &Value,
                _context: &wf_tools::executor::trait_def::ToolExecutionContext,
                _resources: &wf_tools::executor::BuiltinHandlerResources,
            ) -> wf_tools::error::ToolResult<Value> {
                Ok(parameters.clone())
            }
        }

        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let registry = std::sync::Arc::new(wf_tools::registry::ToolRegistry::new());
        registry.register_builtin_handler("echo_tool", std::sync::Arc::new(EchoBuiltin));
        let mut tool = wf_types::tool::Tool {
            id: wf_types::Id::from("echo_tool"),
            name: "echo_tool".to_string(),
            description: "echo".to_string(),
            tool_type: wf_types::tool::ToolType::BuiltIn,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        };
        tool.parameters = Some(wf_types::tool::ToolParameterSchema {
            r#type: "object".into(),
            properties: Default::default(),
            required: Vec::new(),
            additional_properties: Some(true),
        });
        registry.register_tool(tool);

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars.clone(),
        );
        ctx.tool_registry = Some(registry);

        let call = wf_types::message::LlmToolCall {
            id: "call-1".to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: "echo_tool".to_string(),
                arguments: "{\"x\": 1}".to_string(),
            },
        };

        let ok = execute_tool_call(&ctx, &call).await;
        let is_error = ok
            .metadata
            .as_ref()
            .and_then(|m| m.get("is_error"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(!is_error, "unblocked tool must execute");

        vars.insert(
            format!("{}{}", wf_agent::BLOCKED_VARIABLE_PREFIX, "echo_tool"),
            serde_json::json!(true),
        );
        let blocked = execute_tool_call(&ctx, &call).await;
        assert!(
            matches!(&blocked.content, MessageContentValue::Text(t) if t.contains("not visible"))
        );
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
