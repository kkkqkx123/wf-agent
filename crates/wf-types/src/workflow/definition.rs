use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowTemplateType {
    TriggeredSubworkflow,
    Standalone,
    Dependent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggeredSubworkflowConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_checkpoints: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTemplate {
    pub id: super::super::Id,
    pub name: String,
    pub description: Option<String>,
    pub r#type: Option<WorkflowTemplateType>,
    pub version: Option<super::super::Version>,
    pub nodes: Vec<super::super::node::BaseStaticNode>,
    pub edges: Vec<super::Edge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<super::WorkflowConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<super::super::workflow_execution::VariableDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<super::super::trigger::TriggerDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggered_subworkflow_config: Option<TriggeredSubworkflowConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<WorkflowMetadata>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<crate::tool::AvailableTools>,
}
