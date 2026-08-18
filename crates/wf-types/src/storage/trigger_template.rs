use serde::{Deserialize, Serialize};

/// Persisted trigger template definition. A trigger template is a reusable
/// trigger definition that agent loops reference by name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerTemplateStorageMetadata {
    pub id: super::super::Id,
    pub name: String,
    /// `event` | `condition` | `schedule`.
    pub trigger_type: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub enabled: bool,
    pub max_triggers: Option<u32>,
    pub priority: Option<i32>,
    /// Serialized trigger condition (event name / condition expression).
    pub condition: Option<serde_json::Value>,
    /// Serialized trigger action config.
    pub action_config: Option<serde_json::Value>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}
