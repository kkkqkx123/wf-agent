use std::sync::Arc;

use wf_core::failure_policy::{default_retry_policy, FailurePolicyManager};
use wf_core::internal_signal::{InternalSignal, InternalSignalBus, InternalSignalReceiver};
use wf_core::interruption::InterruptionSignal;
use wf_execution_shared::execution_loop;
use wf_metrics::MetricsRegistry;
use wf_types::checkpoint::CheckpointTiming;
use wf_types::execution::FailurePolicyConfig;

use crate::checkpoint::AgentCheckpointIntegration;
use crate::coordinator::iteration::{IterationExecutor, IterationResult};
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::error_analysis::{analyze_error, decide_retry, RetryDecision};

/// Persistence hook invoked after every completed iteration. Lets the
/// coordinator write the `AgentExecution` record at iteration boundaries so a
/// crash mid-loop leaves a record reflecting the real progress (not just the
/// start/end snapshots).
#[async_trait::async_trait]
pub trait IterationPersist: Send + Sync {
    async fn persist_iteration(&self, entity: &AgentLoopEntity);
}

pub struct AgentExecutionCoordinator {
    iteration_coordinator: Arc<dyn IterationExecutor>,
    checkpoint: Option<AgentCheckpointIntegration>,
    iteration_persist: Option<Arc<dyn IterationPersist>>,
    metrics: Option<Arc<MetricsRegistry>>,
    /// Receiver for typed internal signals (replaces the `__`-prefixed
    /// variable protocol); stop/pause/resume are applied at iteration
    /// boundaries.
    ///
    /// Wrapped in a `Mutex` for interior mutability: `execute` is a `&self`
    /// method, but draining the receiver (`try_recv`) requires `&mut` access.
    /// The coordinator is drained at each iteration boundary, so the
    /// mutation is an internal side-effect that must not change the public
    /// `&self` contract.
    signal_receiver: tokio::sync::Mutex<Option<InternalSignalReceiver>>,
}

impl AgentExecutionCoordinator {
    pub fn new(iteration_coordinator: Arc<dyn IterationExecutor>) -> Self {
        Self {
            iteration_coordinator,
            checkpoint: None,
            iteration_persist: None,
            metrics: None,
            signal_receiver: tokio::sync::Mutex::new(None),
        }
    }

    /// Inject the typed signal bus: control signals targeting an agent loop
    /// are delivered to this coordinator at iteration boundaries.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_receiver = tokio::sync::Mutex::new(Some(bus.subscribe()));
        self
    }

    pub fn with_checkpoint(mut self, checkpoint: Option<AgentCheckpointIntegration>) -> Self {
        self.checkpoint = checkpoint;
        self
    }

    /// Register a per-iteration persistence hook (best effort: the hook is
    /// fire-and-forget with regard to the execution outcome).
    pub fn with_iteration_persist(mut self, persist: Option<Arc<dyn IterationPersist>>) -> Self {
        self.iteration_persist = persist;
        self
    }

    pub fn with_metrics(mut self, metrics: Option<Arc<MetricsRegistry>>) -> Self {
        self.metrics = metrics;
        self
    }

    pub async fn execute(
        &self,
        entity: &AgentLoopEntity,
        max_iterations: u32,
        max_execution_time: Option<u64>,
    ) -> AgentResult<(IterationResult, u32)> {
        let failure_policy = FailurePolicyManager::new(FailurePolicyConfig {
            retry_policy: Some(default_retry_policy()),
            fallback_policy: None,
            non_retryable_errors: None,
            log_level: Some("info".to_string()),
            metrics_enabled: Some(false),
        });

        // Wall-clock timeout: a background stop signal interrupts a slow
        // iteration as well. The timeout is paused during approval waits and
        // pauses.
        let timeout_metrics = self.metrics.as_ref().map(|m| m.timeout());
        let timeout_handle = match max_execution_time {
            Some(max) if max > 0 => {
                let interruption = entity.interruption().clone();
                let execution_id = entity.id().to_string();
                if let Some(ref metrics) = timeout_metrics {
                    metrics.record_registration("agent_wall_clock", max as f64, &execution_id);
                }
                let registered_at = std::time::Instant::now();
                let fire_metrics = timeout_metrics.clone();
                Some(entity.timeout_manager().register(
                    format!("wall-clock-{}", entity.id()),
                    std::time::Duration::from_millis(max),
                    move || {
                        tracing::warn!(
                            max_execution_time = max,
                            "Agent loop wall-clock timeout exceeded, stopping execution"
                        );
                        if let Some(ref metrics) = fire_metrics {
                            metrics.record_expiration(
                                "agent_wall_clock",
                                registered_at.elapsed().as_millis() as f64,
                                &execution_id,
                            );
                        }
                        let _ = interruption.stop();
                    },
                ))
            }
            _ => None,
        };

        let outcome = self
            .run_iterations(entity, max_iterations, &failure_policy)
            .await;

        if let Some(handle) = timeout_handle {
            handle.cancel();
            if outcome.is_ok() {
                if let Some(ref metrics) = timeout_metrics {
                    metrics.record_cancellation(
                        "agent_wall_clock",
                        "complete",
                        &entity.id().to_string(),
                    );
                }
            }
        }
        outcome
    }

    async fn run_iterations(
        &self,
        entity: &AgentLoopEntity,
        max_iterations: u32,
        failure_policy: &FailurePolicyManager,
    ) -> AgentResult<(IterationResult, u32)> {
        for iteration in 0..max_iterations {
            // Typed internal signals (stop/pause/resume from trigger
            // actions): drain signals targeting this execution before the
            // suspension gate so a stop lands on the current boundary.
            let mut guard = self.signal_receiver.lock().await;
            if let Some(receiver) = guard.as_mut() {
                self.drain_internal_signals(entity, receiver).await;
            }
            // Snapshot the paused state before the loop suspends, so the
            // pause is recoverable from storage after a crash. No-op unless
            // the entity is actually paused.
            if let Some(ref cp) = self.checkpoint {
                cp.on_pause(entity).await;
            }
            // Suspension gate: a paused loop waits here for resume; a forced
            // stop (wall-clock / pause timeout / explicit stop) exits below.
            execution_loop::wait_for_resume(entity.interruption()).await;
            if execution_loop::is_stopped(entity) {
                return Err(Self::stopped_error(entity).await);
            }

            let iteration_start = wf_common::now();
            let iteration_result = self
                .execute_iteration_with_retry(entity, failure_policy)
                .await?;
            let iteration_duration_ms = (wf_common::now() - iteration_start) as f64;

            if let Some(ref metrics) = self.metrics {
                let profile_id = entity.model().to_string();
                metrics.agent_loop().record_iteration(iteration_duration_ms);
                metrics.agent().record_iteration(&profile_id);
            }

            match iteration_result {
                Some(result) => {
                    if let Some(ref cp) = self.checkpoint {
                        cp.create_checkpoint_gated(entity, CheckpointTiming::AfterExecute, None)
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!("Failed to create iteration checkpoint: {}", e);
                                false
                            });
                    }

                    // Persist the execution record at the iteration boundary so
                    // a crash mid-loop leaves a record reflecting real progress.
                    if let Some(ref persist) = self.iteration_persist {
                        persist.persist_iteration(entity).await;
                    }

                    if !result.should_continue {
                        // The iteration stopped early. Distinguish a genuine
                        // completion from an interruption-driven stop: a pause
                        // at the iteration boundary suspends the loop instead
                        // of terminating it; a stop exits with an error.
                        match entity.interruption().check() {
                            Some(InterruptionSignal::Pause) => continue,
                            Some(InterruptionSignal::Stop) => {
                                return Err(Self::stopped_error(entity).await);
                            }
                            _ => {
                                // File-checkpoint approval policy at loop end
                                // (auto merge / submit to the approval layer).
                                if let Some(ref cp) = self.checkpoint {
                                    cp.on_agent_complete(entity.id().as_str());
                                }
                                return Ok((result, iteration + 1));
                            }
                        }
                    }
                }
                None => {
                    // Reached only when a stop/abort interrupted the iteration.
                    return Err(Self::stopped_error(entity).await);
                }
            }
        }

        if let Some(ref metrics) = self.metrics {
            metrics.agent_loop().record_max_iterations_reached();
        }

        let content = serde_json::Value::String("Max iterations reached".to_string());
        let result = IterationResult {
            should_continue: false,
            content,
            completion_data: None,
            tool_call_count: 0,
            finish_reason: wf_tools::callback::LoopFinishReason::MaxIterationsReached,
        };

        // No completion checkpoint here: the terminal status is only settled
        // after this returns, so the `OnComplete` checkpoint is taken by the
        // outer lifecycle once `Completed` is applied and the snapshot can
        // actually record it.

        // File-checkpoint approval policy at loop end (auto merge / submit
        // to the approval layer).
        if let Some(ref cp) = self.checkpoint {
            cp.on_agent_complete(entity.id().as_str());
        }

        Ok((result, max_iterations))
    }

    /// Drain typed internal signals targeting this execution. Stop/pause/
    /// resume flip the interruption state (the existing gates act on it);
    /// async result signals are logged as the extension point for a loop
    /// awaiting an asynchronous sub-workflow/script/agent result.
    async fn drain_internal_signals(
        &self,
        entity: &AgentLoopEntity,
        receiver: &mut InternalSignalReceiver,
    ) {
        while let Some(signal) = receiver.try_recv() {
            if signal.target_execution_id() != entity.id() {
                continue;
            }
            match signal {
                InternalSignal::StopWorkflow { .. } => {
                    tracing::debug!(execution_id = %entity.id(), "agent loop stop signal");
                    let _ = entity.interruption().stop();
                }
                InternalSignal::PauseWorkflow { .. } => {
                    tracing::debug!(execution_id = %entity.id(), "agent loop pause signal");
                    let _ = entity.interruption().pause();
                }
                InternalSignal::ResumeWorkflow { .. } => {
                    tracing::debug!(execution_id = %entity.id(), "agent loop resume signal");
                    let _ = entity.interruption().resume();
                }
                InternalSignal::SkipNode { .. } => {
                    // Node-level skipping applies to workflows only.
                }
                InternalSignal::SubworkflowResult { .. }
                | InternalSignal::ScriptResult { .. }
                | InternalSignal::AgentResult { .. } => {
                    tracing::debug!(
                        execution_id = %entity.id(),
                        signal_type = signal.variant_name(),
                        "agent loop received async result signal"
                    );
                }
            }
        }
    }

    /// The error for a stopped execution. An explicit `stop()` already settled
    /// the state machine (terminal status); a stop during host shutdown is a
    /// cancellation; only a wall-clock / pause-timeout stop is a timeout.
    async fn stopped_error(entity: &AgentLoopEntity) -> AgentError {
        let status = entity.state.read().await.status();
        if status.is_terminal() {
            AgentError::Cancelled(format!("Agent loop stopped with status {:?}", status))
        } else if wf_common::shutdown::is_active_shutdown() {
            AgentError::Cancelled("Agent loop cancelled by runtime shutdown".to_string())
        } else {
            AgentError::ExecutionTimeout(
                "Agent loop execution time exceeded or was force-stopped".to_string(),
            )
        }
    }

    /// Persist a failing iteration: the error record just written to the
    /// state and, on a granted retry, the charged budget must survive a
    /// crash. Best effort like the rest of the integration: a failed
    /// checkpoint is logged, never propagated into the outcome.
    async fn checkpoint_on_error(&self, entity: &AgentLoopEntity) {
        if let Some(ref cp) = self.checkpoint {
            cp.create_checkpoint_gated(entity, CheckpointTiming::OnError, None)
                .await
                .unwrap_or_else(|ce| {
                    tracing::warn!("Failed to create error checkpoint: {}", ce);
                    false
                });
        }
    }

    async fn execute_iteration_with_retry(
        &self,
        entity: &AgentLoopEntity,
        failure_policy: &FailurePolicyManager,
    ) -> AgentResult<Option<IterationResult>> {
        let mut attempt: u32 = 0;
        loop {
            match self.iteration_coordinator.execute_iteration(entity).await {
                Ok(result) => return Ok(Some(result)),
                Err(e) => {
                    // Record the structured analysis first so the error
                    // checkpoint taken below contains this failure.
                    let analysis = analyze_error(&e);
                    let record = analysis.to_error_record(entity.id(), None);
                    let kind = analysis.kind;
                    let limit = error_retry_limit(failure_policy);
                    let per_call_allowed = failure_policy.should_retry(kind, attempt);
                    // One write guard: record the failure and take the
                    // cross-iteration decision against the run's history.
                    let decision = {
                        let mut state = entity.state.write().await;
                        state.record_error(record);
                        decide_retry(
                            state.error_records(),
                            &analysis,
                            state.retry_total(kind),
                            per_call_allowed,
                            limit,
                        )
                    };

                    match decision {
                        RetryDecision::Grant => {
                            // Charge the grant before the checkpoint so a
                            // restore never hands back a spent retry.
                            entity.state.write().await.record_retry(kind);
                            self.checkpoint_on_error(entity).await;
                            let delay = failure_policy.next_delay(attempt);
                            attempt += 1;
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                        RetryDecision::Stop => {
                            self.checkpoint_on_error(entity).await;
                            return Err(e);
                        }
                        RetryDecision::Trip {
                            repeats,
                            error_type,
                        } => {
                            self.checkpoint_on_error(entity).await;
                            return Err(AgentError::ErrorPatternTripped(format!(
                                "error type {error_type:?} recurred {repeats} times \
                                 (limit {limit}); stopping retries: {e}"
                            )));
                        }
                    }
                }
            }
        }
    }
}

/// Cross-iteration cap and repeated-error breaker threshold: the failure
/// policy's retry budget, falling back to the conservative constant when no
/// retry policy is configured.
fn error_retry_limit(policy: &FailurePolicyManager) -> u32 {
    policy
        .config()
        .retry_policy
        .as_ref()
        .map(|retry| retry.max_retries)
        .unwrap_or(crate::constants::RETRY_LIMIT_FALLBACK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use wf_execution_shared::error::ExecutionSharedError;
    use wf_execution_shared::types::execution_entity::ExecutionStatus;
    use wf_llm::error::LlmError;
    use wf_tools::error::ToolError;
    use wf_types::errors::ErrorKind;
    use wf_types::execution::RetryPolicy;
    use wf_types::Id;

    use crate::coordinator::iteration::IterationExecutor;

    struct SlowIteration {
        delay: Duration,
    }

    #[async_trait::async_trait]
    impl IterationExecutor for SlowIteration {
        async fn execute_iteration(
            &self,
            _entity: &AgentLoopEntity,
        ) -> AgentResult<IterationResult> {
            tokio::time::sleep(self.delay).await;
            Ok(IterationResult {
                should_continue: false,
                content: serde_json::Value::String("done".to_string()),
                completion_data: None,
                tool_call_count: 0,
                finish_reason: wf_tools::callback::LoopFinishReason::Completed,
            })
        }
    }

    #[tokio::test]
    async fn test_agent_wall_clock_timeout() {
        let entity = AgentLoopEntity::new(Id::from("agent-timeout-1".to_string()));
        entity.state.write().await.start().unwrap();

        let coordinator = AgentExecutionCoordinator::new(Arc::new(SlowIteration {
            delay: Duration::from_millis(100),
        }));

        let err = coordinator
            .execute(&entity, 10, Some(20))
            .await
            .expect_err("wall-clock timeout must fail the agent loop");
        assert!(
            matches!(err, AgentError::ExecutionTimeout(_)),
            "timeout must surface as ExecutionTimeout: {err}"
        );
        assert!(entity.interruption().is_interrupted());
        assert_eq!(
            entity.state.read().await.status(),
            ExecutionStatus::Running,
            "the execution coordinator does not settle the terminal state"
        );
    }

    #[tokio::test]
    async fn test_agent_no_timeout_when_under_budget() {
        let entity = AgentLoopEntity::new(Id::from("agent-no-timeout-1".to_string()));
        entity.state.write().await.start().unwrap();

        let coordinator = AgentExecutionCoordinator::new(Arc::new(SlowIteration {
            delay: Duration::from_millis(10),
        }));

        let (result, iterations) = coordinator
            .execute(&entity, 10, Some(5000))
            .await
            .expect("under budget must complete normally");
        assert_eq!(iterations, 1);
        assert!(!result.should_continue);
        assert_eq!(
            result.content,
            serde_json::Value::String("done".to_string())
        );
    }

    struct MultiStepIteration {
        total: u32,
        step: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl IterationExecutor for MultiStepIteration {
        async fn execute_iteration(
            &self,
            _entity: &AgentLoopEntity,
        ) -> AgentResult<IterationResult> {
            let step = self.step.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(IterationResult {
                should_continue: step < self.total,
                content: serde_json::Value::String(format!("step-{step}")),
                completion_data: None,
                tool_call_count: 1,
                finish_reason: wf_tools::callback::LoopFinishReason::Completed,
            })
        }
    }

    /// The iteration-level persistence hook fires after every completed
    /// iteration, not just at the start/end of the loop (a crash
    /// mid-loop must leave a record reflecting real progress).
    #[tokio::test]
    async fn test_iteration_persist_fires_every_iteration() {
        #[derive(Clone)]
        struct CountingPersist {
            count: Arc<AtomicU32>,
        }
        #[async_trait::async_trait]
        impl IterationPersist for CountingPersist {
            async fn persist_iteration(&self, _entity: &AgentLoopEntity) {
                self.count.fetch_add(1, Ordering::SeqCst);
            }
        }

        let persist = CountingPersist {
            count: Arc::new(AtomicU32::new(0)),
        };

        let entity = AgentLoopEntity::new(Id::from("agent-iter-persist-1".to_string()));
        entity.state.write().await.start().unwrap();

        let coordinator = AgentExecutionCoordinator::new(Arc::new(MultiStepIteration {
            total: 3,
            step: std::sync::atomic::AtomicU32::new(0),
        }))
        .with_iteration_persist(Some(Arc::new(persist.clone())));

        let (result, iterations) = coordinator
            .execute(&entity, 10, Some(5000))
            .await
            .expect("multi-step run completes");
        assert_eq!(iterations, 3);
        assert!(!result.should_continue);
        assert_eq!(persist.count.load(Ordering::SeqCst), 3);
    }

    /// Fails the first call of every simulated iteration and succeeds on the
    /// retry, so exactly one failure reaches each next iteration boundary the
    /// way a run that keeps hitting the same fault does. `alternate_types`
    /// switches the failure between two error types that share
    /// [`ErrorKind::Network`], isolating the cross-iteration budget from the
    /// repeated-error breaker.
    struct FlakyIteration {
        calls: AtomicU32,
        alternate_types: bool,
    }

    #[async_trait::async_trait]
    impl IterationExecutor for FlakyIteration {
        async fn execute_iteration(
            &self,
            _entity: &AgentLoopEntity,
        ) -> AgentResult<IterationResult> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call.is_multiple_of(2) {
                let failure = call / 2;
                let error = if self.alternate_types && failure % 2 == 1 {
                    AgentError::LlmError(LlmError::ProviderError("HTTP 500 upstream".to_string()))
                } else {
                    AgentError::SharedError(ExecutionSharedError::ToolError(
                        ToolError::TransportError("connection reset".to_string()),
                    ))
                };
                return Err(error);
            }
            Ok(IterationResult {
                should_continue: true,
                content: serde_json::Value::String("recovered".to_string()),
                completion_data: None,
                tool_call_count: 0,
                finish_reason: wf_tools::callback::LoopFinishReason::Completed,
            })
        }
    }

    /// Retry policy that keeps the same budget as production but without the
    /// second-long backoff, so the budget tests stay fast.
    fn fast_failure_policy() -> FailurePolicyManager {
        FailurePolicyManager::new(FailurePolicyConfig {
            retry_policy: Some(RetryPolicy {
                base_delay_ms: 1,
                jitter: Some(false),
                ..default_retry_policy()
            }),
            fallback_policy: None,
            non_retryable_errors: None,
            log_level: None,
            metrics_enabled: None,
        })
    }

    /// The cross-iteration budget, not the per-call one, caps a run that keeps
    /// failing: the per-call attempt restarts every iteration, so without the
    /// tally the retries would never stop. Mixed error types under one kind
    /// keep the breaker silent, and the failure after the cap is terminal.
    #[tokio::test]
    async fn cross_iteration_retry_budget_caps_total_retries() {
        let entity = AgentLoopEntity::new(Id::from("agent-retry-budget".to_string()));
        entity.state.write().await.start().unwrap();

        let executor = Arc::new(FlakyIteration {
            calls: AtomicU32::new(0),
            alternate_types: true,
        });
        let coordinator = AgentExecutionCoordinator::new(executor.clone());
        let policy = fast_failure_policy();

        for iteration in 0..3 {
            coordinator
                .execute_iteration_with_retry(&entity, &policy)
                .await
                .unwrap_or_else(|e| panic!("iteration {iteration} must recover: {e}"));
        }

        let err = coordinator
            .execute_iteration_with_retry(&entity, &policy)
            .await
            .expect_err("an exhausted cross-iteration budget must fail terminally");
        assert!(
            !matches!(err, AgentError::ErrorPatternTripped(_)),
            "mixed error types exhaust the budget instead of tripping the breaker: {err}"
        );
        assert_eq!(
            entity.state.read().await.retry_total(ErrorKind::Network),
            3,
            "one kind may be granted max_retries retries over the whole run"
        );
        assert_eq!(
            executor.calls.load(Ordering::SeqCst),
            7,
            "three recovering iterations plus the one failure that ends the run"
        );
    }

    /// The breaker, not the budget, stops a run whose failures keep repeating
    /// the very same error type: it fires at the threshold while the granted
    /// retry total still has headroom.
    #[tokio::test]
    async fn repeating_error_type_trips_the_circuit_breaker() {
        let entity = AgentLoopEntity::new(Id::from("agent-error-pattern".to_string()));
        entity.state.write().await.start().unwrap();

        let executor = Arc::new(FlakyIteration {
            calls: AtomicU32::new(0),
            alternate_types: false,
        });
        let coordinator = AgentExecutionCoordinator::new(executor.clone());
        let policy = fast_failure_policy();

        for iteration in 0..2 {
            coordinator
                .execute_iteration_with_retry(&entity, &policy)
                .await
                .unwrap_or_else(|e| panic!("iteration {iteration} must recover: {e}"));
        }

        let err = coordinator
            .execute_iteration_with_retry(&entity, &policy)
            .await
            .expect_err("the third identical failure must trip the breaker");
        let message = err.to_string();
        assert!(
            matches!(err, AgentError::ErrorPatternTripped(_)),
            "a repeating error type must stop as ErrorPatternTripped: {message}"
        );
        assert!(
            message.contains("ToolError recurred 3 times")
                && message.contains("(limit 3)")
                && message.contains("stopping retries"),
            "the breaker error must carry the recurring type, count and limit: {message}"
        );

        let state = entity.state.read().await;
        assert_eq!(
            state.retry_total(ErrorKind::Network),
            2,
            "the breaker stops the run while the budget still had headroom"
        );
        assert_eq!(
            state.error_records().len(),
            3,
            "every failure of the run is recorded"
        );
    }
}
