use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionOperationType {
    UpdateVariables,
    AddMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowVariableUpdateConfig {
    pub variable_name: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowMessageConfig {
    pub role: String,
    pub content_template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInteractionNodeConfig {
    pub operation_type: InteractionOperationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<WorkflowVariableUpdateConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<WorkflowMessageConfig>,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInteractionNodeOutput {
    pub operation_type: InteractionOperationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_variables: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_messages: Option<Vec<String>>,
}
