use serde::{Deserialize, Serialize};

use super::ForkJoinContext;
use super::NodeExecutionResult;
use super::VariableDefinition;
use super::WorkflowExecutionStatus;
use super::WorkflowExecutionType;
use super::WorkflowGraphStructure;
use crate::execution::ExecutionHierarchy;
use crate::Id;
use crate::Timestamp;
use crate::Version;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecution {
    pub id: Id,
    pub workflow_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_version: Option<Version>,
    pub status: WorkflowExecutionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<WorkflowGraphStructure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<VariableDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_results: Option<Vec<NodeExecutionResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_type: Option<WorkflowExecutionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_join_context: Option<ForkJoinContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<ExecutionHierarchy>,
}
