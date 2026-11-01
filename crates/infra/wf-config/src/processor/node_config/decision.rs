use serde_json::Value;
use std::collections::HashSet;

use super::common::{field_path, node_path, NodeConfigIssue};

fn question_type_of(question: &serde_json::Map<String, Value>) -> Option<String> {
    question
        .get("question_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase())
}

pub(crate) fn validate_decision_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("DECISION node '{node_id}' has no config"),
        ));
        return errors;
    };
    let config_value = Value::Object(config.clone());

    let has_profile = config_value
        .get("decision_profile_id")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty());
    let has_inline = config_value
        .get("base_url")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty())
        && config_value
            .get("model")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty());
    if !has_profile && !has_inline {
        errors.push(NodeConfigIssue::new(
            field_path(node_id, "connection"),
            format!(
                "Node '{node_id}' ({node_type}) must define decision_profile_id or inline base_url and model"
            ),
        ));
    }
    if let Some(protocol) = config_value.get("protocol").and_then(|v| v.as_str()) {
        let normalized = protocol.to_lowercase();
        if !matches!(
            normalized.as_str(),
            "openai" | "jev" | "typesafe" | "systemone"
        ) {
            errors.push(NodeConfigIssue::new(
                field_path(node_id, "protocol"),
                format!(
                    "Node '{node_id}' ({node_type}) field 'protocol' has invalid value '{protocol}', expected one of openai, jev"
                ),
            ));
        }
    }
    if let Some(timeout) = config_value.get("timeout_secs") {
        let invalid = match timeout {
            Value::Number(n) => n.as_u64().is_none_or(|v| v == 0),
            _ => true,
        };
        if invalid {
            errors.push(NodeConfigIssue::new(
                field_path(node_id, "timeout_secs"),
                format!(
                    "Node '{node_id}' ({node_type}) field 'timeout_secs' must be an integer >= 1"
                ),
            ));
        }
    }

    let questions = config_value.get("questions").and_then(|v| v.as_array());
    let Some(questions) = questions else {
        errors.push(NodeConfigIssue::new(
            field_path(node_id, "questions"),
            format!("Node '{node_id}' ({node_type}) must define a non-empty questions array"),
        ));
        return errors;
    };
    if questions.is_empty() || questions.len() > 400 {
        errors.push(NodeConfigIssue::new(
            field_path(node_id, "questions"),
            format!("Node '{node_id}' ({node_type}) field 'questions' must contain 1 to 400 items"),
        ));
        return errors;
    }
    for (index, question) in questions.iter().enumerate() {
        let prefix = format!("questions[{index}]");
        let Some(obj) = question.as_object() else {
            errors.push(NodeConfigIssue::new(
                field_path(node_id, &prefix),
                format!("Node '{node_id}' ({node_type}) field '{prefix}' must be an object"),
            ));
            continue;
        };
        let kind = question_type_of(obj);
        match kind.as_deref() {
            Some("predicate") => {
                validate_instructions(node_id, node_type, obj, &prefix, &mut errors);
            }
            Some("choice") => {
                validate_instructions(node_id, node_type, obj, &prefix, &mut errors);
                match obj.get("choices").and_then(|v| v.as_array()) {
                    None => errors.push(NodeConfigIssue::new(
                        field_path(node_id, &format!("{prefix}.choices")),
                        format!(
                            "Node '{node_id}' ({node_type}) field '{prefix}.choices' is required for choice questions"
                        ),
                    )),
                    Some(choices) => {
                        if choices.len() < 2 || choices.len() > 255 {
                            errors.push(NodeConfigIssue::new(
                                field_path(node_id, &format!("{prefix}.choices")),
                                format!(
                                    "Node '{node_id}' ({node_type}) field '{prefix}.choices' must contain 2 to 255 options"
                                ),
                            ));
                        } else {
                            let mut seen = HashSet::new();
                            for choice in choices {
                                let key = match choice.get("value") {
                                    Some(Value::String(s)) => format!("str:{s}"),
                                    Some(Value::Bool(b)) => format!("bool:{b}"),
                                    Some(Value::Number(n)) => format!("num:{n}"),
                                    _ => {
                                        errors.push(NodeConfigIssue::new(
                                            field_path(node_id, &format!("{prefix}.choices.value")),
                                            format!(
                                                "Node '{node_id}' ({node_type}) choice values must be strings, booleans or numbers"
                                            ),
                                        ));
                                        continue;
                                    }
                                };
                                if !seen.insert(key) {
                                    errors.push(NodeConfigIssue::new(
                                        field_path(node_id, &format!("{prefix}.choices")),
                                        format!(
                                            "Node '{node_id}' ({node_type}) choice values must be unique"
                                        ),
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Some("score") => {
                validate_instructions(node_id, node_type, obj, &prefix, &mut errors);
                match obj.get("levels").and_then(|v| v.as_array()) {
                    None | Some(_)
                        if obj
                            .get("levels")
                            .and_then(|v| v.as_array())
                            .is_none_or(|a| a.is_empty()) =>
                    {
                        errors.push(NodeConfigIssue::new(
                            field_path(node_id, &format!("{prefix}.levels")),
                            format!(
                                "Node '{node_id}' ({node_type}) field '{prefix}.levels' must be a non-empty array for score questions"
                            ),
                        ));
                    }
                    _ => {}
                }
            }
            Some("box") => {
                validate_instructions(node_id, node_type, obj, &prefix, &mut errors);
            }
            _ => {
                errors.push(NodeConfigIssue::new(
                    field_path(node_id, &format!("{prefix}.question_type")),
                    format!(
                        "Node '{node_id}' ({node_type}) field '{prefix}.question_type' must be one of predicate, choice, score, box"
                    ),
                ));
            }
        }
    }
    validate_state_sources(node_id, node_type, &config_value, &mut errors);
    validate_image_assets(node_id, node_type, &config_value, &mut errors);
    errors
}

fn validate_state_sources(
    node_id: &str,
    node_type: &str,
    config: &Value,
    errors: &mut Vec<NodeConfigIssue>,
) {
    let mut sources = Vec::new();
    for field in [
        "state_variable",
        "state_expression",
        "state_json",
        "state_messages",
        "state_parts",
    ] {
        let present = match config.get(field) {
            None | Some(Value::Null) => false,
            Some(Value::String(s)) => !s.trim().is_empty(),
            Some(Value::Array(items)) => !items.is_empty(),
            Some(_) => true,
        };
        if present {
            sources.push(field);
        }
    }
    if sources.len() > 1 {
        errors.push(NodeConfigIssue::new(
            field_path(node_id, "state"),
            format!(
                "Node '{node_id}' ({node_type}) must use at most one state source; found {}",
                sources.join(", ")
            ),
        ));
    }
    if let Some(messages) = config.get("state_messages") {
        if !messages.is_null() && messages.as_array().is_none() {
            errors.push(NodeConfigIssue::new(
                field_path(node_id, "state_messages"),
                format!(
                    "Node '{node_id}' ({node_type}) field 'state_messages' must be an array of decision messages"
                ),
            ));
        }
    }
    if let Some(parts) = config.get("state_parts") {
        if !parts.is_null() && parts.as_array().is_none() {
            errors.push(NodeConfigIssue::new(
                field_path(node_id, "state_parts"),
                format!(
                    "Node '{node_id}' ({node_type}) field 'state_parts' must be an array of content parts"
                ),
            ));
        }
    }
}

fn validate_image_assets(
    node_id: &str,
    node_type: &str,
    config: &Value,
    errors: &mut Vec<NodeConfigIssue>,
) {
    let Some(assets) = config.get("image_assets").and_then(|v| v.as_array()) else {
        return;
    };
    let mut seen = HashSet::new();
    for (index, asset) in assets.iter().enumerate() {
        let prefix = format!("image_assets[{index}]");
        let Some(obj) = asset.as_object() else {
            errors.push(NodeConfigIssue::new(
                field_path(node_id, &prefix),
                format!("Node '{node_id}' ({node_type}) field '{prefix}' must be an object"),
            ));
            continue;
        };
        match obj.get("alias").and_then(|v| v.as_str()) {
            Some(alias) if !alias.trim().is_empty() => {
                if !seen.insert(alias.to_string()) {
                    errors.push(NodeConfigIssue::new(
                        field_path(node_id, &format!("{prefix}.alias")),
                        format!(
                            "Node '{node_id}' ({node_type}) image asset aliases must be unique"
                        ),
                    ));
                }
            }
            _ => errors.push(NodeConfigIssue::new(
                field_path(node_id, &format!("{prefix}.alias")),
                format!(
                    "Node '{node_id}' ({node_type}) field '{prefix}.alias' must be a non-empty string"
                ),
            )),
        }
        let has_base64 = obj
            .get("base64")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty());
        let has_variable = obj
            .get("variable")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty());
        if has_base64 == has_variable {
            errors.push(NodeConfigIssue::new(
                field_path(node_id, &prefix),
                format!(
                    "Node '{node_id}' ({node_type}) field '{prefix}' must define exactly one of base64 or variable"
                ),
            ));
        }
    }
}

fn validate_instructions(
    node_id: &str,
    node_type: &str,
    question: &serde_json::Map<String, Value>,
    prefix: &str,
    errors: &mut Vec<NodeConfigIssue>,
) {
    if let Some(instructions) = question.get("instructions") {
        let empty = match instructions {
            Value::Null => true,
            Value::String(s) => s.trim().is_empty(),
            Value::Array(items) => items.is_empty(),
            Value::Object(map) => map.is_empty(),
            _ => false,
        };
        if empty {
            errors.push(NodeConfigIssue::new(
                field_path(node_id, &format!("{prefix}.instructions")),
                format!(
                    "Node '{node_id}' ({node_type}) field '{prefix}.instructions' must not be empty"
                ),
            ));
        }
    }
}
