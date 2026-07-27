use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentIterationCoordinator;

impl AgentIterationCoordinator {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute_iteration(
        &self,
        _entity: &AgentLoopEntity,
    ) -> AgentResult<()> {
        Ok(())
    }
}
