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
                .is_some_and(|flight| wf_execution_shared::check_anchor(flight.version, version));
            (version, in_flight)
        };
        if !in_flight {
            return Ok(None);
        }
        let execution_id = entity.id().to_string();
        let conversation_handle = entity.conversation().clone();
        let bus_handle = self.event_bus.clone();
        let abort = entity.get_abort_signal();
        let outcome = wf_execution_shared::context_store::settle_compression_anchor(
            version,
            self.effective_settle_timeout_ms(),
            Some(abort.clone()),
            {
                let handle = conversation_handle.clone();
                move || {
                    let handle = handle.clone();
                    async move { handle.read().await.conversation_version() }
                }
            },
            {
                let handle = conversation_handle.clone();
                move || {
                    let handle = handle.clone();
                    async move {
                        !handle
                            .read()
                            .await
                            .compression_flight()
                            .is_some_and(|flight| {
                                wf_execution_shared::check_anchor(flight.version, version)
                            })
                    }
                }
            },
            {
                let bus = bus_handle.clone();
                let execution_id = execution_id.clone();
                move || {
                    let bus = bus.clone();
                    let execution_id = execution_id.clone();
                    async move {
                        bus.as_ref().and_then(|bus| {
                            wf_execution_shared::context_store::find_compression_failure(
                                &bus.recent_events(),
                                &execution_id,
                                version,
                                None,
                            )
                        })
                    }
                }
            },
        )
        .await;
        // Single parking rule shared with the workflow waiter: version
        // advance continues, cancellation propagates, every other terminal
        // parks with the same message shape (agent keeps its pause product
        // form, workflow keeps its classified-failure form).
        if let Some(message) = wf_execution_shared::settle_parking_message(
            wf_execution_shared::CONVERSATION_CONTEXT_ID,
            version,
            &outcome,
        ) {
            return Ok(self
                .compression_failure_park(entity, version, &message)
                .await);
        }
        match outcome {
            wf_execution_shared::context_store::CompressionSettleOutcome::VersionAdvanced => {
                Ok(None)
            }
            wf_execution_shared::context_store::CompressionSettleOutcome::Cancelled => {
                Err(AgentError::LlmError(wf_llm::LlmError::Cancelled))
            }
            _ => Ok(None),
        }
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
        // The pause keeps its product shape, but the failure is recorded
        // with its compression category so audit and checkpoints tell a
        // compression-failure pause apart from a user pause.
        let analysis = crate::error_analysis::shared_error_analysis(
            &wf_execution_shared::error::ExecutionSharedError::NodeFailure {
                node_id: entity.id().to_string(),
                category: wf_types::workflow::error_branch::NodeErrorCategory::CompressionFailure,
                detail: format!("context compression failed at version {version}: {reason}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            },
        );
        let record = analysis.to_error_record(entity.id(), None);
        entity.state.write().await.record_error(record);
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
        let limits = self.effective_compaction_limits();
        // Overflow-reactive compaction is delegated to the bound engine so
        // the safety net obeys the same replacement as every other entrance.
        // The range spans the whole request snapshot; the builtin engine
        // reduces it to the forced stateless plan, preserving the invariant
        // that no directive reaches the forced path.
        let range = wf_types::message::SeqRange {
            start: 0,
            end: messages.len().saturating_sub(1) as u64,
        };
        let cancel = entity.get_abort_signal();
        let forced_outcome = {
            let conversation = entity.conversation().read().await;
            let snapshot = conversation.view_messages();
            let seqs = conversation.message_seqs();
            let call = wf_execution_shared::CompactionCall {
                execution_id: entity.id(),
                messages: &snapshot,
                history: conversation.messages(),
                seqs,
                view: conversation.active_view(),
                state: conversation.compaction_state(),
                limits: &limits,
                hot_tokens: tokens_used,
                window_tokens: conversation.compaction_window(),
                context_limit: effective_limit,
                version,
                cancel: &cancel,
            };
            let sink = wf_execution_shared::TracingProgressSink::new(
                self.compaction_engine().name(),
                entity.id(),
            );
            self.compaction_engine()
                .compact_region(&call, &sink, range)
                .await
        };
        let forced_plan: Vec<wf_execution_shared::CompactionPlanItem> = match forced_outcome {
            Ok(wf_execution_shared::CompactionOutcome::View(plan)) => plan.items,
            _ => wf_execution_shared::forced_stateless_plan(
                &messages,
                tokens_used,
                effective_limit,
                &limits,
            ),
        };
        // Forced path carries the same pending contract as the regular
        // path: plan-mode forced results match against this pending copy.
        // The stateless plan never consults planning advice, so no
        // request-level directive can reach it (known boundary).
        entity
            .conversation()
            .write()
            .await
            .set_pending_compaction(forced_plan.clone(), Vec::new());
        // Snapshot the pre-compression state before the safety-net summary
        // workflow runs; the compressed view is checkpointed on write-back.
        self.boundary_checkpoint(
            entity,
            wf_types::checkpoint::CheckpointTiming::BeforeCompression,
        )
        .await;
        // Single shared emission entry with the regular path; the agent hook
        // vocabulary has no compression point, so the static slice stays
        // empty by design (vocabulary extension is out of scope). The
        // stateless plan never consults planning advice, so no directive
        // can reach the forced path (known boundary, shared wording).
        let emission = wf_execution_shared::UnifiedEmission {
            target: wf_execution_shared::CONVERSATION_CONTEXT_ID,
            tokens_used,
            token_limit: effective_limit,
            array_version: version,
            forced: true,
            messages: &messages,
            plan: &forced_plan,
            directive: "",
            execution_id: entity.id(),
            agent_loop_id: Some(entity.id()),
            static_defs: &[],
        };
        let (outcome, audit) = wf_execution_shared::dispatch_unified_emission(
            self.hook_handler_registry.as_deref(),
            self.event_bus.as_deref(),
            &emission,
            entity.get_abort_signal(),
        )
        .await;
        let _ = bus.publish(audit);
        let mut session = entity.conversation().write().await;
        session.mark_compression_emitted(version);
        match outcome {
            wf_execution_shared::CompressionSignalOutcome::Taken => {
                session.reset_compression_no_taker();
                session.begin_compression_flight(version, true);
            }
            wf_execution_shared::CompressionSignalOutcome::Vetoed { reason } => {
                tracing::warn!("forced compression vetoed at version {version}: {reason}");
                session.clear_pending_compaction();
            }
            wf_execution_shared::CompressionSignalOutcome::Untaken => {
                session.clear_pending_compaction();
                let streak = session.record_compression_no_taker();
                wf_execution_shared::report_no_taker(
                    wf_execution_shared::CONVERSATION_CONTEXT_ID,
                    version,
                    streak,
                );
            }
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
        // Compaction requires a known model window. Without one the
        // decision holds and the forced safety-net owns recovery.
        // The hot estimate is refreshed inside the decision so the
        // emitted budget and the threshold comparison share one sample.
        if conversation.compaction_window() == 0 {
            return;
        }
        drop(conversation);
        self.record_compaction_decision(entity, context_limit, prompt_est)
            .await;
    }
}

impl AgentIterationCoordinator {
    /// Budget-driven compaction decision for one settlement point. Syncs
    /// blocks, refreshes the hot estimate, and either holds, drops the
    /// oldest active block, enters handoff, or emits a planned compaction.
    pub(super) async fn record_compaction_decision(
        &self,
        entity: &AgentLoopEntity,
        context_limit: u64,
        prompt_est: u32,
    ) {
        if entity.conversation().write().await.take_handoff_requested() {
            self.enter_handoff_flow(entity, wf_execution_shared::REASON_POST_COMPACT_STILL_HIGH)
                .await;
            return;
        }
        let limits = self.effective_compaction_limits();
        let mut conversation = entity.conversation().write().await;
        conversation.refresh_hot_estimate();
        if conversation.compaction_mode() == wf_execution_shared::CompactionMode::Handoff {
            let hot_probe = wf_execution_shared::context_store::request_hot_estimate(
                conversation.compaction_state().hot_tokens,
                prompt_est as u64,
                conversation.calibration_bias(),
            );
            if conversation.try_exit_handoff(hot_probe) {
                return;
            }
        }
        let hot = wf_execution_shared::context_store::request_hot_estimate(
            conversation.compaction_state().hot_tokens,
            prompt_est as u64,
            conversation.calibration_bias(),
        );
        match conversation.decide_compaction(hot, &limits) {
            wf_execution_shared::CompactionDecision::Hold => {}
            wf_execution_shared::CompactionDecision::DropOldestActive => {
                let recent = limits
                    .recent_raw_turns
                    .unwrap_or(wf_execution_shared::RECENT_RAW_TURNS_DEFAULT)
                    .saturating_mul(2);
                conversation.drop_oldest_active_block(recent);
            }
            wf_execution_shared::CompactionDecision::Handoff { reason } => {
                drop(conversation);
                self.enter_handoff_flow(entity, reason).await;
            }
            wf_execution_shared::CompactionDecision::Compact { aggressive: _ } => {
                // Backpressure coalescing: one in-flight plan settles
                // first. The newer version re-decides on the next
                // settlement instead of stacking a second plan signal.
                // Expiry uses the shared flight predicate so agent and
                // workflow emitters release lost completions identically.
                if conversation.compression_flight().is_some() {
                    let settle_ms = self.effective_settle_timeout_ms();
                    if conversation
                        .take_expired_compression_flight(wf_common::now(), settle_ms)
                        .is_some()
                    {
                        conversation.clear_pending_compaction();
                    } else {
                        return;
                    }
                }
                let version = conversation.conversation_version();
                if !conversation.should_emit_compression(version) {
                    return;
                }
                // The bound engine owns the compact decision and plan. Handoff
                // and oldest-block dropping stay host safety nets above; here
                // the host only applies the engine's outcome through the
                // existing summary-chain emission and write-back path.
                let snapshot = conversation.view_messages();
                let cancel = entity.get_abort_signal();
                let outcome = {
                    let call = wf_execution_shared::CompactionCall {
                        execution_id: entity.id(),
                        messages: &snapshot,
                        history: conversation.messages(),
                        seqs: conversation.message_seqs(),
                        view: conversation.active_view(),
                        state: conversation.compaction_state(),
                        limits: &limits,
                        hot_tokens: hot,
                        window_tokens: conversation.compaction_window(),
                        context_limit,
                        version,
                        cancel: &cancel,
                    };
                    let sink = wf_execution_shared::TracingProgressSink::new(
                        self.compaction_engine().name(),
                        entity.id(),
                    );
                    self.compaction_engine()
                        .compact_if_needed(&call, &sink)
                        .await
                };
                let plan = match outcome {
                    Ok(Some(wf_execution_shared::CompactionOutcome::View(plan))) => plan,
                    Ok(Some(wf_execution_shared::CompactionOutcome::Replace(replacement))) => {
                        // Destructive replacement is a host-owned write-back.
                        // Recompute the shrink proof from host data, then
                        // release the write lock before applying so the host
                        // helper can take it again. The engine never writes.
                        let engine = self.compaction_engine();
                        let proof = {
                            let call = wf_execution_shared::CompactionCall {
                                execution_id: entity.id(),
                                messages: &snapshot,
                                history: conversation.messages(),
                                seqs: conversation.message_seqs(),
                                view: conversation.active_view(),
                                state: conversation.compaction_state(),
                                limits: &limits,
                                hot_tokens: hot,
                                window_tokens: conversation.compaction_window(),
                                context_limit,
                                version,
                                cancel: &cancel,
                            };
                            wf_execution_shared::validate_replacement(
                                engine.name(),
                                engine.capabilities(),
                                &call,
                                replacement.as_ref(),
                                wf_execution_shared::R_MIN_RANGE_DEFAULT,
                            )
                        };
                        drop(conversation);
                        match proof {
                            Ok(_proof) => {
                                if crate::conversation_compression::apply_replacement(
                                    entity.conversation(),
                                    version,
                                    replacement.as_ref(),
                                )
                                .await
                                .is_some()
                                {
                                    self.boundary_checkpoint(
                                        entity,
                                        wf_types::checkpoint::CheckpointTiming::AfterCompression,
                                    )
                                    .await;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("compaction replacement rejected: {e}");
                            }
                        }
                        return;
                    }
                    Ok(None) | Ok(Some(wf_execution_shared::CompactionOutcome::Noop)) => return,
                    Err(e) => {
                        tracing::warn!("compaction engine call failed: {e}");
                        return;
                    }
                };
                let single_summary = wf_execution_shared::is_single_summary_strategy(
                    &wf_execution_shared::compaction_strategy_name(&limits),
                );
                let aggressive = plan.aggressive;
                if !single_summary
                    && aggressive
                    && hot.saturating_sub(plan.freed_tokens)
                        >= conversation.compaction_state().thresholds.t_hard
                {
                    drop(conversation);
                    self.enter_handoff_flow(entity, wf_execution_shared::REASON_HARD_LIMIT)
                        .await;
                    return;
                }
                let legacy = plan.items.is_empty();
                // Budget, count and snapshot share the active view so the
                // plan indices stay valid for the service run. The pending
                // store keeps items and drops only; the directive travels
                // in the signal request.
                let items = plan.items;
                conversation.set_pending_compaction(items.clone(), plan.drops);
                let capped = plan.capped;
                let directive = plan.directive.unwrap_or_default();
                drop(conversation);
                if self
                    .emit_compaction_signal(
                        entity,
                        version,
                        hot,
                        context_limit,
                        snapshot,
                        items,
                        directive,
                    )
                    .await
                {
                    self.record_plan_emitted(legacy, aggressive);
                    if capped {
                        self.record_plan_capped();
                    }
                }
            }
        }
    }

    /// Emit one compaction signal with an optional plan, then anchor
    /// backpressure exactly like the legacy path. An empty plan is the
    /// legacy single-summary mode; a non-empty plan makes the service run
    /// the summary chain once per item and write back one result. Returns
    /// whether the signal reached a taker (only then is the flight
    /// anchored and the emission counted).
    #[allow(clippy::too_many_arguments)]
    async fn emit_compaction_signal(
        &self,
        entity: &AgentLoopEntity,
        version: u64,
        tokens_used: u64,
        context_limit: u64,
        messages: Vec<wf_types::message::Message>,
        plan: Vec<wf_execution_shared::CompactionPlanItem>,
        directive: String,
    ) -> bool {
        // Snapshot the pre-compression state; the compacted view lands in
        // a post-compression checkpoint when the summary workflow writes
        // back. Single shared emission entry with the forced path and the
        // workflow emitter: request build, sync dispatch and audit
        // annotation live in the shared layer.
        self.boundary_checkpoint(
            entity,
            wf_types::checkpoint::CheckpointTiming::BeforeCompression,
        )
        .await;
        // The agent hook vocabulary has no compression point, so the static
        // slice stays empty by design (vocabulary extension is out of scope).
        let emission = wf_execution_shared::UnifiedEmission {
            target: wf_execution_shared::CONVERSATION_CONTEXT_ID,
            tokens_used,
            token_limit: context_limit,
            array_version: version,
            forced: false,
            messages: &messages,
            plan: &plan,
            directive: &directive,
            execution_id: entity.id(),
            agent_loop_id: Some(entity.id()),
            static_defs: &[],
        };
        let (outcome, audit) = wf_execution_shared::dispatch_unified_emission(
            self.hook_handler_registry.as_deref(),
            self.event_bus.as_deref(),
            &emission,
            entity.get_abort_signal(),
        )
        .await;
        if let Some(ref bus) = self.event_bus {
            let _ = bus.publish(audit);
        }
        let mut session = entity.conversation().write().await;
        session.mark_compression_emitted(version);
        // Backpressure anchors only on `Taken` (shared predicate); a vetoed
        // signal clears the pending plan without a no-taker streak, and
        // without a taker nothing settles the flight.
        match &outcome {
            wf_execution_shared::CompressionSignalOutcome::Taken => {
                session.reset_compression_no_taker();
                session.begin_compression_flight(version, false);
            }
            wf_execution_shared::CompressionSignalOutcome::Vetoed { reason } => {
                tracing::warn!(
                    "compaction signal vetoed at version {version}: {reason}; \
                     pending plan cleared without anchoring a flight"
                );
                session.clear_pending_compaction();
            }
            wf_execution_shared::CompressionSignalOutcome::Untaken => {
                session.clear_pending_compaction();
                let streak = session.record_compression_no_taker();
                wf_execution_shared::report_no_taker(
                    wf_execution_shared::CONVERSATION_CONTEXT_ID,
                    version,
                    streak,
                );
            }
        }
        wf_execution_shared::emission_should_anchor(&outcome)
    }

    /// Count one emitted compaction signal on the shared metrics registry
    /// (`legacy` marks the empty-plan single-summary path). Absent when
    /// the runtime runs without metrics.
    pub(super) fn record_plan_emitted(&self, legacy: bool, aggressive: bool) {
        if let Some(ref metrics) = self.metrics {
            metrics
                .compaction()
                .record_plan_emitted(if legacy { "legacy" } else { "plan" }, aggressive);
        }
    }

    /// Count one plan round truncated by the per-round item cap.
    pub(super) fn record_plan_capped(&self) {
        if let Some(ref metrics) = self.metrics {
            metrics.compaction().record_plan_capped();
        }
    }
}
