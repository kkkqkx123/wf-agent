use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentLoopFactory;

impl AgentLoopFactory {
    pub fn create(id: wf_types::Id) -> AgentLoopEntity {
        AgentLoopEntity::new(id)
    }
}
