use std::sync::Arc;

use wf_execution_shared::hooks::executor::HookExecutor;
use wf_llm::LlmWrapper;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};
use wf_tools::registry::ToolRegistry;

use crate::coordinator::execution::AgentExecutionCoordinator;
use crate::coordinator::iteration::AgentIterationCoordinator;
use crate::coordinator::state_transitor::AgentLoopStateTransitor;
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentLoopCoordinator {
    llm_wrapper: Arc<LlmWrapper>,
    tool_registry: Arc<ToolRegistry>,
    hook_executor: Arc<HookExecutor>,
}

impl AgentLoopCoordinator {
    pub fn new(
        llm_wrapper: Arc<LlmWrapper>,
        tool_registry: Arc<ToolRegistry>,
    ) -> Self {
        let hook_executor = Arc::new(HookExecutor::new());
        Self {
            llm_wrapper,
            tool_registry,
            hook_executor,
        }
    }

    pub fn with_hook_executor(mut self, hook_executor: Arc<HookExecutor>) -> Self {
        self.hook_executor = hook_executor;
        self
    }

    pub async fn execute(
        &self,
        config: AgentLoopConfig,
        _input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let entity = self.build_entity(config).await?;

        AgentLoopStateTransitor::start_agent_loop(&entity).await?;

        let iteration_coordinator = Arc::new(AgentIterationCoordinator::new(
            self.llm_wrapper.clone(),
            self.tool_registry.clone(),
            self.hook_executor.clone(),
        ));
        let execution_coordinator = AgentExecutionCoordinator::new(iteration_coordinator);

        let max_iterations = 10;
        match execution_coordinator.execute(&entity, max_iterations).await {
            Ok((result, iterations)) => {
                if result.completion_data.is_some() || !result.should_continue {
                    AgentLoopStateTransitor::complete_agent_loop(&entity).await?;
                }
                Ok(AgentLoopOutput {
                    result: result.content,
                    iterations,
                })
            }
            Err(e) => {
                AgentLoopStateTransitor::fail_agent_loop(&entity, e.to_string()).await?;
                Err(e)
            }
        }
    }

    async fn build_entity(&self, config: AgentLoopConfig) -> AgentResult<AgentLoopEntity> {
        Ok(AgentLoopEntity::new(config.agent_id.clone()))
    }
}
