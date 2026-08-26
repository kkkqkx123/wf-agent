use serde::{Deserialize, Serialize};

use crate::message::Message;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowVariableInput {
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
pub struct WorkflowVariableOutput {
    pub internal_name: String,
    pub target_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowMessageInput {
    pub source_context_id: String,
    pub internal_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowMessageOutput {
    pub internal_name: String,
    pub target_context_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDataInput {
    pub parent_field: String,
    pub internal_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDataOutput {
    pub internal_name: String,
    pub output_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowStartConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_inputs: Option<Vec<WorkflowVariableInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_inputs: Option<Vec<WorkflowMessageInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_inputs: Option<Vec<WorkflowDataInput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowEndConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_outputs: Option<Vec<WorkflowVariableOutput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_outputs: Option<Vec<WorkflowMessageOutput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_outputs: Option<Vec<WorkflowDataOutput>>,
}
