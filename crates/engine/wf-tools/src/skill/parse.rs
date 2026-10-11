//! SKILL.md parsing: frontmatter extraction, minimal YAML and variable
//! substitution. Pure functions with no IO.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::{ToolError, ToolResult};
use wf_types::skill::SkillMetadata;

/// Replace `{{path}}` placeholders with the given values. Flat exact
/// matches win; otherwise spans resolve as dotted paths against the same
/// table, matching the resource model-bound engine. Non-string values
/// render with display coercion. Single-pass verbatim without rescanning.
pub fn substitute_variables(content: &str, variables: &HashMap<String, Value>) -> String {
    let mapped: HashMap<String, String> = variables
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                wf_common::template::value_to_display_string(value),
            )
        })
        .collect();
    wf_common::template::apply_template_variables_with_structured_context(
        content, &mapped, variables,
    )
}

pub(super) fn strip_frontmatter(content: &str) -> String {
    let body_match = content.strip_prefix("---").and_then(|rest| {
        rest.find("\n---").map(|idx| {
            let start = idx + "\n---".len();
            rest[start..]
                .strip_prefix('\n')
                .unwrap_or(&rest[start..])
                .to_string()
        })
    });
    match body_match {
        Some(body) => body.trim().to_string(),
        None => content.trim().to_string(),
    }
}

pub(super) fn parse_skill_md(
    content: &str,
    skill_dir: &std::path::Path,
) -> ToolResult<SkillMetadata> {
    let frontmatter = content
        .strip_prefix("---")
        .and_then(|rest| rest.find("\n---").map(|idx| &rest[..idx]))
        .ok_or_else(|| {
            ToolError::ValidationFailed(format!(
                "Missing YAML frontmatter in {}",
                skill_dir.display()
            ))
        })?;

    let fields = parse_yaml_frontmatter(frontmatter);

    let name = fields.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
        ToolError::ValidationFailed(format!(
            "Missing required field: name in {}",
            skill_dir.display()
        ))
    })?;

    if fields.get("description").and_then(|v| v.as_str()).is_none() {
        return Err(ToolError::ValidationFailed(format!(
            "Missing required field: description in {}",
            skill_dir.display()
        )));
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ToolError::ValidationFailed(format!(
            "Invalid skill name '{}': must be lowercase alphanumeric with hyphens only",
            name
        )));
    }

    Ok(SkillMetadata {
        name: name.to_string(),
        description: fields
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        when_to_use: fields
            .get("when_to_use")
            .and_then(|v| v.as_str())
            .map(String::from),
        version: fields
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from),
        license: fields
            .get("license")
            .and_then(|v| v.as_str())
            .map(String::from),
        allowed_tools: fields
            .get("allowedTools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
        metadata: fields
            .get("metadata")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.to_string()))
                    .collect()
            }),
    })
}

/// Minimal line-based YAML frontmatter parsing.
pub(super) fn parse_yaml_frontmatter(yaml: &str) -> HashMap<String, Value> {
    let mut result: HashMap<String, Value> = HashMap::new();
    let mut in_array = false;
    let mut array_key: Option<String> = None;

    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(item) = trimmed.strip_prefix("- ") {
            if in_array {
                if let Some(key) = &array_key {
                    let entry = result
                        .entry(key.clone())
                        .or_insert_with(|| Value::Array(vec![]));
                    if let Value::Array(arr) = entry {
                        arr.push(Value::String(item.trim().to_string()));
                    }
                }
            }
            continue;
        }

        let Some(colon_index) = trimmed.find(':') else {
            continue;
        };
        let key = trimmed[..colon_index].trim().to_string();
        let value = trimmed[colon_index + 1..].trim();

        if value.is_empty() || value == "[]" {
            array_key = Some(key.clone());
            in_array = true;
            if value == "[]" {
                result.insert(key, Value::Array(vec![]));
            }
        } else {
            result.insert(key, parse_yaml_value(value));
            in_array = false;
        }
    }

    result
}

fn parse_yaml_value(value: &str) -> Value {
    let value = value.trim();
    if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
        || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
    {
        return Value::String(value[1..value.len() - 1].to_string());
    }

    match value {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        "null" | "~" => return Value::Null,
        _ => {}
    }

    if let Ok(num) = value.parse::<i64>() {
        return Value::Number(num.into());
    }
    if let Ok(num) = value.parse::<f64>() {
        if let Some(number) = serde_json::Number::from_f64(num) {
            return Value::Number(number);
        }
    }

    Value::String(value.to_string())
}
