use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecution {
    pub id: super::super::Id,
    pub workflow_id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_version: Option<super::super::Version>,
    pub status: super::WorkflowExecutionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<super::WorkflowGraphStructure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<super::VariableDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_results: Option<Vec<super::NodeExecutionResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
    pub started_at: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<super::super::Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_type: Option<super::WorkflowExecutionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_join_context: Option<super::ForkJoinContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<super::super::execution::ExecutionHierarchy>,
}
