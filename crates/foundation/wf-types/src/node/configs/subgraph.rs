use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubgraphNodeConfig {
    pub subgraph_id: Option<String>,
    pub embed_id: Option<String>,
    pub async_: Option<bool>,
    /// Child failure policy for the SUBGRAPH node: when set to an ignore
    /// marker (`ignore`, `continue`, `continue_on_error`) a failed child
    /// execution resolves into a marked success instead of failing the
    /// parent. Absent means fail-fast propagation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_child_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_inputs: Option<Vec<super::super::super::workflow::WorkflowVariableInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_outputs: Option<Vec<super::super::super::workflow::WorkflowVariableOutput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubgraphNodeOutput {
    pub execution_result: SubgraphExecutionResult,
    pub duration: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubgraphExecutionResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    pub status: String,
}
