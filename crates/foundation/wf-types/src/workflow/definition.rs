use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::Edge;
use super::WorkflowConfig;
use crate::hook::HookPointConfig;
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

/// Terminal-failure fallback policy declared by a compression summary
/// sub-workflow. The policy names what the emitting execution receives when
/// every summary attempt has failed; anything a fallback lands must stay
/// visible to the LLM (an explicit notice heads the array) — a silently
/// shortened history is never an acceptable degradation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CompressionFallbackMode {
    /// Publish `CONTEXT_COMPRESSION_FAILED` and park the emitting execution
    /// on the standard pause path for external handling (no write-back).
    #[default]
    Fail,
    /// Write back a locally trimmed window headed by an explicit failure
    /// notice as a degraded `CONTEXT_COMPRESSION_COMPLETED`, so the emitting
    /// execution survives with a context whose gaps it can see.
    PartialSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggeredSubworkflowConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_checkpoints: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Terminal-failure fallback for compression runs of this workflow
    /// (read by the compression service from the summary workflow resource;
    /// ignored by workflows that never serve a compression chain).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression_fallback: Option<CompressionFallbackMode>,
}

/// Persisted canvas position for one node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LayoutPoint {
    pub x: i32,
    pub y: i32,
}

/// Canvas layout algorithm persisted with a workflow.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LayoutKind {
    #[default]
    Layered,
    Columns,
    Rows,
    Compact,
}

/// Canvas layout direction persisted with a workflow.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LayoutDirection {
    Lr,
    Tb,
    Rl,
    Bt,
}

/// Canvas layout settings persisted with a workflow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LayoutSettings {
    #[serde(default)]
    pub kind: LayoutKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<LayoutDirection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank_sep: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_sep: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_height: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_width: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<i32>,
}

/// Persisted canvas layout for a workflow definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WorkflowLayout {
    #[serde(default)]
    pub positions: BTreeMap<String, LayoutPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<LayoutSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
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
    pub hooks: Option<Vec<HookPointConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<WorkflowLayout>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_system: Option<bool>,
}
