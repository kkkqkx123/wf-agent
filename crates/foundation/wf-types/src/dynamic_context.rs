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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicRuntimeContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub todo_list: Option<Vec<super::TodoItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_files: Option<Vec<super::PinnedFileItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<super::SkillConfigItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflows: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_file_tree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<std::collections::HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicContextMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}
