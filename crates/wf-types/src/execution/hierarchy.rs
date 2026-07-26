use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionHierarchy {
    pub workflow_id: super::super::Id,
    pub execution_id: super::super::Id,
    pub parent_execution_id: Option<super::super::Id>,
    pub depth: u32,
}
