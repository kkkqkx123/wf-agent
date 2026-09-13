use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PredefinedToolsPresetConfig {
    pub enabled: Option<bool>,
    pub tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PredefinedPromptsPresetConfig {
    pub enabled: Option<bool>,
    pub prompts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PresetsConfig {
    pub predefined_tools: Option<PredefinedToolsPresetConfig>,
    pub predefined_prompts: Option<PredefinedPromptsPresetConfig>,
}
