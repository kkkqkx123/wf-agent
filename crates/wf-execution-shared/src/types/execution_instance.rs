use super::execution_entity::IExecutionEntity;

pub enum ExecutionInstance {
    AgentLoop(Box<dyn IExecutionEntity>),
    Workflow(Box<dyn IExecutionEntity>),
}
