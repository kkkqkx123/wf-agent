use serde::{Deserialize, Serialize};

/// Persisted agent hook template definition (TS `AgentHookTemplateRegistryAPI`
/// counterpart). Hooks are reusable lifecycle hooks attached to agent loops,
/// keyed by their hook type (BEFORE_ITERATION, AFTER_TOOL_CALL, ...).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentHookTemplateStorageMetadata {
    pub id: super::super::Id,
    pub name: String,
    pub hook_type: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    /// Serialized hook event config (event_name / condition / weight).
    pub hook_config: Option<serde_json::Value>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}
