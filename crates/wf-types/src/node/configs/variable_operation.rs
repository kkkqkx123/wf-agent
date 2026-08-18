use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AggregateMode {
    Array,
    Object,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum VariableOperationConfig {
    Aggregate {
        source_variable: String,
        target_variable: String,
        aggregate_mode: AggregateMode,
    },
    Transform {
        source_variable: String,
        target_variable: String,
        transform: String,
    },
    BatchUpdate {
        #[serde(skip_serializing_if = "Option::is_none")]
        source_variable: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_variable: Option<String>,
        updates: Vec<VariableUpdate>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableUpdate {
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableOperationOutput {
    pub operation: String,
    pub modified_variables: Vec<String>,
    pub execution_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<serde_json::Value>,
}
