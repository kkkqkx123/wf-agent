use serde_json::Value;

use wf_types::llm::LlmExecutionConfig;

/// A single config issue found on a workflow node. Field carries a dotted
/// path pointing at the offending attribute (e.g. `nodes.n1.config.profile_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeConfigIssue {
    pub field: String,
    pub message: String,
}

impl NodeConfigIssue {
    pub(crate) fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

pub(crate) fn field_path(node_id: &str, field: &str) -> String {
    format!("nodes.{}.config.{}", node_id, field)
}

pub(crate) fn node_path(node_id: &str) -> String {
    format!("nodes.{}", node_id)
}

pub fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn validate_variable_path(path: &str) -> Option<String> {
    if path.trim().is_empty() {
        return Some("variable path cannot be empty".to_string());
    }
    if path.starts_with('.') || path.ends_with('.') || path.contains("..") {
        return Some(format!(
            "variable path '{}' has empty segments; use dotted identifiers like 'user.name'",
            path
        ));
    }
    for segment in path.split('.') {
        if !is_valid_identifier(segment) {
            return Some(format!(
                "variable path '{}' has invalid segment '{}'; each segment must start with a letter or '_' and contain only letters, digits or '_'",
                path, segment
            ));
        }
    }
    None
}

pub(crate) fn validate_internal_name(name: &str) -> Option<String> {
    if !is_valid_identifier(name) {
        return Some(format!(
            "internal_name '{}' must start with a letter or '_' and contain only letters, digits or '_'",
            name
        ));
    }
    None
}

/// Validate embedded `${path}` / `{{path}}` references inside a free-form
/// expression. Bare literals without references always pass; unterminated or
/// malformed paths fail registration instead of resolving to empty at runtime.
pub(crate) fn validate_embedded_variable_paths(expression: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let bytes = expression.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            match expression[i + 2..].find('}') {
                Some(end) => {
                    let path = &expression[i + 2..i + 2 + end];
                    if let Some(reason) = validate_variable_path(path) {
                        problems.push(format!("invalid '${{{}}}' reference: {}", path, reason));
                    }
                    i += 2 + end + 1;
                }
                None => {
                    problems.push("unterminated '${...}' reference; missing '}'".to_string());
                    break;
                }
            }
            continue;
        }
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            match expression[i + 2..].find("}}") {
                Some(end) => {
                    let path = &expression[i + 2..i + 2 + end];
                    if let Some(reason) = validate_variable_path(path) {
                        problems.push(format!("invalid '{{{{{}}}}} reference: {}", path, reason));
                    }
                    i += 2 + end + 2;
                }
                None => {
                    problems.push("unterminated '{{...}}' reference; missing '}}'".to_string());
                    break;
                }
            }
            continue;
        }
        i += 1;
    }
    problems
}

pub(crate) fn require_string(
    node_id: &str,
    node_type: &str,
    config: &Value,
    field: &str,
    required: bool,
) -> Option<NodeConfigIssue> {
    match config.get(field) {
        Some(Value::String(s)) if !s.trim().is_empty() => None,
        Some(_) => Some(NodeConfigIssue::new(
            field_path(node_id, field),
            format!(
                "Node '{}' ({}) field '{}' must be a non-empty string",
                node_id, node_type, field
            ),
        )),
        None if required => Some(NodeConfigIssue::new(
            field_path(node_id, field),
            format!(
                "Node '{}' ({}) is missing required config '{}'",
                node_id, node_type, field
            ),
        )),
        None => None,
    }
}

pub(crate) fn field_not_in(
    node_id: &str,
    node_type: &str,
    config: &Value,
    field: &str,
    allowed: &[&str],
) -> Option<NodeConfigIssue> {
    let value = config.get(field)?;
    let actual = value.as_str().unwrap_or_default();
    if allowed.contains(&actual) {
        return None;
    }
    Some(NodeConfigIssue::new(
        field_path(node_id, field),
        format!(
            "Node '{}' ({}) field '{}' has invalid value '{}', expected one of {}",
            node_id,
            node_type,
            field,
            actual,
            allowed.join(", ")
        ),
    ))
}

/// Validate one optional typed field: absent or null passes, a present
/// value must deserialize to `T` (the same serde shape the execution
/// handlers parse, so static and runtime agree on what is valid).
pub(crate) fn validate_typed_field<T: serde::de::DeserializeOwned>(
    node_id: &str,
    node_type: &str,
    config: &Value,
    field: &str,
) -> Option<NodeConfigIssue> {
    let value = config.get(field)?;
    if value.is_null() {
        return None;
    }
    serde_json::from_value::<T>(value.clone()).err().map(|e| {
        NodeConfigIssue::new(
            field_path(node_id, field),
            format!(
                "Node '{}' ({}) field '{}' has invalid value: {}",
                node_id, node_type, field, e
            ),
        )
    })
}

/// Validate the flattened execution settings carried on the node config
/// (token limits, timeouts, nested generation). The handlers parse this
/// same blob as `LlmExecutionConfig` and degrade the whole blob to defaults
/// on a type mismatch, so a mismatch is a registration error, not a
/// runtime warning. Unknown node-level keys are ignored by the typed
/// struct, exactly as at runtime. `generation` is excluded here because it
/// gets its own field-level issue via `validate_typed_field`.
pub(crate) fn validate_execution_settings(
    node_id: &str,
    node_type: &str,
    config: &Value,
) -> Option<NodeConfigIssue> {
    let mut blob = config.clone();
    if let Some(object) = blob.as_object_mut() {
        object.remove("generation");
    }
    serde_json::from_value::<LlmExecutionConfig>(blob)
        .err()
        .map(|e| {
            NodeConfigIssue::new(
                format!("nodes.{}.config", node_id),
                format!(
                    "Node '{}' ({}) has invalid execution settings: {}",
                    node_id, node_type, e
                ),
            )
        })
}
