use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerTemplate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<super::TriggerCondition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<super::TriggerAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_triggers: Option<u32>,
    /// Template priority when multiple templates match one event: higher
    /// wins; equal priority falls back to specificity then registration
    /// order. Default 0 (the predefined fallback compression trigger is
    /// registered with a negative priority).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_checkpoint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_description_template: Option<String>,
}
