use std::collections::HashMap;

use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::events::{BaseEvent, EventType};
use wf_types::llm::LlmRequest;

use super::messages::declared_contexts;
use crate::message_context;

pub fn emit_llm_event(
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
pub fn publish_stream_termination(
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
            wf_execution_shared::build_llm_stream_aborted_event(
                &ctx.execution_id,
                None,
                message,
                profile_id,
            ),
            &format!(
                "workflow={} llm={} stream-aborted",
                ctx.execution_id, ctx.node_id
            ),
        )
        .ok();
    } else {
        bus.publish_logged(
            wf_execution_shared::build_llm_stream_error_event(
                &ctx.execution_id,
                None,
                message,
                profile_id,
            ),
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
pub async fn publish_forced_compression(ctx: &NodeExecutionContext, request: &LlmRequest) {
    let Some(ref bus) = ctx.event_bus else {
        return;
    };
    // Nested compression runs never re-emit (chicken-and-egg guard).
    if message_context::compression_depth(&ctx.variables) > 0 {
        return;
    }
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let target = declared_contexts(config)
        .first()
        .cloned()
        .unwrap_or_else(|| message_context::DEFAULT_CONTEXT_ID.to_string());
    let tokens_used = u64::from(wf_llm::estimate_request_tokens(request));
    let message_count = request.messages.len();
    let array_version = message_context::array_version(&ctx.variables, &target);
    // Dedup: a regular compression signal for this version already fired.
    if !message_context::should_emit_compression(&ctx.variables, &target, array_version) {
        return;
    }
    let context_limit = if let Some(ref tracker) = ctx.token_tracker {
        tracker.lock().await.context_limit()
    } else {
        0
    };
    // With no model window the budget is unknown: report a zero limit with
    // the unknown-budget marker instead of a fabricated ratio.
    let effective_limit = if context_limit > 0 {
        context_limit
    } else {
        tracing::warn!(
            execution_id = %ctx.execution_id,
            node_id = %ctx.node_id,
            "forced compression with unknown context budget"
        );
        0
    };
    let compression_request = wf_execution_shared::context_store::compression_request(
        &target,
        tokens_used,
        effective_limit,
        message_count,
        array_version,
        true,
        &request.messages,
    );
    bus.publish_logged(
        wf_execution_shared::context_store::compression_event(
            &ctx.execution_id.to_string(),
            None,
            &compression_request,
        ),
        &format!(
            "workflow={} llm={} forced-compression",
            ctx.execution_id, ctx.node_id
        ),
    )
    .ok();
    let dispatched = dispatch_compression_signal(ctx, &compression_request).await;
    if !dispatched {
        tracing::warn!(
            execution_id = %ctx.execution_id,
            node_id = %ctx.node_id,
            target = %target,
            "forced compression signal has no taker; audit event kept, flight not anchored"
        );
    }
    message_context::mark_compression_emitted(&ctx.variables, &target, array_version);
    if let Some(ref tracker) = ctx.token_tracker {
        let mut tracker = tracker.lock().await;
        if dispatched {
            tracker.begin_compression_flight(&target, array_version, true);
        }
        super::token_budget::persist_tracker_state(ctx, &tracker);
    }
}

/// Dispatch the `CONTEXT_COMPRESSION_REQUESTED` engine signal through the
/// shared context store: registered receivers (the compression service)
/// are notified synchronously so the summary sub-workflow takes over
/// immediately. Workflow targets have no `agent_loop_id`: the write-back
/// goes through the execution registry. Returns false when no registry can
/// take over (audit event is kept, backpressure must not anchor).
pub async fn dispatch_compression_signal(
    ctx: &NodeExecutionContext,
    request: &wf_execution_shared::ContextCompressionRequest<'_>,
) -> bool {
    wf_execution_shared::context_store::dispatch_compression_signal(
        ctx.hook_handler_registry.as_deref(),
        ctx.event_bus.as_deref(),
        &ctx.execution_id,
        None,
        request,
    )
    .await
}
