//! Unified node template handling at the composition boundary.
//!
//! Nodes carry no template reference in the static schema, so the reference
//! travels inside the node config blob under `template_id` (or the legacy
//! `node_template_id` key). Template `default_config` supplies defaults under
//! explicit node keys: explicit node fields always win. Only JSON objects
//! merge; non-object defaults apply solely when the node has no config.

use wf_core::registry::Registry;
use wf_resource::registry::ResourceRegistries;
use wf_types::workflow::NodeTemplate;

/// Config keys inspected for a node template reference, in order.
pub const NODE_TEMPLATE_KEYS: &[&str] = &["template_id", "node_template_id"];

/// Extract a template reference from a node config blob.
pub fn extract_template_id(config: Option<&serde_json::Value>) -> Option<String> {
    let config = config?;
    let map = config.as_object()?;
    NODE_TEMPLATE_KEYS.iter().find_map(|key| {
        map.get(*key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    })
}

/// Deep merge template defaults under explicit node config. Explicit leaf
/// values win; nested objects merge recursively.
pub fn apply_node_template_defaults(
    explicit: &serde_json::Value,
    template: &NodeTemplate,
) -> serde_json::Value {
    let Some(defaults) = template.default_config.as_ref() else {
        return explicit.clone();
    };
    merge_under(explicit, defaults)
}

fn merge_under(explicit: &serde_json::Value, defaults: &serde_json::Value) -> serde_json::Value {
    match (explicit, defaults) {
        (serde_json::Value::Object(explicit_map), serde_json::Value::Object(default_map)) => {
            let mut merged = default_map.clone();
            for (key, value) in explicit_map {
                if key == "template_id" || key == "node_template_id" {
                    continue;
                }
                let merged_value = match merged.get(key) {
                    Some(base) => merge_under(value, base),
                    None => value.clone(),
                };
                merged.insert(key.clone(), merged_value);
            }
            serde_json::Value::Object(merged)
        }
        _ => explicit.clone(),
    }
}

/// Apply registered node template defaults to every node in a workflow
/// definition whose config carries a template reference. Nodes without a
/// reference or with an unknown id are left untouched.
pub fn apply_templates_to_definition(
    definition: &mut wf_types::workflow::WorkflowDefinition,
    regs: &ResourceRegistries,
) {
    for node in &mut definition.nodes {
        let template_id = extract_template_id(node.config.as_ref());
        let Some(template_id) = template_id else {
            continue;
        };
        let explicit = node.config.clone().unwrap_or(serde_json::Value::Null);
        node.config = Some(resolve_node_config(regs, &template_id, &explicit));
    }
}

/// Resolve a node config against the registry: unknown template ids leave the
/// config untouched, matching the agent unknown-id behavior.
pub fn resolve_node_config(
    regs: &ResourceRegistries,
    template_id: &str,
    explicit: &serde_json::Value,
) -> serde_json::Value {
    let Some(template) = regs
        .node_templates
        .get(template_id)
        .map(|t| t.as_ref().clone())
    else {
        return explicit.clone();
    };
    apply_node_template_defaults(explicit, &template)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::MutableRegistry;

    fn node_template() -> NodeTemplate {
        NodeTemplate {
            id: "nt-llm".into(),
            name: "llm".into(),
            description: String::new(),
            node_type: "LLM".into(),
            default_config: Some(serde_json::json!({
                "profile_id": "default",
                "generation": {"temperature": 0.2},
            })),
        }
    }

    #[test]
    fn explicit_fields_win_over_template_defaults() {
        let explicit = serde_json::json!({"profile_id": "mock"});
        let merged = apply_node_template_defaults(&explicit, &node_template());
        assert_eq!(merged["profile_id"], "mock");
        assert_eq!(merged["generation"]["temperature"], 0.2);
    }

    #[test]
    fn template_reference_keys_are_recognized() {
        let config = serde_json::json!({"template_id": "nt-llm"});
        assert_eq!(
            extract_template_id(Some(&config)).as_deref(),
            Some("nt-llm")
        );
        let legacy = serde_json::json!({"node_template_id": "nt-llm"});
        assert_eq!(
            extract_template_id(Some(&legacy)).as_deref(),
            Some("nt-llm")
        );
        assert!(extract_template_id(None).is_none());
    }

    #[test]
    fn unknown_template_id_leaves_config_untouched() {
        let regs = ResourceRegistries::new();
        let explicit = serde_json::json!({"profile_id": "mock"});
        let resolved = resolve_node_config(&regs, "missing", &explicit);
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn registered_template_supplies_defaults() {
        let regs = ResourceRegistries::new();
        regs.node_templates
            .register("nt-llm".into(), std::sync::Arc::new(node_template()))
            .expect("register");
        let explicit = serde_json::json!({"profile_id": "mock"});
        let resolved = resolve_node_config(&regs, "nt-llm", &explicit);
        assert_eq!(resolved["profile_id"], "mock");
        assert_eq!(resolved["generation"]["temperature"], 0.2);
    }

    #[test]
    fn definition_nodes_without_reference_are_untouched() {
        use wf_types::node::{BaseStaticNode, StaticNodeType};
        let regs = ResourceRegistries::new();
        regs.node_templates
            .register("nt-llm".into(), std::sync::Arc::new(node_template()))
            .expect("register");
        let mut definition = wf_types::workflow::WorkflowDefinition {
            id: "wf-1".into(),
            name: "wf".into(),
            description: None,
            r#type: None,
            version: None,
            nodes: vec![
                BaseStaticNode {
                    id: "n1".into(),
                    node_type: StaticNodeType::Llm,
                    name: None,
                    description: None,
                    config: Some(serde_json::json!({
                        "template_id": "nt-llm",
                        "profile_id": "mock",
                    })),
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "n2".into(),
                    node_type: StaticNodeType::Llm,
                    name: None,
                    description: None,
                    config: Some(serde_json::json!({"profile_id": "keep"})),
                    execution_config: None,
                },
            ],
            edges: Vec::new(),
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            available_tools: None,
            hooks: None,
        };
        apply_templates_to_definition(&mut definition, &regs);
        assert_eq!(
            definition.nodes[0].config.as_ref().unwrap()["profile_id"],
            "mock"
        );
        assert_eq!(
            definition.nodes[0].config.as_ref().unwrap()["generation"]["temperature"],
            0.2
        );
        assert_eq!(
            definition.nodes[1].config.as_ref().unwrap()["profile_id"],
            "keep"
        );
    }
}
