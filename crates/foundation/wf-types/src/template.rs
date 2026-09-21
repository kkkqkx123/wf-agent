use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Declared shape of one template variable. The prompt text itself is
/// always rendered as text; the declaration records the caller-side shape
/// so registration and typed rendering can reject mismatches early.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TemplateVariableType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

/// Definition of one template variable (`{{name}}` placeholder).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateVariableDefinition {
    pub name: String,
    pub r#type: TemplateVariableType,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
}

/// Whether one dotted-path segment addresses an object key or an array index.
/// Numeric segments address arrays, other segments address object keys.
pub fn is_valid_template_path_segment(segment: &str) -> bool {
    if segment.is_empty() {
        return false;
    }
    if segment.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Validate a dotted template path. Returns the reason when invalid.
/// Single owner for path shape rules so every engine accepts the same paths.
pub fn validate_template_path(path: &str) -> Option<String> {
    if path.trim().is_empty() {
        return Some("template path cannot be empty".to_string());
    }
    if path.starts_with('.') || path.ends_with('.') || path.contains("..") {
        return Some(format!(
            "template path '{path}' has empty segments; use dotted identifiers like 'user.name'"
        ));
    }
    for segment in path.split('.') {
        if !is_valid_template_path_segment(segment) {
            return Some(format!(
                "template path '{path}' has invalid segment '{segment}'; each segment must be an identifier or array index"
            ));
        }
    }
    None
}

/// Render a JSON value as display text for template substitution: strings
/// pass through, null becomes empty, anything else uses its JSON form.
/// Single owner for the coercion so every template call site agrees on
/// null and scalar rendering.
pub fn template_value_to_display_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Whether the JSON value matches the declared variable shape.
pub fn variable_value_matches(template_type: &TemplateVariableType, value: &serde_json::Value) -> bool {
    match template_type {
        TemplateVariableType::String => value.is_string(),
        TemplateVariableType::Number => value.is_number(),
        TemplateVariableType::Boolean => value.is_boolean(),
        TemplateVariableType::Array => value.is_array(),
        TemplateVariableType::Object => value.is_object(),
    }
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
/// changes. Variables use the `{{name}}` placeholder syntax;
/// `{{fragments}}` is a renderer pseudo-variable (composed by the render
/// engine, not substituted verbatim).
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
    /// Fragment ids composed into the `{{fragments}}` pseudo-variable.
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
