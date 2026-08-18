use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubgraphNodeConfig {
    pub subgraph_id: Option<String>,
    pub embed_id: Option<String>,
    pub async_: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<crate::execution::RetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<crate::execution::FailureAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<serde_json::Value>,
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
