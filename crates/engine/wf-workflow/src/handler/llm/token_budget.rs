use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;
use wf_llm::LlmGateway;

use super::events::dispatch_compression_signal;
use super::messages::{declared_contexts, injected_messages, read_context_id};
use crate::message_context;

/// Variable-map key carrying the execution-scoped token tracker state so
/// checkpoints restore the guards and accumulations with the execution state.
pub const TRACKER_STATE_KEY: &str = "__token_tracker__";

/// Metadata key of the preflight warning carrying per-array budget details.
pub const KEY_ARRAY_DETAILS: &str = "array_details";

/// Restore the execution-scoped tracker from the variable map once (checkpoint
/// symmetry): a fresh execution tracker (no accumulated state) picks up
/// the checkpointed guards and accumulations together with the variables.
pub fn restore_tracker_from_variables(
    ctx: &NodeExecutionContext,
    tracker: &mut wf_execution_shared::TokenUsageTracker,
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
pub fn persist_tracker_state(
    ctx: &NodeExecutionContext,
    tracker: &wf_execution_shared::TokenUsageTracker,
) {
    if let Ok(value) = serde_json::to_value(tracker.state()) {
        ctx.variables.insert(TRACKER_STATE_KEY.to_string(), value);
    }
}

/// Emit token usage events (actual-first task budget, calibrated compression):
///
/// - warning: task budget (billed cumulative vs task limit),
///   single-shot guard;
/// - limit exceeded: task budget, one emission per 50% tier band
///   (100%, 150%, 200%, ...);
/// - compression requested: per declared named array, driven by the
///   incremental ledger estimate plus this request's dynamic overhead
///   (system prompt, transform injections, inline messages, tool-loop
///   turns, tool declarations — everything assembled into the request
///   beyond the read array, computed fresh and never accumulated into
///   the ledger), calibrated by the actual-minus-estimated bias, compared
///   against the model-window context budget, guarded by the array
///   version (single-shot per version, checkpointed in the ledger).
///
/// Nested compression runs (see [`crate::message_context::compression_depth`])
/// never emit: the summary sub-workflow summarizes an already over-budget
/// snapshot and must not recurse into its own compression chain.
pub async fn emit_token_usage_events(
    ctx: &NodeExecutionContext,
    warning_threshold: u64,
    request: &wf_types::llm::LlmRequest,
) {
    let (Some(ref tracker), Some(ref bus)) = (&ctx.token_tracker, &ctx.event_bus) else {
        return;
    };
    let mut tracker = tracker.lock().await;
    let token_limit = tracker.token_limit();
    let context_limit = tracker.context_limit();
    if token_limit == 0 && context_limit == 0 {
        return;
    }
    let tokens_used = tracker.billed_total();
    if token_limit > 0 && tracker.consume_warning(warning_threshold as f64) {
        let percentage = tracker.billed_usage_percentage().unwrap_or(0.0);
        bus.publish_logged(
            wf_execution_shared::build_token_usage_warning_event(
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
    if token_limit > 0 && tracker.consume_limit_exceeded_tier().is_some() {
        bus.publish_logged(
            wf_execution_shared::build_token_limit_exceeded_event(
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
    if context_limit == 0 {
        return;
    }
    if crate::message_context::compression_depth(&ctx.variables) > 0 {
        return;
    }

    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let injected_count = injected_messages(config).len();
    // Per-request dynamic overhead beyond the read array: system prompt,
    // transform injections, inline messages, tool-loop turns and tool
    // declarations. Derived from the assembled request (the same text the
    // provider receives) so none of it ever accumulates into the array
    // ledger, mirroring the agent-side request-minus-view subtraction.
    let dynamic = wf_execution_shared::context_store::dynamic_request_overhead(
        u64::from(wf_llm::estimate_request_tokens(request)),
        u64::from(wf_llm::estimate_messages(&message_context::get_context(
            &ctx.variables,
            read_context_id(config),
        ))),
    );
    for context_id in declared_contexts(config) {
        let context_messages = message_context::get_context(&ctx.variables, &context_id);
        if context_messages.is_empty() {
            // No named array to compress: nothing to write back to.
            continue;
        }
        // Array budget = ledger estimate (recomputed lazily after
        // replacements) + this request's dynamic overhead, calibrated
        // by the actual-minus-estimated bias.
        let stable =
            message_context::ledger_estimated_tokens(&ctx.variables, &context_id) + dynamic;
        let estimated = tracker.calibrated(stable);
        let version = message_context::array_version(&ctx.variables, &context_id);
        if wf_execution_shared::context_store::over_budget(estimated, context_limit)
            && message_context::should_emit_compression(&ctx.variables, &context_id, version)
        {
            let compression_request = wf_execution_shared::context_store::compression_request(
                &context_id,
                estimated,
                context_limit,
                context_messages.len(),
                version,
                false,
                &context_messages,
            );
            let mut event = wf_execution_shared::context_store::compression_event(
                &ctx.execution_id.to_string(),
                None,
                &compression_request,
            );
            if injected_count > 0 {
                if let Some(meta) = event.metadata.as_mut() {
                    meta.insert(
                        wf_execution_shared::KEY_INJECTED_MESSAGE_COUNT.to_string(),
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
            dispatch_compression_signal(ctx, &compression_request).await;
            message_context::mark_compression_emitted(&ctx.variables, &context_id, version);
            // Backpressure anchor: the next node waits for this version to
            // settle before re-sending the over-budget array. Anchored only
            // when a hook receiver can take over.
            if ctx.hook_handler_registry.is_some() {
                tracker.begin_compression_flight(&context_id, version, false);
            }
            persist_tracker_state(ctx, &tracker);
        }
    }
}

/// Blocking backpressure gate for workflow LLM nodes: when any declared
/// array carries an in-flight compression anchored at its current version,
/// wait without timeout for the write-back to land before assembling the
/// request. A compression failure stops the node for manual handling;
/// cancellation aborts the wait.
pub async fn await_compression_settle(
    ctx: &NodeExecutionContext,
) -> crate::error::WorkflowResult<()> {
    let Some(ref tracker) = ctx.token_tracker else {
        return Ok(());
    };
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let targets = declared_contexts(config);
    if targets.is_empty() {
        return Ok(());
    }
    let anchored: Vec<(String, u64)> = {
        let tracker = tracker.lock().await;
        targets
            .into_iter()
            .filter_map(|id| {
                let version = message_context::array_version(&ctx.variables, &id);
                let in_flight = tracker
                    .compression_flight(&id)
                    .is_some_and(|flight| flight.version == version);
                in_flight.then_some((id, version))
            })
            .collect()
    };
    if anchored.is_empty() {
        return Ok(());
    }
    let execution_id = ctx.execution_id.to_string();
    let mut failure_events = ctx
        .event_bus
        .as_ref()
        .map(|bus| bus.subscribe_typed(wf_types::events::EventType::ContextCompressionFailed));
    if let Some(ref bus) = ctx.event_bus {
        for event in bus.recent_events() {
            if let Some(err) = matching_compression_failure(&event, &execution_id, None) {
                for (target, version) in &anchored {
                    if err.0 == *target && err.1 == *version {
                        return Err(crate::error::WorkflowError::TriggerError(err.2.clone()));
                    }
                }
            }
        }
    }
    for (target, version) in anchored {
        loop {
            if message_context::array_version(&ctx.variables, &target) != version {
                break;
            }
            if let Some(ref mut sub) = failure_events {
                while let Ok(event) = sub.try_recv() {
                    if let Some((failed_target, failed_version, message)) =
                        matching_compression_failure(&event, &execution_id, None)
                    {
                        if failed_target == target && failed_version == version {
                            return Err(crate::error::WorkflowError::TriggerError(message));
                        }
                    }
                }
            }
            let wait = tokio::time::sleep(std::time::Duration::from_millis(
                wf_execution_shared::COMPRESSION_SETTLE_POLL_MS,
            ));
            match ctx.cancellation.clone() {
                Some(token) => {
                    tokio::select! {
                        _ = wait => {}
                        _ = token.cancelled() => {
                            return Err(crate::error::WorkflowError::OperationError(
                                "aborted while waiting for compression".to_string(),
                            ));
                        }
                    }
                }
                None => wait.await,
            }
        }
    }
    Ok(())
}

/// Match a compression failure event against an emitting execution.
/// Returns the target array, anchor version and manual-handling message.
fn matching_compression_failure(
    event: &wf_types::events::BaseEvent,
    execution_id: &str,
    _agent_loop_id: Option<&str>,
) -> Option<(String, u64, String)> {
    if event.r#type != wf_types::events::EventType::ContextCompressionFailed {
        return None;
    }
    if event.execution_id.as_deref() != Some(execution_id) {
        return None;
    }
    let meta = wf_execution_shared::ContextCompressionFailedMeta::try_from(event).ok()?;
    let message = format!(
        "context compression failed for '{}' at version {}: {}; manual handling required",
        meta.target_context_id, meta.array_version, meta.error
    );
    Some((meta.target_context_id, meta.array_version, message))
}

/// Initialize the execution-scoped tracker limits from the node config
/// (task budget) and the model window (context budget), then restore
/// checkpointed guards. Single lock acquisition covers setup and restore.
pub async fn setup_token_tracker(
    ctx: &NodeExecutionContext,
    exec_config: &wf_types::llm::LlmExecutionConfig,
    enabled: bool,
    context_budget: u64,
) {
    if !enabled {
        return;
    }
    if let Some(ref tracker) = ctx.token_tracker {
        let mut tracker = tracker.lock().await;
        if let Some(token_limit) = exec_config.token_limit.map(u64::from) {
            if tracker.token_limit() == 0 && token_limit > 0 {
                tracker.set_token_limit(token_limit);
            }
        }
        if tracker.context_limit() == 0 && context_budget > 0 {
            tracker.set_context_limit(context_budget);
        }
        restore_tracker_from_variables(ctx, &mut tracker);
    }
}

/// Pre-request context budget check: the whole-request estimate (and, near
/// the threshold, the provider count-tokens API as a higher-precision
/// estimate) is compared against the model-window context budget.
/// Estimation is approximate, so this is a warning only (never blocks the
/// request); the warning carries per-array budget details so listeners can
/// route per-array strategies.
pub async fn check_preflight_budget(
    ctx: &NodeExecutionContext,
    gateway: &LlmGateway,
    request: &wf_types::llm::LlmRequest,
    enabled: bool,
) {
    if !enabled {
        return;
    }
    // Nested compression runs stay silent (chicken-and-egg guard).
    if message_context::compression_depth(&ctx.variables) > 0 {
        return;
    }
    let Some(ref tracker) = ctx.token_tracker else {
        return;
    };
    let context_limit = {
        let tracker = tracker.lock().await;
        tracker.context_limit()
    };
    if context_limit == 0 {
        return;
    }
    let mut estimated = u64::from(wf_llm::estimate_request_tokens(request));
    if estimated as f64 > context_limit as f64 * 0.8 {
        if let Ok(count) = gateway
            .count_tokens(request, ctx.cancellation.clone())
            .await
        {
            estimated = u64::from(count.input_tokens);
        }
    }
    let mut tracker = tracker.lock().await;
    if estimated > context_limit && tracker.consume_preflight_warning() {
        if let Some(ref bus) = ctx.event_bus {
            let mut event = wf_execution_shared::build_token_usage_warning_event(
                &ctx.execution_id,
                Some(&ctx.node_id),
                estimated,
                context_limit,
                estimated as f64 / context_limit as f64 * 100.0,
            );
            let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
            let array_details: Vec<Value> = declared_contexts(config)
                .iter()
                .map(|id| {
                    let msgs = message_context::get_context(&ctx.variables, id);
                    serde_json::json!({
                        "context_id": id,
                        "tokens": message_context::ledger_estimated_tokens(&ctx.variables, id),
                        "message_count": msgs.len(),
                    })
                })
                .collect();
            if let Some(meta) = event.metadata.as_mut() {
                meta.insert(KEY_ARRAY_DETAILS.to_string(), Value::Array(array_details));
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

/// Record the non-streaming request outcome on the decision track (local
/// estimate always) and the cost track (provider usage or estimated marker),
/// then persist the tracker state for checkpoint symmetry.
pub async fn record_non_stream_usage(
    ctx: &NodeExecutionContext,
    request: &wf_types::llm::LlmRequest,
    response: &wf_types::llm::LlmResult,
    enabled: bool,
) {
    if !enabled {
        return;
    }
    if let Some(ref tracker) = ctx.token_tracker {
        let mut tracker = tracker.lock().await;
        let completion = response.content.as_deref().unwrap_or_default();
        let prompt_est = wf_llm::estimate_request_tokens(request);
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wf_core::EventBus;
    use wf_types::events::EventType;
    use wf_types::message::{Message, MessageRole};
    use wf_types::node::StaticNodeType;

    use super::super::messages::text_message;
    use super::*;

    fn bare_request(messages: Vec<Message>) -> wf_types::llm::LlmRequest {
        wf_types::llm::LlmRequest {
            profile_id: "mock".to_string(),
            messages,
            parameters: None,
            generation: None,
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        }
    }

    /// The system prompt and inline messages assembled into the request are
    /// part of the context budget even though they never enter the array
    /// ledger: a bare array under the limit must still trigger compression
    /// once the request-level overhead crosses the threshold.
    #[tokio::test]
    async fn emission_counts_system_prompt_and_inline_messages() {
        let bus = Arc::new(EventBus::new(16));
        let mut sub = bus.subscribe();
        let vars = Arc::new(dashmap::DashMap::new());

        let history = vec![
            text_message(
                MessageRole::User,
                "what did we decide earlier about the storage layout".to_string(),
            ),
            text_message(
                MessageRole::Assistant,
                "we settled on a single append-only ledger per named array".to_string(),
            ),
        ];
        message_context::append_context(&vars, "chat", history.clone());

        let system: String = "You are a precise assistant. ".repeat(8);
        let inline_text =
            "before answering, restate the constraints in full detail so nothing is forgotten";
        let mut request = bare_request(vec![]);
        request
            .messages
            .push(text_message(MessageRole::System, system.clone()));
        request.messages.extend(history.clone());
        request
            .messages
            .push(text_message(MessageRole::User, inline_text.to_string()));

        let array_estimate = u64::from(wf_llm::estimate_messages(&history));
        let request_estimate = u64::from(wf_llm::estimate_request_tokens(&request));
        assert!(
            request_estimate > array_estimate,
            "system + inline must add overhead: array={array_estimate} request={request_estimate}"
        );
        // Budget sits between the bare array and the full request: only the
        // request-inclusive estimate can cross it.
        let context_limit = array_estimate + (request_estimate - array_estimate) / 2;

        let mut ctx = NodeExecutionContext::new(
            "exec-budget".to_string(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "profile_id": "mock",
            "context_id": "chat",
            "system_prompt": system,
            "messages": [{
                "role": "user",
                "content": inline_text,
                "id": "inline-1",
                "timestamp": 1
            }],
        }));
        ctx.event_bus = Some(bus.clone());
        let tracker = Arc::new(tokio::sync::Mutex::new(
            wf_execution_shared::TokenUsageTracker::new(0),
        ));
        tracker.lock().await.set_context_limit(context_limit);
        ctx.token_tracker = Some(tracker);

        emit_token_usage_events(&ctx, 85, &request).await;

        let event = sub
            .try_recv()
            .expect("system prompt and inline messages must count toward the budget");
        assert_eq!(event.r#type, EventType::ContextCompressionRequested);
        let tokens_used = event
            .metadata
            .as_ref()
            .and_then(|m| m.get(wf_execution_shared::KEY_TOKENS_USED))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert_eq!(
            tokens_used, request_estimate,
            "emission must report the full request estimate, not the bare array"
        );
        assert!(tokens_used > array_estimate);
    }
}
