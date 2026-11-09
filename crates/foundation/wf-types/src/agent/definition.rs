use serde::{Deserialize, Serialize};

use super::AgentConfig;
use crate::Id;
use crate::Timestamp;
use crate::Version;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Whether the agent is published to the template library. Unset reads
    /// as unpublished, so pre-existing definitions keep their visibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<bool>,
    /// Whether anyone may use the agent from the template library.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_public: Option<bool>,
    /// Whether the agent is a system fixture rather than a user entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_system: Option<bool>,
    /// Whether the agent may run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentDefinition {
    pub id: Id,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Version>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<AgentConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AgentMetadata>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTemplate {
    pub id: Id,
    pub name: String,
    pub description: String,
    pub definition: AgentDefinition,
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
