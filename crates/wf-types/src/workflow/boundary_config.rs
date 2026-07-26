use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowStartConfig {
    pub trigger_on_start: Option<bool>,
    pub initial_variables: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowEndConfig {
    pub output_variable: Option<String>,
    pub on_completion: Option<String>,
}
