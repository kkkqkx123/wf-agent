use serde_json::Value;

use super::common::{
    field_not_in, field_path, is_valid_identifier, node_path, require_string, NodeConfigIssue,
};

pub(crate) fn validate_fork_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("FORK node '{}' has no config", node_id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    match config.get("fork_paths").and_then(|v| v.as_array()) {
        Some(arr) if arr.is_empty() => errors.push(NodeConfigIssue::new(
            field_path(node_id, "fork_paths"),
            format!(
                "FORK node '{}' must define a non-empty fork_paths array",
                node_id
            ),
        )),
        None => errors.push(NodeConfigIssue::new(
            field_path(node_id, "fork_paths"),
            format!(
                "FORK node '{}' must define a non-empty fork_paths array",
                node_id
            ),
        )),
        Some(paths) => {
            let mut seen_paths = std::collections::HashSet::new();
            for (idx, path) in paths.iter().enumerate() {
                let path = path.as_object().cloned().unwrap_or_default();
                let path = Value::Object(path);
                if let Some(err) = require_string(node_id, node_type, &path, "path_id", true) {
                    errors.push(NodeConfigIssue::new(
                        format!("nodes.{}.config.fork_paths[{}]", node_id, idx),
                        err.message,
                    ));
                } else if let Some(path_id) = path.get("path_id").and_then(|v| v.as_str()) {
                    if !is_valid_identifier(path_id) {
                        errors.push(NodeConfigIssue::new(
                            format!("nodes.{}.config.fork_paths[{}].path_id", node_id, idx),
                            format!(
                                "FORK node '{}' has invalid path_id '{}'; must start with a letter or '_' and contain only letters, digits or '_'",
                                node_id, path_id
                            ),
                        ));
                    } else if !seen_paths.insert(path_id.to_string()) {
                        errors.push(NodeConfigIssue::new(
                            format!("nodes.{}.config.fork_paths[{}].path_id", node_id, idx),
                            format!(
                                "FORK node '{}' has duplicate path_id '{}'",
                                node_id, path_id
                            ),
                        ));
                    }
                }
                if let Some(err) = require_string(node_id, node_type, &path, "child_node_id", true)
                {
                    errors.push(NodeConfigIssue::new(
                        format!("nodes.{}.config.fork_paths[{}]", node_id, idx),
                        err.message,
                    ));
                }
            }
        }
    }

    if let Some(err) = field_not_in(
        node_id,
        node_type,
        &config,
        "fork_strategy",
        &["serial", "parallel"],
    ) {
        errors.push(err);
    }
    errors
}

pub(crate) fn validate_join_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("JOIN node '{}' has no config", node_id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    match config.get("fork_path_ids").and_then(|v| v.as_array()) {
        Some(arr) if arr.is_empty() => errors.push(NodeConfigIssue::new(
            field_path(node_id, "fork_path_ids"),
            format!(
                "JOIN node '{}' must define a non-empty fork_path_ids array",
                node_id
            ),
        )),
        None => errors.push(NodeConfigIssue::new(
            field_path(node_id, "fork_path_ids"),
            format!(
                "JOIN node '{}' must define a non-empty fork_path_ids array",
                node_id
            ),
        )),
        Some(arr) => {
            let path_ids: Vec<&str> = arr
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .collect();
            if path_ids.len() != arr.len() {
                errors.push(NodeConfigIssue::new(
                    field_path(node_id, "fork_path_ids"),
                    format!(
                        "JOIN node '{}' has empty path ids; each entry must be a non-empty string",
                        node_id
                    ),
                ));
            }
            let mut seen = std::collections::HashSet::new();
            for pid in &path_ids {
                if !seen.insert(*pid) {
                    errors.push(NodeConfigIssue::new(
                        field_path(node_id, "fork_path_ids"),
                        format!("JOIN node '{}' has duplicate path id '{}'", node_id, pid),
                    ));
                    break;
                }
                if !is_valid_identifier(pid) {
                    errors.push(NodeConfigIssue::new(
                        field_path(node_id, "fork_path_ids"),
                        format!(
                            "JOIN node '{}' has invalid path id '{}'; must start with a letter or '_' and contain only letters, digits or '_'",
                            node_id, pid
                        ),
                    ));
                    break;
                }
            }
            if let Some(main) = config.get("main_path_id").and_then(|v| v.as_str()) {
                if !path_ids.contains(&main) {
                    errors.push(NodeConfigIssue::new(
                        field_path(node_id, "main_path_id"),
                        format!(
                            "JOIN node '{}' main_path_id '{}' is not in fork_path_ids",
                            node_id, main
                        ),
                    ));
                }
            }
            let strategy = config
                .get("join_strategy")
                .and_then(|v| v.as_str())
                .unwrap_or("wait_for_all");
            if let Some(threshold) = config.get("threshold").filter(|v| !v.is_null()) {
                match threshold.as_u64() {
                    Some(n) if n >= 1 => {
                        if strategy == "wait_for_n" && (n as usize) > path_ids.len() {
                            errors.push(NodeConfigIssue::new(
                                field_path(node_id, "threshold"),
                                format!(
                                    "JOIN node '{}' threshold {} exceeds fork_path_ids count {}; wait_for_n can wait for at most the declared paths",
                                    node_id,
                                    n,
                                    path_ids.len()
                                ),
                            ));
                        }
                    }
                    _ => errors.push(NodeConfigIssue::new(
                        field_path(node_id, "threshold"),
                        format!("JOIN node '{}' threshold must be an integer >= 1", node_id),
                    )),
                }
            }
        }
    }

    if let Some(err) = field_not_in(
        node_id,
        node_type,
        &config,
        "join_strategy",
        &["wait_for_all", "wait_for_any", "wait_for_n"],
    ) {
        errors.push(err);
    }
    errors
}
