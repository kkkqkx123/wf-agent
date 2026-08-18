use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncVariableExchange {
    pub source_path_id: String,
    pub source_variable: String,
    pub target_path_id: String,
    pub target_variable: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncNodeConfig {
    pub source_path_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_path_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_mappings: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_for_completion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_exchanges: Option<Vec<SyncVariableExchange>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncNodeOutput {
    pub synced_from_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synced_variables: Option<Vec<String>>,
    pub synced_variable_count: u32,
    pub synced_data_count: u32,
    pub synced_message_count: u32,
    pub completed: bool,
}
