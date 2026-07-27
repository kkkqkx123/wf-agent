use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_tools::callback::{AgentLoopOutput, AgentLoopConfig, AgentLoopInput};
use wf_types::Id;

use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;
use crate::state::AgentLoopState;

pub struct AgentLoopCoordinator;

impl AgentLoopCoordinator {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute(
        &self,
        entity: &AgentLoopEntity,
        max_iterations: u32,
    ) -> AgentResult<AgentLoopOutput> {
        entity.state.write().await.start();

        for iteration in 0..max_iterations {
            if !entity.state.read().await.is_running() {
                break;
            }

            entity.state.write().await.start_iteration();

            let result = self.execute_iteration(entity).await;

            entity.state.write().await.end_iteration();

            match result {
                Ok(output) => {
                    if output.should_continue {
                        entity.state.write().await.record_tool_call();
                    } else {
                        entity.state.write().await.complete();
                        return Ok(AgentLoopOutput {
                            result: output.content,
                            iterations: iteration + 1,
                        });
                    }
                }
                Err(e) => {
                    entity.state.write().await.fail(e.to_string());
                    return Err(e);
                }
            }
        }

        entity.state.write().await.complete();
        Ok(AgentLoopOutput {
            result: serde_json::Value::String("Max iterations reached".to_string()),
            iterations: max_iterations,
        })
    }

    async fn execute_iteration(
        &self,
        _entity: &AgentLoopEntity,
    ) -> AgentResult<IterationResult> {
        Ok(IterationResult {
            should_continue: false,
            content: serde_json::Value::String("stub".to_string()),
        })
    }
}

struct IterationResult {
    should_continue: bool,
    content: serde_json::Value,
}
