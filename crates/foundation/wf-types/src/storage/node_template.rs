use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeTemplateStorageMetadata {
    pub id: super::super::Id,
    pub name: String,
    pub node_type: String,
    pub description: Option<String>,
    /// First-class derivation lineage: set when created from a template or
    /// cloned. Absent means sourceless; never backfilled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<super::super::workflow::definition::DefinitionDerivation>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}
