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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkflowMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Where this definition was derived from, when it was created from a
    /// template or cloned from another definition.
    ///
    /// First-class lineage for template-to-workflow derivation and clones:
    /// publish/unpublish only flips the published mark, derivation always
    /// writes this field, and history without it is treated as sourceless
    /// rather than backfilled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<DefinitionDerivation>,
    /// Whether the workflow is published to the template library.
    ///
    /// The marker lives with the definition because the registry is in
    /// memory: a definition is the only place a publish decision can
    /// outlive a restart. The registry entry derives its published state
    /// from here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<bool>,
    /// Whether anyone may use the workflow from the template library.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_public: Option<bool>,
    /// Whether the workflow is a system fixture rather than a user entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_system: Option<bool>,
    /// Whether the workflow may be run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
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

/// First-class derivation lineage shared by every definition body.
///
/// A definition created from a template or cloned from another definition
/// records its source here: the source kind (`workflow`, `agent`, `node`,
/// `trigger`), the source id, and when the derivation happened. History
/// without a source is sourceless; it is never backfilled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DefinitionDerivation {
    pub source_kind: String,
    pub source_id: Id,
    pub derived_at: Timestamp,
}

impl DefinitionDerivation {
    pub fn new(
        source_kind: impl Into<String>,
        source_id: impl Into<Id>,
        derived_at: Timestamp,
    ) -> Self {
        Self {
            source_kind: source_kind.into(),
            source_id: source_id.into(),
            derived_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggeredSubworkflowConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_checkpoints: Option<bool>,
    /// Wall-clock execution budget in milliseconds.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_published: Option<bool>,
}
