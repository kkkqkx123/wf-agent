use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentExecutionCoordinator;

impl AgentExecutionCoordinator {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute(
        &self,
        _entity: &AgentLoopEntity,
        _max_iterations: u32,
    ) -> AgentResult<()> {
        Ok(())
    }
}
