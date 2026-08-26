use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Definition of one template variable (`{name}` placeholder).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateVariableDefinition {
    pub name: String,
    pub r#type: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
}

/// Allowed `Template.category` values: `system | rules | user-command |
/// tools | composite | fragments | dynamic`, plus the values the Rust side
/// already uses: `user`/`assistant` (custom prompt type mapping) and
/// `tool-visibility` (wf-resource visibility texts). Validation lives in
/// wf-config (`validate_prompt_template`); the field stays a `String` so
/// externally loaded configs keep deserializing.
pub const TEMPLATE_CATEGORIES: &[&str] = &[
    "system",
    "rules",
    "user-command",
    "tools",
    "composite",
    "fragments",
    "dynamic",
    "user",
    "assistant",
    "tool-visibility",
];

/// Whether `category` is one of [`TEMPLATE_CATEGORIES`].
pub fn is_valid_template_category(category: &str) -> bool {
    TEMPLATE_CATEGORIES.contains(&category)
}

/// Unified templateable prompt text.
///
/// Carries both system-prompt templates (`system.default` et al.) and
/// tool-visibility prompt texts (activation/block announcements,
/// discoverable metadata block, general description). Registered as a
/// loadable resource so operators can adjust the texts without code
/// changes. Variables use the `{{name}}` placeholder syntax (legacy
/// `{name}` is still rendered); `{{fragments}}` and
/// `{{tool_descriptions}}` are renderer pseudo-variables (composed by the
/// render engine, not substituted verbatim).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Template {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub category: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<TemplateVariableDefinition>>,
    /// Fragment ids composed into the `{fragments}` pseudo-variable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragments: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateFillRule {
    pub template_id: String,
    pub variable_mapping: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment_mapping: Option<HashMap<String, String>>,
}
