//! Thin application-facing facade over `wf-config`.
//!
//! `wf-api` does not re-implement config semantics; this module re-exposes the
//! `wf-config` entry points grouped by category — parse (TOML/JSON), validate,
//! transform, export and infrastructure assembly — so server/CLI layers can
//! consume a single config surface.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use wf_types::agent::AgentDefinition;
use wf_types::llm::LlmProfile;
use wf_types::node::BaseStaticNode;
use wf_types::workflow::edge::Edge;
use wf_types::workflow::WorkflowDefinition;

/// Parse: config file / text deserialization.
pub use wf_config::parser::{
    config_format_from_path, parse_config, parse_config_file, parse_json, parse_toml, ConfigFormat,
};

/// Validate: per-entity and per-node config validation.
pub use wf_config::processor::agent_loop::{
    validate_agent_definition, validate_agent_definition_with_profiles,
};
pub use wf_config::processor::checkpoint::validate_checkpoint_config;
pub use wf_config::processor::file_checkpoint::validate_file_checkpoint_config;
pub use wf_config::processor::node_config::{
    validate_all_node_configs, validate_node_config, NodeConfigIssue,
};
pub use wf_config::processor::node_template::validate_node_template;
pub use wf_config::processor::prompt::validate_prompt_template;
pub use wf_config::processor::sandbox_global::validate_sandbox_global;
pub use wf_config::processor::script::validate_script_executor;
pub use wf_config::processor::script_flow::validate_script_flow;
pub use wf_config::processor::script_interactive::validate_interactive_script;
pub use wf_config::processor::trigger::validate_trigger_template;
pub use wf_config::processor::workflow::validate_workflow_definition;
pub use wf_config::validator::{
    validate_all, validate_array_not_empty, validate_email, validate_enum, validate_length,
    validate_max, validate_min, validate_no_intersection, validate_not_empty, validate_pattern,
    validate_range, validate_required, validate_url,
};

/// Transform: canonical node/edge conversion and parameter substitution.
pub use wf_config::processor::substitute::{
    substitute_in_struct, substitute_parameters_in_value, substitute_string,
};
pub use wf_config::processor::workflow::{
    transform_edges, transform_nodes, WorkflowEdgeConfig, WorkflowNodeConfig,
};

/// Export: canonicalization of entity configs.
pub use wf_config::processor::agent_loop::export_agent_loop_config;
pub use wf_config::processor::llm_profile::export_llm_profile;
pub use wf_config::processor::node_template::export_node_template;
pub use wf_config::processor::prompt::export_prompt_template;
pub use wf_config::processor::sandbox_global::export_sandbox_global;
pub use wf_config::processor::script_flow::export_script_flow;
pub use wf_config::processor::script_interactive::export_interactive_script;
pub use wf_config::processor::trigger::export_trigger_template;

/// Infrastructure assembly (project/global config loading, env overrides).
pub use wf_config::layered::{load_layered_config, load_layered_config_sync, merge_toml_values};
pub use wf_config::orchestrator::{
    AssembledConfig, ConfigOrchestrator, ConfigOrchestratorBuilder, ConfigOverrides,
};
pub use wf_config::ConfigError;

/// Parse a workflow definition from TOML or JSON text.
pub fn parse_workflow(content: &str, format: ConfigFormat) -> crate::ApiResult<WorkflowDefinition> {
    parse_config(content, format).map_err(Into::into)
}

/// Parse a workflow definition from a config file.
pub fn parse_workflow_file(path: &Path) -> crate::ApiResult<WorkflowDefinition> {
    parse_config_file(path).map_err(Into::into)
}

/// Validate a workflow definition (config-level checks).
pub fn validate_workflow(definition: &WorkflowDefinition) -> crate::ApiResult<()> {
    validate_workflow_definition(definition).map_err(Into::into)
}

/// Validate an LLM profile.
pub fn validate_llm_profile(profile: &LlmProfile) -> crate::ApiResult<()> {
    wf_config::processor::llm_profile::validate_llm_profile(profile).map_err(Into::into)
}

/// Validate an agent definition.
pub fn validate_agent(definition: &AgentDefinition) -> crate::ApiResult<()> {
    validate_agent_definition(definition).map_err(Into::into)
}

/// Validate a single node config by node type.
pub fn validate_node(
    node_type: &str,
    node_id: &str,
    config: Option<&Value>,
) -> crate::ApiResult<()> {
    let issues = validate_node_config(node_type, node_id, config);
    if issues.is_empty() {
        Ok(())
    } else {
        let detail = issues
            .iter()
            .map(|i| format!("{}: {}", i.field, i.message))
            .collect::<Vec<_>>()
            .join("; ");
        Err(crate::ApiError::Validation(detail))
    }
}

/// Transform declarative node configs into static nodes.
pub fn transform_workflow_nodes(nodes: &[WorkflowNodeConfig]) -> Vec<BaseStaticNode> {
    transform_nodes(nodes)
}

/// Transform declarative edge configs into edges.
pub fn transform_workflow_edges(edges: &[WorkflowEdgeConfig]) -> Vec<Edge> {
    transform_edges(edges)
}

/// Serialize a config value as pretty JSON.
pub fn export_json<T: Serialize>(value: &T) -> crate::ApiResult<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| crate::ApiError::execution(format!("failed to serialize config: {e}")))
}

/// Serialize a config value as pretty TOML.
pub fn export_toml<T: Serialize>(value: &T) -> crate::ApiResult<String> {
    toml::to_string_pretty(value)
        .map_err(|e| crate::ApiError::execution(format!("failed to serialize config: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_workflow_from_json() {
        let content = r#"{
            "id": "wf-1",
            "name": "Test",
            "nodes": [
                {"id": "start", "node_type": "START"},
                {"id": "end", "node_type": "END"}
            ],
            "edges": [
                {"id": "e1", "source_node_id": "start", "target_node_id": "end", "type": "DEFAULT"}
            ],
            "created_at": 0,
            "updated_at": 0
        }"#;
        let wf = parse_workflow(content, ConfigFormat::Json).unwrap();
        assert_eq!(wf.id, "wf-1");
        assert_eq!(wf.nodes.len(), 2);
        assert!(validate_workflow(&wf).is_ok());
    }

    #[test]
    fn rejects_malformed_json() {
        let err = parse_workflow("{not json", ConfigFormat::Json).unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));
    }

    #[test]
    fn transforms_declarative_nodes_and_edges() {
        let nodes = vec![WorkflowNodeConfig {
            id: "n1".into(),
            node_type: "LLM".into(),
            name: None,
            description: None,
            config: Some(serde_json::json!({"profile_id": "mock"})),
        }];
        let built = transform_workflow_nodes(&nodes);
        assert_eq!(built.len(), 1);
        assert_eq!(built[0].node_type, wf_types::node::StaticNodeType::Llm);

        let edges = vec![WorkflowEdgeConfig {
            id: None,
            source_node_id: Some("n1".into()),
            target_node_id: Some("n2".into()),
            condition: None,
            label: None,
            description: None,
            weight: None,
        }];
        let built = transform_workflow_edges(&edges);
        assert_eq!(built.len(), 1);
        assert_eq!(built[0].r#type, wf_types::workflow::EdgeType::Default);
    }

    #[test]
    fn exports_json_and_toml() {
        let value = serde_json::json!({"name": "test", "nested": {"enabled": true}});
        let json = export_json(&value).unwrap();
        assert!(json.contains("\"name\": \"test\""));

        let toml_out = export_toml(&value).unwrap();
        assert!(toml_out.contains("enabled = true"));
    }

    #[test]
    fn validates_node_config() {
        assert!(validate_node(
            "LLM",
            "n1",
            Some(&serde_json::json!({"profile_id": "mock"}))
        )
        .is_ok());
        assert!(validate_node("LLM", "n1", Some(&serde_json::json!({}))).is_err());
    }
}
