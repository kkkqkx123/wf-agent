use std::sync::Arc;

use wf_core::execution_loop;
use wf_core::failure_policy::{default_retry_policy, FailurePolicyManager};
use wf_core::interruption::InterruptionSignal;
use wf_metrics::MetricsRegistry;
use wf_types::checkpoint::CheckpointTiming;
use wf_types::execution::FailurePolicyConfig;

use crate::checkpoint::AgentCheckpointIntegration;
use crate::coordinator::iteration::{IterationExecutor, IterationResult};
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::error_analysis::analyze_error;

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
}

impl AgentExecutionCoordinator {
    pub fn new(iteration_coordinator: Arc<dyn IterationExecutor>) -> Self {
        Self {
            iteration_coordinator,
            checkpoint: None,
            iteration_persist: None,
            metrics: None,
        }
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
        let timeout_handle = match max_execution_time {
            Some(max) if max > 0 => {
                let interruption = entity.interruption().clone();
                Some(entity.timeout_manager().register(
                    format!("wall-clock-{}", entity.id()),
                    std::time::Duration::from_millis(max),
                    move || {
                        tracing::warn!(
                            max_execution_time = max,
                            "Agent loop wall-clock timeout exceeded, stopping execution"
                        );
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
                        cp.create_checkpoint(entity, CheckpointTiming::AfterExecute)
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!("Failed to create iteration checkpoint: {}", e);
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
        };

        if let Some(ref cp) = self.checkpoint {
            cp.create_checkpoint(entity, CheckpointTiming::OnComplete)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to create agent completion checkpoint: {}", e);
                });
        }

        // File-checkpoint approval policy at loop end (auto merge / submit
        // to the approval layer).
        if let Some(ref cp) = self.checkpoint {
            cp.on_agent_complete(entity.id().as_str());
        }

        Ok((result, max_iterations))
    }

    /// The error for a stopped execution. An explicit `stop()` already settled
    /// the state machine (terminal status); everything else is a timeout.
    async fn stopped_error(entity: &AgentLoopEntity) -> AgentError {
        let status = entity.state.read().await.status();
        if status.is_terminal() {
            AgentError::ExecutionError(format!("Agent loop stopped with status {:?}", status))
        } else {
            AgentError::ExecutionTimeout(
                "Agent loop execution time exceeded or was force-stopped".to_string(),
            )
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
                    if let Some(ref cp) = self.checkpoint {
                        cp.create_checkpoint(entity, CheckpointTiming::OnError)
                            .await
                            .unwrap_or_else(|ce| {
                                tracing::warn!("Failed to create error checkpoint: {}", ce);
                            });
                    }

                    // Persist the structured analysis into the entity state so
                    // it lands in the snapshot and can be queried post-hoc.
                    let analysis = analyze_error(&e);
                    let record = analysis.to_error_record(entity.id(), None);
                    entity.state.write().await.record_error(record);

                    if failure_policy.should_retry(analysis.kind, attempt) {
                        let delay = failure_policy.next_delay(attempt);
                        attempt += 1;
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use wf_execution_shared::types::execution_entity::ExecutionStatus;
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
            use std::sync::atomic::Ordering;
            let step = self.step.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(IterationResult {
                should_continue: step < self.total,
                content: serde_json::Value::String(format!("step-{step}")),
                completion_data: None,
                tool_call_count: 1,
            })
        }
    }

    /// The iteration-level persistence hook fires after every completed
    /// iteration, not just at the start/end of the loop (a crash
    /// mid-loop must leave a record reflecting real progress).
    #[tokio::test]
    async fn test_iteration_persist_fires_every_iteration() {
        use std::sync::atomic::{AtomicU32, Ordering};

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
}
