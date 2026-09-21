use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicContextConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_current_time: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_todo_list: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_workspace_files: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_file_depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_pinned_files: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_skills: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_workflows: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_environment_info: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_sections: Option<std::collections::HashMap<String, String>>,
}
