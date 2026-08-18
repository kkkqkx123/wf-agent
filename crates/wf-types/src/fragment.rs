use serde::{Deserialize, Serialize};

/// Allowed `SystemPromptFragment.category` values. Enforced when fragments
/// are registered through wf-resource's `register_fragment`.
pub const FRAGMENT_CATEGORIES: &[&str] = &[
    "role",
    "capability",
    "constraint",
    "tool-usage",
    "task-instruction",
];

/// Whether `category` is one of [`FRAGMENT_CATEGORIES`].
pub fn is_valid_fragment_category(category: &str) -> bool {
    FRAGMENT_CATEGORIES.contains(&category)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemPromptFragment {
    pub id: String,
    pub category: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<super::TemplateVariableDefinition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FragmentCompositionConfig {
    pub fragment_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
}
