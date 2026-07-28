use std::sync::Arc;

use wf_execution_shared::interruption::check_execution_interruption;
use wf_execution_shared::retry::budget::{RetryBudget, RetryBudgetConfig, TimeBudgetMode};

use crate::coordinator::iteration::{AgentIterationCoordinator, IterationResult};
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentExecutionCoordinator {
    iteration_coordinator: Arc<AgentIterationCoordinator>,
}

impl AgentExecutionCoordinator {
    pub fn new(iteration_coordinator: Arc<AgentIterationCoordinator>) -> Self {
        Self { iteration_coordinator }
    }

    pub async fn execute(
        &self,
        entity: &AgentLoopEntity,
        max_iterations: u32,
    ) -> AgentResult<(IterationResult, u32)> {
        let retry_config = RetryBudgetConfig {
            max_retries: 3,
            time_budget_ms: 300_000,
            time_budget_mode: TimeBudgetMode::DelayOnly,
        };
        let mut budget = RetryBudget::new(retry_config);

        for iteration in 0..max_iterations {
            let running = entity.state.read().await.is_running();
            if !running {
                break;
            }

            let iteration_result = self.execute_iteration_with_retry(entity, &mut budget).await?;

            match iteration_result {
                Some(result) => {
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
        Ok((
            IterationResult {
                should_continue: false,
                content,
                completion_data: None,
                tool_call_count: 0,
            },
            max_iterations,
        ))
    }

    async fn execute_iteration_with_retry(
        &self,
        entity: &AgentLoopEntity,
        budget: &mut RetryBudget,
    ) -> AgentResult<Option<IterationResult>> {
        let interruption = check_execution_interruption(entity.interruption(), None);
        match interruption {
            wf_execution_shared::types::interruption::ExecutionInterruptionCheckResult::Paused { .. } => {
                return Ok(None);
            }
            wf_execution_shared::types::interruption::ExecutionInterruptionCheckResult::Stopped { .. }
            | wf_execution_shared::types::interruption::ExecutionInterruptionCheckResult::Aborted { .. } => {
                return Ok(None);
            }
            _ => {}
        }

        loop {
            match self.iteration_coordinator.execute_iteration(entity).await {
                Ok(result) => return Ok(Some(result)),
                Err(e) => {
                    if budget.can_retry() {
                        let delay = std::time::Duration::from_millis(
                            1000 * 2_u64.pow(budget.attempts().min(6)),
                        );
                        budget.record_attempt(delay);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(e);
                }
            }
        }
    }
}
