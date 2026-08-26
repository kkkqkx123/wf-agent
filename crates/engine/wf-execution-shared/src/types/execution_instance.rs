use super::execution_entity::ExecutionEntity;

pub enum ExecutionInstance {
    AgentLoop(Box<dyn ExecutionEntity>),
    Workflow(Box<dyn ExecutionEntity>),
}
