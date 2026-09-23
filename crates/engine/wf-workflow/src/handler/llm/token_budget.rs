use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;
use wf_llm::LlmGateway;

use super::events::dispatch_compression_signal;
use super::messages::{declared_contexts, injected_messages};
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
///   incremental ledger estimate + transform-context injections + tool
///   declarations, calibrated by the actual-minus-estimated bias, compared
///   against the model-window context budget, guarded by the array version
///   (single-shot per version, checkpointed in the ledger).
///
/// Nested compression runs (see [`crate::message_context::compression_depth`])
/// never emit: the summary sub-workflow summarizes an already over-budget
/// snapshot and must not recurse into its own compression chain.
pub async fn emit_token_usage_events(
    ctx: &NodeExecutionContext,
    warning_threshold: u64,
    tools: Option<&[wf_types::tool::Tool]>,
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
    let injected = injected_messages(config);
    let injected_estimate = u64::from(wf_llm::estimate_messages(&injected));
    let injected_count = injected.len();
    // Tool declarations are per-request dynamic overhead: estimated fresh
    // here, never accumulated into the array ledger.
    let tools_estimate = u64::from(wf_llm::estimate_tool_declarations(tools));
    for context_id in declared_contexts(config) {
        let context_messages = message_context::get_context(&ctx.variables, &context_id);
        if context_messages.is_empty() {
            // No named array to compress: nothing to write back to.
            continue;
        }
        // Array budget = ledger estimate (recomputed lazily after
        // replacements) + this request's injected messages + tool
        // declarations, calibrated by the actual-minus-estimated bias.
        let stable = message_context::ledger_estimated_tokens(&ctx.variables, &context_id)
            + injected_estimate
            + tools_estimate;
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

/// Cooperative backpressure gate for workflow LLM nodes: when any declared
/// array carries an in-flight compression anchored at its current version,
/// wait for the write-back to land (bounded, cancellation-aware) before
/// assembling the request. A timeout drops the stale anchor; the emission
/// guard stays, so the same version never re-emits in a loop.
pub async fn await_compression_settle(ctx: &NodeExecutionContext) {
    let Some(ref tracker) = ctx.token_tracker else {
        return;
    };
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let targets = declared_contexts(config);
    if targets.is_empty() {
        return;
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
        return;
    }
    let variables = ctx.variables.clone();
    let cancel = ctx.cancellation.clone();
    for (target, version) in anchored {
        let vars = variables.clone();
        let probe = target.clone();
        let settled = async {
            wf_execution_shared::context_store::wait_for_version_shift(
                move || {
                    let vars = vars.clone();
                    let probe = probe.clone();
                    async move { message_context::array_version(&vars, &probe) }
                },
                version,
                wf_execution_shared::COMPRESSION_SETTLE_WAIT_MS,
            )
            .await
        };
        let ok = match cancel.clone() {
            Some(token) => tokio::select! {
                settled = settled => settled,
                _ = token.cancelled() => false,
            },
            None => settled.await,
        };
        if !ok {
            let mut tracker = tracker.lock().await;
            tracker.end_compression_flight(&target, version);
            persist_tracker_state(ctx, &tracker);
        }
    }
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
