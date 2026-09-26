use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_types::llm::LlmRequest;

use super::{AgentIterationCoordinator, IterationResult};
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};

impl AgentIterationCoordinator {
    /// Wait for the in-flight compression write-back to land for the
    /// current conversation version. A terminal failure (published FAILED
    /// event, anchor released without a version advance, or the settle
    /// budget elapsing) must not fail the run and must not degrade it
    /// silently: the loop pauses into an externally perceivable state so
    /// the handling can be decided from outside. Returns `Some` when the
    /// iteration must end early on that pause.
    pub(super) async fn settle_compression_flight(
        &self,
        entity: &AgentLoopEntity,
    ) -> AgentResult<Option<IterationResult>> {
        let (version, in_flight) = {
            let conversation = entity.conversation().read().await;
            let version = conversation.conversation_version();
            let in_flight = conversation
                .compression_flight()
                .is_some_and(|flight| flight.version == version);
            (version, in_flight)
        };
        if !in_flight {
            return Ok(None);
        }
        let execution_id = entity.id().to_string();
        let mut failure_events = self
            .event_bus
            .as_ref()
            .map(|bus| bus.subscribe_typed(wf_types::events::EventType::ContextCompressionFailed));
        if let Some(ref bus) = self.event_bus {
            for event in bus.recent_events() {
                if let Some(message) = matching_compression_failure(&event, &execution_id, version)
                {
                    return Ok(self
                        .compression_failure_park(entity, version, &message)
                        .await);
                }
            }
        }
        let settle_start = std::time::Instant::now();
        let settle_timeout_ms = wf_execution_shared::compression_settle_timeout_ms();
        loop {
            {
                let conversation = entity.conversation().read().await;
                if conversation.conversation_version() != version {
                    break;
                }
                let flight_gone = !conversation
                    .compression_flight()
                    .is_some_and(|flight| flight.version == version);
                if flight_gone {
                    return Ok(self
                        .compression_failure_park(
                            entity,
                            version,
                            "compression anchor released without version advance",
                        )
                        .await);
                }
            }
            if settle_start.elapsed().as_millis() as u64 >= settle_timeout_ms {
                return Ok(self
                    .compression_failure_park(entity, version, "compression settle budget exceeded")
                    .await);
            }
            if let Some(ref mut sub) = failure_events {
                while let Ok(event) = sub.try_recv() {
                    if let Some(message) =
                        matching_compression_failure(&event, &execution_id, version)
                    {
                        return Ok(self
                            .compression_failure_park(entity, version, &message)
                            .await);
                    }
                }
            }
            let wait = tokio::time::sleep(std::time::Duration::from_millis(
                wf_execution_shared::COMPRESSION_SETTLE_POLL_MS,
            ));
            let abort = entity.get_abort_signal();
            tokio::select! {
                _ = wait => {}
                _ = abort.cancelled() => {
                    return Err(AgentError::LlmError(wf_llm::error::LlmError::Cancelled));
                }
            }
        }
        Ok(None)
    }

    /// Park the loop after a terminal compression failure: the pause makes
    /// the state externally perceivable (resume continues the next
    /// iteration without compression, stop exits, pause-timeout policies
    /// apply). If a resume raced the pause the caller simply proceeds
    /// without compression.
    pub(super) async fn compression_failure_park(
        &self,
        entity: &AgentLoopEntity,
        version: u64,
        reason: &str,
    ) -> Option<IterationResult> {
        tracing::warn!(
            "context compression failed at version {version}: {reason}; \
             pausing the loop for external handling"
        );
        let _ = entity.interruption().pause();
        self.interrupted(entity, 0).await
    }

    /// Safety-net path: emit a forced CONTEXT_COMPRESSION_REQUESTED over
    /// the actual request messages when the provider rejected them with a
    /// context-length-exceeded error. Skipped when a regular compression
    /// signal for the same version already fired (the summary is in flight).
    pub(super) async fn publish_forced_compression(
        &self,
        entity: &AgentLoopEntity,
        request: &LlmRequest,
    ) {
        let Some(ref bus) = self.event_bus else {
            return;
        };
        let (version, context_limit, may_emit) = {
            let conversation = entity.conversation().read().await;
            let version = conversation.conversation_version();
            let may_emit = conversation.should_emit_compression(version);
            (version, conversation.context_limit(), may_emit)
        };
        if !may_emit {
            return;
        }
        let tokens_used = u64::from(wf_llm::estimate_request_tokens(request));
        // With no model window the budget is unknown: report a zero limit
        // with the unknown-budget marker instead of a fabricated ratio.
        let effective_limit = if context_limit > 0 {
            context_limit
        } else {
            tracing::warn!(
                entity_id = %entity.id(),
                "forced compression with unknown context budget"
            );
            0
        };
        let messages = request.messages.clone();
        let compression_request = wf_execution_shared::context_store::compression_request(
            wf_execution_shared::CONVERSATION_CONTEXT_ID,
            tokens_used,
            effective_limit,
            request.messages.len(),
            version,
            true,
            &messages,
        );
        // Snapshot the pre-compression state before the safety-net summary
        // workflow runs; the compressed view is checkpointed on write-back.
        self.boundary_checkpoint(
            entity,
            wf_types::checkpoint::CheckpointTiming::BeforeCompression,
        )
        .await;
        let _ = bus.publish(wf_execution_shared::context_store::compression_event(
            &entity.id().to_string(),
            Some(entity.id()),
            &compression_request,
        ));
        let dispatched = wf_execution_shared::context_store::dispatch_compression_signal(
            self.hook_handler_registry.as_deref(),
            self.event_bus.as_deref(),
            entity.id(),
            Some(entity.id()),
            entity.get_abort_signal(),
            &compression_request,
        )
        .await;
        if !dispatched {
            tracing::warn!(
                entity_id = %entity.id(),
                version = version,
                "forced compression signal has no taker; audit event kept, flight not anchored"
            );
        }
        let mut session = entity.conversation().write().await;
        session.mark_compression_emitted(version);
        if dispatched {
            session.begin_compression_flight(version, true);
        }
    }

    /// Record token usage into the conversation session after an LLM call
    /// (v2 dual-track semantics) and emit the derived token / compression
    /// events: task warnings, task limit, and the context-budget
    /// compression trigger with its signal dispatch. Skipped entirely when
    /// token tracking is disabled by config.
    pub(super) async fn record_usage_and_compression(
        &self,
        entity: &AgentLoopEntity,
        request: &LlmRequest,
        assistant_msg: &wf_types::message::Message,
        request_usage: &Option<wf_execution_shared::RequestUsage>,
    ) {
        if !self.token_tracking_enabled {
            return;
        }
        let execution_id = entity.id().clone();
        let prompt_est = wf_llm::estimate_request_tokens(request);
        let completion_est =
            wf_llm::estimate_tokens(&super::llm_call::text_of(&assistant_msg.content)) as u32;
        let mut conversation = entity.conversation().write().await;
        if let Some(usage) = request_usage {
            conversation.accumulate_stream_usage(usage);
            conversation.accumulate_estimated_usage(prompt_est, completion_est);
        } else {
            conversation.update_estimated_usage(prompt_est, completion_est);
        }
        conversation.finalize_current_request();

        // Emit token usage events: task warnings compare the billed
        // cumulative (actual-first with estimation fallback) against the
        // task token limit, while compression compares the projected
        // view plus fresh per-request dynamic overhead (tool
        // declarations, injected blocks), calibrated by the
        // actual-minus-estimated bias, against the model-window context
        // budget. The view estimate shrinks after compression even
        // though the history keeps growing, so history-based estimates
        // would re-trigger compression immediately.
        let Some(ref bus) = self.event_bus else {
            return;
        };
        let tokens_used = conversation.billed_total();
        let token_limit = conversation.token_limit();
        if token_limit > 0 {
            if conversation.consume_token_warning(self.token_warning_threshold as f64) {
                let percentage = conversation.usage_percentage().unwrap_or(0.0);
                let _ = bus.publish(wf_execution_shared::build_token_usage_warning_event(
                    &execution_id,
                    Some(entity.id()),
                    tokens_used,
                    token_limit,
                    percentage,
                ));
            }
            if conversation.consume_limit_exceeded_tier().is_some() {
                let _ = bus.publish(wf_execution_shared::build_token_limit_exceeded_event(
                    &execution_id,
                    Some(entity.id()),
                    tokens_used,
                    token_limit,
                ));
            }
        }
        let context_limit = conversation.context_limit();
        if context_limit == 0 {
            return;
        }
        let stable = conversation.estimated_view_tokens();
        let dynamic =
            wf_execution_shared::context_store::dynamic_request_overhead(prompt_est as u64, stable);
        let estimated = conversation.calibrated_estimate(stable.saturating_add(dynamic));
        let version = conversation.conversation_version();
        if wf_execution_shared::context_store::over_budget(estimated, context_limit)
            && conversation.should_emit_compression(version)
        {
            // Budget, count and snapshot share the active view
            // so the summary input matches the controlled size.
            let messages = conversation.view_messages();
            let compression_request = wf_execution_shared::context_store::compression_request(
                wf_execution_shared::CONVERSATION_CONTEXT_ID,
                estimated,
                context_limit,
                messages.len(),
                version,
                false,
                &messages,
            );
            // Release the session lock: the pre-compression
            // checkpoint below reads the session back.
            drop(conversation);
            // Snapshot the pre-compression state; the compressed
            // view lands in a post-compression checkpoint when the
            // summary workflow writes back.
            self.boundary_checkpoint(
                entity,
                wf_types::checkpoint::CheckpointTiming::BeforeCompression,
            )
            .await;
            // The event-bus copy stays the audit / persistence /
            // user-rule channel; delivery is the synchronous hook
            // dispatch (the compression service takes over here).
            let _ = bus.publish(wf_execution_shared::context_store::compression_event(
                &execution_id,
                Some(entity.id()),
                &compression_request,
            ));
            let dispatched = wf_execution_shared::context_store::dispatch_compression_signal(
                self.hook_handler_registry.as_deref(),
                self.event_bus.as_deref(),
                entity.id(),
                Some(entity.id()),
                entity.get_abort_signal(),
                &compression_request,
            )
            .await;
            if !dispatched {
                tracing::warn!(
                    entity_id = %entity.id(),
                    version = version,
                    "compression signal has no taker; audit event kept, flight not anchored"
                );
            }
            let mut session = entity.conversation().write().await;
            session.mark_compression_emitted(version);
            // Backpressure only anchors when the signal was
            // actually dispatched; without a taker nothing
            // settles the flight and later iterations would wait
            // in vain.
            if dispatched {
                session.begin_compression_flight(version, false);
            }
        }
    }
}

/// Match a compression failure event against an agent emission.
/// Returns the manual-handling message when the event targets the same
/// execution and anchor version.
fn matching_compression_failure(
    event: &wf_types::events::BaseEvent,
    execution_id: &str,
    version: u64,
) -> Option<String> {
    if event.r#type != wf_types::events::EventType::ContextCompressionFailed {
        return None;
    }
    if event.execution_id.as_deref() != Some(execution_id) {
        return None;
    }
    let meta = wf_execution_shared::ContextCompressionFailedMeta::try_from(event).ok()?;
    if meta.array_version != version {
        return None;
    }
    Some(format!(
        "context compression failed for '{}' at version {}: {}; the execution pauses for external handling",
        meta.target_context_id, meta.array_version, meta.error
    ))
}
