use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopVariableInput {
    pub source_path: String,
    pub internal_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataSource {
    pub iterable: String,
    pub variable_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopStartNodeConfig {
    pub loop_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_inputs: Option<Vec<LoopVariableInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<DataSource>,
    pub max_iterations: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_iteration_failure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_consecutive_failures: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopStartNodeOutput {
    pub loop_id: String,
    pub iteration_count: u32,
    pub max_iterations: u32,
    pub has_more_iterations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopEndNodeConfig {
    pub loop_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub break_condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_start_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopEndNodeOutput {
    pub loop_id: String,
    pub break_triggered: bool,
    pub iteration_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_node_id: Option<String>,
}
