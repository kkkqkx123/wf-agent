use std::sync::Arc;

use wf_core::failure_policy::{default_retry_policy, FailurePolicyManager};
use wf_core::interruption::check_execution_interruption;
use wf_metrics::MetricsRegistry;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::execution::FailurePolicyConfig;

use crate::checkpoint::AgentCheckpointIntegration;
use crate::coordinator::iteration::{IterationExecutor, IterationResult};
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::error_analysis::analyze_error;

pub struct AgentExecutionCoordinator {
    iteration_coordinator: Arc<dyn IterationExecutor>,
    checkpoint: Option<AgentCheckpointIntegration>,
    metrics: Option<Arc<MetricsRegistry>>,
}

impl AgentExecutionCoordinator {
    pub fn new(iteration_coordinator: Arc<dyn IterationExecutor>) -> Self {
        Self {
            iteration_coordinator,
            checkpoint: None,
            metrics: None,
        }
    }

    pub fn with_checkpoint(mut self, checkpoint: Option<AgentCheckpointIntegration>) -> Self {
        self.checkpoint = checkpoint;
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

        // Wall-clock timeout: a background stop signal mirrors TS
        // TimeoutManager so a slow iteration is interrupted as well. The
        // timeout is paused during approval waits and pauses.
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
            .run_iterations(entity, max_iterations, max_execution_time, &failure_policy)
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
        max_execution_time: Option<u64>,
        failure_policy: &FailurePolicyManager,
    ) -> AgentResult<(IterationResult, u32)> {
        for iteration in 0..max_iterations {
            let running = entity.state.read().await.is_running();
            if !running {
                break;
            }

            let iteration_start = wf_common::now();
            let iteration_result = self
                .execute_iteration_with_retry(entity, failure_policy)
                .await?;
            let iteration_duration_ms = (wf_common::now() - iteration_start) as f64;

            if let Some(ref metrics) = self.metrics {
                let profile_id = entity.model().to_string();
                metrics
                    .agent_loop()
                    .record_iteration(entity.id(), iteration_duration_ms);
                metrics.agent().record_iteration(&profile_id);
            }

            match iteration_result {
                Some(result) => {
                    if let Some(ref cp) = self.checkpoint {
                        cp.create_checkpoint(entity, CheckpointTrigger::AfterExecute)
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!("Failed to create iteration checkpoint: {}", e);
                            });
                    }

                    if !result.should_continue {
                        if Self::is_stopped(entity) {
                            return Err(Self::timeout_error(max_execution_time));
                        }
                        return Ok((result, iteration + 1));
                    }
                }
                None => {
                    if Self::is_stopped(entity) {
                        return Err(Self::timeout_error(max_execution_time));
                    }
                    break;
                }
            }
        }

        if let Some(ref metrics) = self.metrics {
            metrics
                .agent_loop()
                .record_max_iterations_reached(entity.id());
        }

        let content = serde_json::Value::String("Max iterations reached".to_string());
        let result = IterationResult {
            should_continue: false,
            content,
            completion_data: None,
            tool_call_count: 0,
        };

        if let Some(ref cp) = self.checkpoint {
            cp.create_checkpoint(entity, CheckpointTrigger::OnComplete)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to create agent completion checkpoint: {}", e);
                });
        }

        Ok((result, max_iterations))
    }

    /// The wall-clock timeout (or an explicit stop) has been signalled.
    fn is_stopped(entity: &AgentLoopEntity) -> bool {
        matches!(
            entity.interruption().check(),
            Some(wf_core::interruption::InterruptionSignal::Stop)
        )
    }

    fn timeout_error(max_execution_time: Option<u64>) -> AgentError {
        AgentError::ExecutionError(format!(
            "Agent loop exceeded max_execution_time ({}ms)",
            max_execution_time.unwrap_or(0)
        ))
    }

    async fn execute_iteration_with_retry(
        &self,
        entity: &AgentLoopEntity,
        failure_policy: &FailurePolicyManager,
    ) -> AgentResult<Option<IterationResult>> {
        let interruption = check_execution_interruption(entity.interruption(), None);
        match interruption {
            wf_core::types::interruption::ExecutionInterruptionCheckResult::Paused { .. } => {
                return Ok(None);
            }
            wf_core::types::interruption::ExecutionInterruptionCheckResult::Stopped { .. }
            | wf_core::types::interruption::ExecutionInterruptionCheckResult::Aborted { .. } => {
                return Ok(None);
            }
            _ => {}
        }

        let mut attempt: u32 = 0;
        loop {
            match self.iteration_coordinator.execute_iteration(entity).await {
                Ok(result) => return Ok(Some(result)),
                Err(e) => {
                    if let Some(ref cp) = self.checkpoint {
                        cp.create_checkpoint(entity, CheckpointTrigger::OnError)
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
        entity.state.write().await.start();

        let coordinator = AgentExecutionCoordinator::new(Arc::new(SlowIteration {
            delay: Duration::from_millis(100),
        }));

        let err = coordinator
            .execute(&entity, 10, Some(20))
            .await
            .expect_err("wall-clock timeout must fail the agent loop");
        assert!(err.to_string().contains("max_execution_time"));
        assert!(entity.interruption().is_interrupted());
    }

    #[tokio::test]
    async fn test_agent_no_timeout_when_under_budget() {
        let entity = AgentLoopEntity::new(Id::from("agent-no-timeout-1".to_string()));
        entity.state.write().await.start();

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
}
