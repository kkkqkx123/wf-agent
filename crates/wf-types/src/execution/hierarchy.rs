use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionType {
    Workflow,
    AgentLoop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionIdentity {
    pub r#type: ExecutionType,
    pub id: super::super::Id,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionHierarchy {
    pub workflow_id: super::super::Id,
    pub execution_id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_execution_id: Option<super::super::Id>,
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_execution_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ChildExecutionReference>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChildExecutionReference {
    pub child_type: ExecutionType,
    pub child_id: super::super::Id,
    pub created_at: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_path_id: Option<String>,
}
