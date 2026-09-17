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

/// Emit token usage events (v2 dual-track semantics):
///
/// - warning: decision track (estimated cumulative), single-shot guard;
/// - limit exceeded: decision track, one emission per 50% tier band
///   (100%, 150%, 200%, ...);
/// - compression requested: per declared named array, driven by the
///   incremental ledger estimate + transform-context injections, guarded by
///   the array version (single-shot per version, checkpointed in the ledger).
///
/// No provider usage participates in any of these decisions.
pub async fn emit_token_usage_events(ctx: &NodeExecutionContext, warning_threshold: u64) {
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
    if tracker.consume_limit_exceeded_tier().is_some() {
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
        if wf_execution_shared::context_store::over_budget(estimated, token_limit)
            && message_context::should_emit_compression(&ctx.variables, &context_id, version)
        {
            let compression_request = wf_execution_shared::context_store::compression_request(
                &context_id,
                estimated,
                token_limit,
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
        }
    }
}

/// Initialize the execution-scoped tracker limit from the node config and
/// restore checkpointed guards. Single lock acquisition covers both setup
/// and restore.
pub async fn setup_token_tracker(
    ctx: &NodeExecutionContext,
    exec_config: &wf_types::llm::LlmExecutionConfig,
    enabled: bool,
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
        restore_tracker_from_variables(ctx, &mut tracker);
    }
}

/// Pre-request token budget check: the whole-request estimate (and, near the
/// threshold, the provider count-tokens API as a higher-precision estimate)
/// is compared against the limit. Estimation is approximate, so this is a
/// warning only (never blocks the request); the warning carries per-array
/// budget details so listeners can route per-array strategies.
pub async fn check_preflight_budget(
    ctx: &NodeExecutionContext,
    gateway: &LlmGateway,
    request: &wf_types::llm::LlmRequest,
    enabled: bool,
) {
    if !enabled {
        return;
    }
    let Some(ref tracker) = ctx.token_tracker else {
        return;
    };
    let token_limit = {
        let tracker = tracker.lock().await;
        tracker.token_limit()
    };
    if token_limit == 0 {
        return;
    }
    let mut estimated = u64::from(wf_llm::estimate_request_tokens(request));
    if estimated as f64 > token_limit as f64 * 0.8 {
        if let Ok(count) = gateway
            .count_tokens(request, ctx.cancellation.clone())
            .await
        {
            estimated = u64::from(count.input_tokens);
        }
    }
    let mut tracker = tracker.lock().await;
    if estimated > token_limit && tracker.consume_preflight_warning() {
        if let Some(ref bus) = ctx.event_bus {
            let mut event = wf_execution_shared::build_token_usage_warning_event(
                &ctx.execution_id,
                Some(&ctx.node_id),
                estimated,
                token_limit,
                estimated as f64 / token_limit as f64 * 100.0,
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
