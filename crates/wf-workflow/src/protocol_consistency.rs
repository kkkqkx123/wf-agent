use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use wf_llm::ProfileManager;
use wf_types::llm::ToolCallFormat;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::validation::ValidationError;

/// Extract the explicit tool call format from a node config:
/// - LLM nodes: top-level `tool_call_format` string
/// - AGENT_LOOP nodes: `inline_definition.config.tool_call_format` string
///
/// Returns `None` when the node declares no explicit format (the profile /
/// provider default then applies) or the value is not a canonical string.
fn node_tool_call_format(node: &wf_types::workflow_execution::WorkflowNode) -> Option<String> {
    let inner = match node.node_type.as_str() {
        "LLM" => node.inner.get("tool_call_format"),
        "AGENT_LOOP" => node
            .inner
            .get("inline_definition")
            .and_then(|v| v.get("config"))
            .and_then(|v| v.get("tool_call_format")),
        _ => return None,
    };
    inner
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
}

/// Referenced profile id of an LLM or AGENT_LOOP node.
fn node_profile_id(node: &wf_types::workflow_execution::WorkflowNode) -> Option<String> {
    match node.node_type.as_str() {
        "LLM" => node.inner.get("profile_id").and_then(|v| v.as_str()),
        "AGENT_LOOP" => node
            .inner
            .get("inline_definition")
            .and_then(|v| v.get("config"))
            .and_then(|v| v.get("profile_id"))
            .and_then(|v| v.as_str()),
        _ => None,
    }
    .map(String::from)
}

/// Validate tool call protocol consistency across all LLM and AGENT_LOOP
/// nodes in the graph:
/// - explicit `tool_call_format` values must be recognized
/// - all nodes that declare a format must agree on the same protocol
pub fn validate_protocol_consistency(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    validate_protocol_consistency_with(graph, None)
}

/// Profile-aware variant: additionally checks that every referenced
/// `profile_id` exists in the registered profile set and that a node's
/// explicit format is compatible with the referenced profile's format
/// (TS protocol-consistency-validator node-vs-profile check).
pub fn validate_protocol_consistency_with(
    graph: &WorkflowGraphStructure,
    profiles: Option<&ProfileManager>,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut declared_formats: HashSet<ToolCallFormat> = HashSet::new();

    for node in &graph.nodes {
        let Some(format) = node_tool_call_format(node) else {
            continue;
        };

        match ToolCallFormat::from_str(&format) {
            Ok(parsed) => {
                declared_formats.insert(parsed);
            }
            Err(_) => {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config.tool_call_format", node.id),
                    format!(
                        "Node '{}' ({}) has unsupported tool_call_format '{}', expected one of {}",
                        node.id,
                        node.node_type,
                        format,
                        ToolCallFormat::ALL
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
    }

    if declared_formats.len() > 1 {
        let mut formats: Vec<String> = declared_formats.iter().map(ToString::to_string).collect();
        formats.sort();
        errors.push(ValidationError::new(
            "nodes",
            format!(
                "Inconsistent tool call protocols across workflow nodes: {}. All nodes that reference LLM profiles should use the same protocol",
                formats.join(", ")
            ),
        ));
    }

    if let Some(registry) = profiles {
        errors.extend(validate_profile_references(graph, registry));
        errors.extend(validate_node_profile_compatibility(graph, registry));
    }

    errors
}

/// Every profile_id referenced by an LLM or AGENT_LOOP node must resolve.
fn validate_profile_references(
    graph: &WorkflowGraphStructure,
    profiles: &ProfileManager,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for node in &graph.nodes {
        let Some(profile_id) = node_profile_id(node) else {
            continue;
        };
        if profiles.get(&profile_id).is_none() {
            errors.push(ValidationError::new(
                format!("nodes.{}.config.profile_id", node.id),
                format!(
                    "Node '{}' ({}) references profile '{}' which is not registered",
                    node.id, node.node_type, profile_id
                ),
            ));
        }
    }

    errors
}

/// A node with an explicit format must be compatible with the format the
/// referenced profile is configured for (same rule as the runtime merge).
fn validate_node_profile_compatibility(
    graph: &WorkflowGraphStructure,
    profiles: &ProfileManager,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let profiles_by_id: HashMap<String, wf_types::llm::LlmProfile> = profiles
        .list()
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect();

    for node in &graph.nodes {
        let Some(profile_id) = node_profile_id(node) else {
            continue;
        };
        let Some(node_format) =
            node_tool_call_format(node).and_then(|f| ToolCallFormat::from_str(&f).ok())
        else {
            continue;
        };

        let Some(profile) = profiles_by_id.get(&profile_id) else {
            continue;
        };
        let Some(profile_format) = profile
            .tool_call_format
            .as_ref()
            .map(|config| config.format.clone())
        else {
            continue;
        };

        if node_format != profile_format && !node_format.is_compatible_with(&profile_format) {
            errors.push(ValidationError::new(
                format!("nodes.{}.config.tool_call_format", node.id),
                format!(
                    "Node '{}' tool call format \"{}\" is incompatible with profile '{}' format \"{}\"",
                    node.id, node_format, profile_id, profile_format
                ),
            ));
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_types::workflow_execution::WorkflowNode;

    fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn agent_loop_node(id: &str, format: &str) -> WorkflowNode {
        node(
            id,
            "AGENT_LOOP",
            serde_json::json!({
                "inline_definition": {
                    "id": id,
                    "name": id,
                    "config": {
                        "profile_id": "mock",
                        "tool_call_format": format,
                    }
                }
            }),
        )
    }

    fn graph_with(nodes: Vec<WorkflowNode>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges: vec![],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: None,
            end_node_ids: vec![],
        }
    }

    fn profile(id: &str, format: Option<&str>) -> wf_types::llm::LlmProfile {
        wf_types::llm::LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            provider: wf_types::llm::LlmProvider::OpenaiChat,
            model: "mock-model".to_string(),
            api_key: None,
            base_url: None,
            parameters: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_format: format
                .map(|f| wf_types::llm::ToolCallFormatConfig::from_format_str(f).unwrap()),
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
        }
    }

    fn registry(profiles: Vec<wf_types::llm::LlmProfile>) -> ProfileManager {
        let reg = ProfileManager::new();
        for p in profiles {
            let _ = reg.register(p);
        }
        reg
    }

    #[test]
    fn consistent_formats_pass() {
        let g = graph_with(vec![
            node(
                "l1",
                "LLM",
                serde_json::json!({"tool_call_format": "native"}),
            ),
            agent_loop_node("a1", "native"),
        ]);
        assert!(validate_protocol_consistency(&g).is_empty());
    }

    #[test]
    fn object_tool_call_format_is_not_canonical() {
        let g = graph_with(vec![node(
            "l1",
            "LLM",
            serde_json::json!({"toolCallFormat": {"format": "native"}}),
        )]);
        assert!(
            validate_protocol_consistency(&g).is_empty(),
            "non-canonical toolCallFormat object must be ignored"
        );
    }

    #[test]
    fn agent_loop_top_level_format_is_ignored() {
        let g = graph_with(vec![node(
            "a1",
            "AGENT_LOOP",
            serde_json::json!({"tool_call_format": "xml"}),
        )]);
        assert!(
            validate_protocol_consistency(&g).is_empty(),
            "AGENT_LOOP tool_call_format lives in inline_definition.config"
        );
    }

    #[test]
    fn inconsistent_formats_are_rejected() {
        let g = graph_with(vec![
            node(
                "l1",
                "LLM",
                serde_json::json!({"tool_call_format": "native"}),
            ),
            agent_loop_node("a1", "xml"),
        ]);
        let errors = validate_protocol_consistency(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("Inconsistent"));
    }

    #[test]
    fn unknown_format_is_rejected() {
        let g = graph_with(vec![node(
            "l1",
            "LLM",
            serde_json::json!({"tool_call_format": "yaml"}),
        )]);
        let errors = validate_protocol_consistency(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unsupported"));
    }

    #[test]
    fn nodes_without_explicit_format_are_ignored() {
        let g = graph_with(vec![
            node("l1", "LLM", serde_json::json!({"profile_id": "mock"})),
            node("v1", "VARIABLE", serde_json::json!({})),
        ]);
        assert!(validate_protocol_consistency(&g).is_empty());
    }

    #[test]
    fn unknown_agent_loop_format_is_rejected() {
        let g = graph_with(vec![agent_loop_node("a1", "yaml")]);
        let errors = validate_protocol_consistency(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unsupported"));
    }

    #[test]
    fn missing_profile_reference_is_rejected() {
        let reg = registry(vec![profile("known", None)]);
        let g = graph_with(vec![node(
            "l1",
            "LLM",
            serde_json::json!({"profile_id": "ghost"}),
        )]);
        let errors = validate_protocol_consistency_with(&g, Some(&reg));
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .message
            .contains("'ghost' which is not registered"));

        // Registered reference passes.
        let g = graph_with(vec![node(
            "l1",
            "LLM",
            serde_json::json!({"profile_id": "known"}),
        )]);
        assert!(validate_protocol_consistency_with(&g, Some(&reg)).is_empty());
    }

    #[test]
    fn agent_loop_profile_reference_checked() {
        let reg = registry(vec![profile("known", None)]);
        let g = graph_with(vec![node(
            "a1",
            "AGENT_LOOP",
            serde_json::json!({
                "inline_definition": {
                    "id": "a1",
                    "name": "a1",
                    "config": {"profile_id": "ghost"}
                }
            }),
        )]);
        let errors = validate_protocol_consistency_with(&g, Some(&reg));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("'ghost'"));
    }

    #[test]
    fn node_format_must_be_compatible_with_profile_format() {
        let reg = registry(vec![profile("xml-profile", Some("xml"))]);
        let g = graph_with(vec![node(
            "l1",
            "LLM",
            serde_json::json!({"profile_id": "xml-profile", "tool_call_format": "native"}),
        )]);
        let errors = validate_protocol_consistency_with(&g, Some(&reg));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("incompatible"));

        // Matching node format passes.
        let g = graph_with(vec![node(
            "l1",
            "LLM",
            serde_json::json!({"profile_id": "xml-profile", "tool_call_format": "xml"}),
        )]);
        assert!(validate_protocol_consistency_with(&g, Some(&reg)).is_empty());
    }

    #[test]
    fn json_formats_are_mutually_compatible() {
        let reg = registry(vec![profile("json-profile", Some("json_wrapped"))]);
        let g = graph_with(vec![node(
            "l1",
            "LLM",
            serde_json::json!({"profile_id": "json-profile", "tool_call_format": "json_raw"}),
        )]);
        assert!(validate_protocol_consistency_with(&g, Some(&reg)).is_empty());
    }
}
