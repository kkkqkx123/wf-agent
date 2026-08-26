use serde::{Deserialize, Serialize};

use super::Edge;
use super::WorkflowConfig;
use crate::hook::BaseHookConfig;
use crate::node::BaseStaticNode;
use crate::tool::AvailableTools;
use crate::workflow_execution::VariableDefinition;
use crate::Id;
use crate::Timestamp;
use crate::Version;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowDefinitionType {
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
pub struct WorkflowDefinition {
    pub id: Id,
    pub name: String,
    pub description: Option<String>,
    pub r#type: Option<WorkflowDefinitionType>,
    pub version: Option<Version>,
    pub nodes: Vec<BaseStaticNode>,
    pub edges: Vec<Edge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<WorkflowConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<VariableDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggered_subworkflow_config: Option<TriggeredSubworkflowConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<WorkflowMetadata>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tools: Option<AvailableTools>,
    /// Workflow-level hooks (BEFORE_EXECUTE / AFTER_EXECUTE, per node).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<BaseHookConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTemplate {
    pub id: Id,
    pub name: String,
    pub description: String,
    pub definition: WorkflowDefinition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_public: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}
