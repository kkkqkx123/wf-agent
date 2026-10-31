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

/// Nested property rule for an object-typed template variable. One level
/// only; deeper nesting is checked as the top-level kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateVariableProperty {
    pub r#type: TemplateVariableType,
    #[serde(default)]
    pub required: bool,
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
    /// One level of object property rules. `None` keeps the top-level
    /// kind check only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, TemplateVariableProperty>>,
    /// Expected element kind for array values. Explicit null elements
    /// count as missing and skip the check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_type: Option<TemplateVariableType>,
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
/// Display use only for prompt text, approval hints, skill content and
/// command embedding. Condition evaluation uses its own literal converter
/// with quoting semantics and must not call this function.
/// Single owner for the coercion so every display call site agrees on
/// null and scalar rendering.
pub fn template_value_to_display_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Whether the JSON value matches the declared variable shape.
pub fn variable_value_matches(
    template_type: &TemplateVariableType,
    value: &serde_json::Value,
) -> bool {
    match template_type {
        TemplateVariableType::String => value.is_string(),
        TemplateVariableType::Number => value.is_number(),
        TemplateVariableType::Boolean => value.is_boolean(),
        TemplateVariableType::Array => value.is_array(),
        TemplateVariableType::Object => value.is_object(),
    }
}

/// One context shape problem with a structured field reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateContextIssue {
    pub field: String,
    pub message: String,
}

impl TemplateContextIssue {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Fill absent context keys from declared defaults. Explicit null counts
/// as missing: a non-null default backfills it, otherwise the key is
/// dropped so the required check reports it once.
pub fn apply_template_context_defaults(
    context: &HashMap<String, serde_json::Value>,
    schema: &[TemplateVariableDefinition],
) -> HashMap<String, serde_json::Value> {
    let mut filled = HashMap::new();
    for (key, value) in context {
        if !value.is_null() {
            filled.insert(key.clone(), value.clone());
        }
    }
    for declared in schema {
        let missing = !filled.contains_key(&declared.name);
        if !missing {
            continue;
        }
        if let Some(default) = declared.default_value.as_ref() {
            if !default.is_null() {
                filled.insert(declared.name.clone(), default.clone());
            }
        }
    }
    filled
}

/// Collect every context shape problem without stopping at the first.
/// Declared keys must match their kind plus one level of nesting;
/// missing required keys without a backfillable default are errors.
/// Unknown keys pass by default and fail only in strict mode. Explicit
/// null counts as missing.
pub fn collect_template_context_issues(
    context: &HashMap<String, serde_json::Value>,
    schema: &[TemplateVariableDefinition],
    strict: bool,
) -> Vec<TemplateContextIssue> {
    let mut issues = Vec::new();
    for declared in schema {
        match context.get(&declared.name) {
            None => {
                if declared.required && declared.default_value.as_ref().is_none_or(|v| v.is_null())
                {
                    issues.push(TemplateContextIssue::new(
                        format!("context.{}", declared.name),
                        format!("context key '{}' is required", declared.name),
                    ));
                }
            }
            Some(value) if value.is_null() => {
                if declared.required && declared.default_value.as_ref().is_none_or(|v| v.is_null())
                {
                    issues.push(TemplateContextIssue::new(
                        format!("context.{}", declared.name),
                        format!("context key '{}' is required", declared.name),
                    ));
                }
            }
            Some(value) => {
                if !variable_value_matches(&declared.r#type, value) {
                    issues.push(TemplateContextIssue::new(
                        format!("context.{}", declared.name),
                        format!(
                            "context key '{}' expects {:?}, got {}",
                            declared.name, declared.r#type, value
                        ),
                    ));
                    continue;
                }
                issues.extend(collect_template_nested_issues(&declared.name, value, declared));
            }
        }
    }
    if strict {
        for key in context.keys() {
            if key.is_empty() || value_is_null(context, key) {
                continue;
            }
            if !schema.iter().any(|declared| declared.name == *key) {
                issues.push(TemplateContextIssue::new(
                    format!("context.{key}"),
                    format!("context has unknown key '{key}'"),
                ));
            }
        }
    }
    issues
}

fn value_is_null(context: &HashMap<String, serde_json::Value>, key: &str) -> bool {
    context.get(key).is_none_or(|v| v.is_null())
}

/// One level of nesting for an already kind-matched context value.
fn collect_template_nested_issues(
    name: &str,
    value: &serde_json::Value,
    declared: &TemplateVariableDefinition,
) -> Vec<TemplateContextIssue> {
    let mut issues = Vec::new();
    if let Some(properties) = declared.properties.as_ref() {
        if let serde_json::Value::Object(map) = value {
            for (prop_name, prop_rule) in properties {
                match map.get(prop_name) {
                    None | Some(serde_json::Value::Null) => {
                        if prop_rule.required {
                            issues.push(TemplateContextIssue::new(
                                format!("context.{name}.{prop_name}"),
                                format!(
                                    "context key '{name}.{prop_name}' is missing required property"
                                ),
                            ));
                        }
                    }
                    Some(prop_value) => {
                        if !variable_value_matches(&prop_rule.r#type, prop_value) {
                            issues.push(TemplateContextIssue::new(
                                format!("context.{name}.{prop_name}"),
                                format!(
                                    "context key '{name}.{prop_name}' expects {:?}, got {}",
                                    prop_rule.r#type, prop_value
                                ),
                            ));
                        }
                    }
                }
            }
        }
    }
    if let Some(element_type) = declared.element_type.as_ref() {
        if let serde_json::Value::Array(items) = value {
            for (index, item) in items.iter().enumerate() {
                if item.is_null() {
                    continue;
                }
                if !variable_value_matches(element_type, item) {
                    issues.push(TemplateContextIssue::new(
                        format!("context.{name}[{index}]"),
                        format!(
                            "context key '{name}[{index}]' expects {:?}, got {}",
                            element_type, item
                        ),
                    ));
                }
            }
        }
    }
    issues
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

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Vec<TemplateVariableDefinition> {
        vec![TemplateVariableDefinition {
            name: "count".to_string(),
            r#type: TemplateVariableType::Number,
            required: true,
            description: None,
            default_value: None,
            properties: None,
            element_type: None,
        }]
    }

    #[test]
    fn null_counts_as_missing_and_defaults_backfill() {
        let label_schema = vec![TemplateVariableDefinition {
            name: "label".to_string(),
            r#type: TemplateVariableType::String,
            required: false,
            description: None,
            default_value: Some(serde_json::json!("hi")),
            properties: None,
            element_type: None,
        }];
        let filled = apply_template_context_defaults(
            &HashMap::from([("label".to_string(), serde_json::Value::Null)]),
            &label_schema,
        );
        assert_eq!(filled["label"], "hi");
        let required = schema();
        let issues = collect_template_context_issues(
            &HashMap::from([("count".to_string(), serde_json::Value::Null)]),
            &required,
            false,
        );
        assert!(issues.iter().any(|issue| issue.field == "context.count"));
    }

    #[test]
    fn nested_and_strict_context_rules() {
        let object_schema = vec![TemplateVariableDefinition {
            name: "user".to_string(),
            r#type: TemplateVariableType::Object,
            required: false,
            description: None,
            default_value: None,
            properties: Some(HashMap::from([(
                "age".to_string(),
                TemplateVariableProperty {
                    r#type: TemplateVariableType::Number,
                    required: true,
                },
            )])),
            element_type: None,
        }];
        let bad = HashMap::from([("user".to_string(), serde_json::json!({"age": "old"}))]);
        let issues = collect_template_context_issues(&bad, &object_schema, false);
        assert!(issues.iter().any(|issue| issue.field == "context.user.age"));
        let extra = HashMap::from([("nope".to_string(), serde_json::json!(1))]);
        assert!(collect_template_context_issues(&extra, &object_schema, false).is_empty());
        assert!(collect_template_context_issues(&extra, &object_schema, true).len() == 1);
    }
}
