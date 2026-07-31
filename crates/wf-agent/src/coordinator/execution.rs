use std::sync::Arc;

use wf_core::failure_policy::{default_retry_policy, FailurePolicyManager};
use wf_core::interruption::check_execution_interruption;
use wf_execution_shared::error::ExecutionSharedError;
use wf_llm::error::LlmError;
use wf_metrics::MetricsRegistry;
use wf_tools::error::ToolError;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::errors::ErrorKind;
use wf_types::execution::FailurePolicyConfig;

use crate::checkpoint::AgentCheckpointIntegration;
use crate::coordinator::iteration::{AgentIterationCoordinator, IterationResult};
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};

fn http_status_to_kind(status: u16) -> ErrorKind {
    match status {
        400 => ErrorKind::Validation,
        401 | 403 => ErrorKind::AuthError,
        404 => ErrorKind::NotFound,
        429 => ErrorKind::RateLimited,
        500..=599 => ErrorKind::ServiceUnavailable,
        _ => ErrorKind::Network,
    }
}

fn tool_error_to_kind(e: &ToolError) -> ErrorKind {
    match e {
        ToolError::NotFound(_) => ErrorKind::NotFound,
        ToolError::ValidationFailed(_) => ErrorKind::Validation,
        ToolError::RestError { status, .. } => http_status_to_kind(*status),
        ToolError::HttpError(e) => match e.status() {
            Some(s) => http_status_to_kind(s.as_u16()),
            None => ErrorKind::Network,
        },
        ToolError::Timeout { .. } => ErrorKind::Timeout,
        ToolError::ConnectionFailed { .. } => ErrorKind::Network,
        ToolError::TransportError(_) => ErrorKind::Network,
        _ => ErrorKind::Tool,
    }
}

fn extract_error_kind(e: &AgentError) -> ErrorKind {
    match e {
        AgentError::StateError(_) => ErrorKind::StateManagement,
        AgentError::ToolError(te) => tool_error_to_kind(te),
        AgentError::LlmError(LlmError::Timeout(_)) => ErrorKind::Timeout,
        AgentError::LlmError(LlmError::HttpError(e)) => match e.status() {
            Some(s) => http_status_to_kind(s.as_u16()),
            None => ErrorKind::Network,
        },
        AgentError::LlmError(LlmError::AuthError(_)) => ErrorKind::AuthError,
        AgentError::LlmError(LlmError::ProfileNotFound(_)) => ErrorKind::NotFound,
        AgentError::LlmError(_) => ErrorKind::Network,
        AgentError::CheckpointError(_) => ErrorKind::AgentCheckpoint,
        AgentError::SharedError(ExecutionSharedError::StateError(_)) => ErrorKind::StateManagement,
        AgentError::SharedError(ExecutionSharedError::ToolError(te)) => tool_error_to_kind(te),
        AgentError::Internal(_) => ErrorKind::General,
        _ => ErrorKind::Execution,
    }
}

pub struct AgentExecutionCoordinator {
    iteration_coordinator: Arc<AgentIterationCoordinator>,
    checkpoint: Option<AgentCheckpointIntegration>,
    metrics: Option<Arc<MetricsRegistry>>,
}

impl AgentExecutionCoordinator {
    pub fn new(iteration_coordinator: Arc<AgentIterationCoordinator>) -> Self {
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
    ) -> AgentResult<(IterationResult, u32)> {
        let failure_policy = FailurePolicyManager::new(FailurePolicyConfig {
            retry_policy: Some(default_retry_policy()),
            fallback_policy: None,
            non_retryable_errors: None,
            log_level: Some("info".to_string()),
            metrics_enabled: Some(false),
        });

        for iteration in 0..max_iterations {
            let running = entity.state.read().await.is_running();
            if !running {
                break;
            }

            let iteration_start = wf_common::now();
            let iteration_result = self
                .execute_iteration_with_retry(entity, &failure_policy)
                .await?;
            let iteration_duration_ms = (wf_common::now() - iteration_start) as f64;

            if let Some(ref metrics) = self.metrics {
                let profile_id = entity
                    .model()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "default".to_string());
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
                        return Ok((result, iteration + 1));
                    }
                }
                None => {
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

                    let kind = extract_error_kind(&e);
                    if failure_policy.should_retry(kind, attempt) {
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
