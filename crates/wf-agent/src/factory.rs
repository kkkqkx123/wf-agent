use crate::entity::AgentLoopEntity;

pub struct AgentLoopFactory;

impl AgentLoopFactory {
    pub fn create(id: wf_types::Id) -> AgentLoopEntity {
        AgentLoopEntity::new(id)
    }
}
