use serde::{Deserialize, Serialize};

/// Variable names read through the shared variable view for volatile tail assembly.
pub const TODO_LIST_VARIABLE: &str = "todo_list";
pub const PINNED_FILES_VARIABLE: &str = "pinned_files";
pub const WORKSPACE_FILE_TREE_VARIABLE: &str = "workspace_file_tree";
pub const CUSTOM_DATA_VARIABLE: &str = "custom_data";

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
