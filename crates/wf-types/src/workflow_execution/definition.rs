use serde::{Deserialize, Serialize};

use super::ForkJoinContext;
use super::NodeExecutionResult;
use super::VariableDefinition;
use super::WorkflowExecutionStatus;
use super::WorkflowExecutionType;
use super::WorkflowGraphStructure;
use crate::checkpoint::workflow::WorkflowExecutionStateSnapshot;
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

impl From<WorkflowExecutionStateSnapshot> for WorkflowExecution {
    fn from(snapshot: WorkflowExecutionStateSnapshot) -> Self {
        let status =
            serde_json::from_str::<WorkflowExecutionStatus>(&format!("\"{}\"", snapshot.status))
                .unwrap_or(WorkflowExecutionStatus::Created);

        let variables = Some(
            snapshot
                .variable_state
                .variables
                .into_iter()
                .map(|(name, value)| VariableDefinition {
                    name,
                    value,
                    r#type: None,
                    scope: None,
                    readonly: None,
                    metadata: None,
                })
                .collect(),
        );

        let node_results = snapshot.node_results.map(|map| {
            map.into_values()
                .filter_map(|value| serde_json::from_value::<NodeExecutionResult>(value).ok())
                .collect()
        });

        let fork_join_context = snapshot
            .fork_join_context
            .and_then(|v| serde_json::from_value::<ForkJoinContext>(v).ok());

        Self {
            id: snapshot.execution_id,
            workflow_id: String::new(),
            workflow_version: None,
            status,
            current_node_id: snapshot.current_node_id,
            graph: None,
            variables,
            input: snapshot.input,
            output: snapshot.output,
            node_results,
            errors: None,
            started_at: 0,
            completed_at: None,
            error: None,
            execution_type: None,
            fork_join_context,
            hierarchy: None,
        }
    }
}
