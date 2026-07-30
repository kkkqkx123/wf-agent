use std::sync::Arc;

use wf_core::failure_policy::{
    default_retry_policy, ExecutionSharedErrorProxy, FailurePolicyManager,
};
use wf_core::interruption::check_execution_interruption;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::execution::FailurePolicyConfig;

use crate::checkpoint::AgentCheckpointIntegration;
use crate::coordinator::iteration::{AgentIterationCoordinator, IterationResult};
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentExecutionCoordinator {
    iteration_coordinator: Arc<AgentIterationCoordinator>,
    checkpoint: Option<AgentCheckpointIntegration>,
}

impl AgentExecutionCoordinator {
    pub fn new(iteration_coordinator: Arc<AgentIterationCoordinator>) -> Self {
        Self {
            iteration_coordinator,
            checkpoint: None,
        }
    }

    pub fn with_checkpoint(mut self, checkpoint: Option<AgentCheckpointIntegration>) -> Self {
        self.checkpoint = checkpoint;
        self
    }

    pub async fn execute(
        &self,
        entity: &AgentLoopEntity,
        max_iterations: u32,
    ) -> AgentResult<(IterationResult, u32)> {
        let failure_policy = FailurePolicyManager::new(FailurePolicyConfig {
            retry_policy: Some(default_retry_policy()),
            fallback_policy: None,
            non_retryable_errors: Some(vec!["abort".to_string(), "cancelled".to_string()]),
            log_level: Some("info".to_string()),
            metrics_enabled: Some(false),
        });

        for iteration in 0..max_iterations {
            let running = entity.state.read().await.is_running();
            if !running {
                break;
            }

            let iteration_result = self
                .execute_iteration_with_retry(entity, &failure_policy)
                .await?;

            match iteration_result {
                Some(result) => {
                    if let Some(ref cp) = self.checkpoint {
                        cp.create_checkpoint(entity, CheckpointTrigger::AfterExecute).await
                            .unwrap_or_else(|e| {
                                tracing::warn!("Failed to create iteration checkpoint: {}", e);
                            });
                    }

                    if !result.should_continue {
                        return Ok((result, iteration + 1));
                    }
                }
                None => {
                    break;
                }
            }
        }

        let content = serde_json::Value::String("Max iterations reached".to_string());
        let result = IterationResult {
            should_continue: false,
            content,
            completion_data: None,
            tool_call_count: 0,
        };

        if let Some(ref cp) = self.checkpoint {
            cp.create_checkpoint(entity, CheckpointTrigger::OnComplete).await
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to create agent completion checkpoint: {}", e);
                });
        }

        Ok((result, max_iterations))
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
                        cp.create_checkpoint(entity, CheckpointTrigger::OnError).await
                            .unwrap_or_else(|ce| {
                                tracing::warn!("Failed to create error checkpoint: {}", ce);
                            });
                    }

                    let proxy = ExecutionSharedErrorProxy::from_message(e.to_string());
                    if failure_policy.should_retry(&proxy, attempt) {
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
