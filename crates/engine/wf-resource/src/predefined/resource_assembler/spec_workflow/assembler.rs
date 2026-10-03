use std::collections::HashMap;

use serde_json::{json, Value};

use crate::resource_assembler::{
    AssemblerConfig, ResourceAssembler, ResourceAssemblerConfigField,
    ResourceAssemblerConfigFieldType, ResourceAssemblerMetadata, ResourceBundle,
};

use super::config::{
    SpecWorkflowConfig, DEFAULT_MAX_ITERATIONS, DEFAULT_SPEC_DIR, DEFAULT_WRITER_PROFILE_ID,
};
use super::prompts::build_stage_prompts;
use super::workflow::build_workflow;

pub const SPEC_WORKFLOW_RESOURCE_ASSEMBLER_ID: &str = "@standard/spec-workflow";

/// Spec-driven pipeline resource assembler: specify -> plan -> tasks ->
/// implement (goal-review delegate) -> converge -> archive
/// (id `@standard/spec-workflow`).
pub struct SpecWorkflowResourceAssembler;

impl SpecWorkflowResourceAssembler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SpecWorkflowResourceAssembler {
    fn default() -> Self {
        Self::new()
    }
}

fn string_field(description: &str, default: Option<Value>) -> ResourceAssemblerConfigField {
    ResourceAssemblerConfigField {
        r#type: ResourceAssemblerConfigFieldType::String,
        description: description.into(),
        default,
        required: None,
        allowed_functions: None,
    }
}

impl ResourceAssembler for SpecWorkflowResourceAssembler {
    fn metadata(&self) -> ResourceAssemblerMetadata {
        ResourceAssemblerMetadata {
            id: SPEC_WORKFLOW_RESOURCE_ASSEMBLER_ID.into(),
            name: "Spec Workflow".into(),
            version: "1.0.0".into(),
            description: "Spec-driven pipeline with review gates and goal-review implement loop"
                .into(),
            author: None,
            tags: Some(vec!["spec-driven".into(), "pipeline".into()]),
            category: Some("spec-driven".into()),
            dependencies: Some(vec![super::super::goal_review::assembler::GOAL_REVIEW_RESOURCE_ASSEMBLER_ID.into()]),
            configurable: Some(HashMap::from([
                (
                    "requirement".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::String,
                        description: "Requirement text the change implements".into(),
                        default: None,
                        required: Some(true),
                        allowed_functions: None,
                    },
                ),
                (
                    "change_id".into(),
                    string_field("Change directory name (derived from requirement)", None),
                ),
                (
                    "spec_dir".into(),
                    string_field(
                        "Spec artifact root directory",
                        Some(json!(DEFAULT_SPEC_DIR)),
                    ),
                ),
                (
                    "max_iterations".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Number,
                        description: "Maximum implement loop iterations".into(),
                        default: Some(json!(DEFAULT_MAX_ITERATIONS)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "spec_profile_id".into(),
                    string_field(
                        "LLM profile for the spec writer",
                        Some(json!(DEFAULT_WRITER_PROFILE_ID)),
                    ),
                ),
                (
                    "plan_profile_id".into(),
                    string_field(
                        "LLM profile for the plan writer",
                        Some(json!(DEFAULT_WRITER_PROFILE_ID)),
                    ),
                ),
                (
                    "tasks_profile_id".into(),
                    string_field(
                        "LLM profile for the task decomposer",
                        Some(json!(DEFAULT_WRITER_PROFILE_ID)),
                    ),
                ),
                (
                    "executor_tools".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Array,
                        description: "Override tools for the inline implementer agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_tools".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Array,
                        description: "Override tools for the inline spec reviewer agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "require_spec_gate".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Boolean,
                        description: "Require human approval of the spec before planning".into(),
                        default: Some(json!(true)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "require_plan_gate".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Boolean,
                        description: "Require human approval of the plan before task decomposition"
                            .into(),
                        default: Some(json!(true)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "use_subgraph_delegate".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Boolean,
                        description: "Delegate implementation to the goal-review workflow via SUBGRAPH instead of inline agents"
                            .into(),
                        default: Some(json!(true)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
            ])),
        }
    }

    fn assemble(&self, config: &Value) -> Result<ResourceBundle, String> {
        let config = SpecWorkflowConfig::parse(config)?;
        let mut builder = ResourceBundle::builder().workflow(build_workflow(&config)?);
        for prompt in build_stage_prompts(&config) {
            builder = builder.prompt(prompt);
        }
        Ok(builder.build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::node::StaticNodeType;
    use wf_types::workflow::EdgeType;

    fn node_types(bundle: &ResourceBundle) -> Vec<StaticNodeType> {
        bundle.workflows[0]
            .definition
            .nodes
            .iter()
            .map(|n| n.node_type.clone())
            .collect()
    }

    #[test]
    fn metadata_declares_spec_pipeline() {
        let assembler = SpecWorkflowResourceAssembler::new();
        let metadata = assembler.metadata();
        assert_eq!(metadata.id, "@standard/spec-workflow");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.category.as_deref(), Some("spec-driven"));
        let configurable = metadata.configurable.unwrap();
        assert!(configurable.contains_key("requirement"));
        assert!(configurable.contains_key("require_spec_gate"));
        assert!(configurable.contains_key("use_subgraph_delegate"));
    }

    #[test]
    fn assemble_builds_gated_pipeline_with_delegate() {
        let assembler = SpecWorkflowResourceAssembler::new();
        let bundle = assembler
            .assemble(&json!({"requirement": "Add dark mode"}))
            .unwrap();

        assert_eq!(bundle.workflows.len(), 1);
        assert_eq!(bundle.workflows[0].id, "@standard/spec-workflow");
        assert_eq!(bundle.prompts.len(), 4);

        let types = node_types(&bundle);
        assert_eq!(
            types,
            vec![
                StaticNodeType::Start,
                StaticNodeType::Llm,
                StaticNodeType::UserInteraction,
                StaticNodeType::Llm,
                StaticNodeType::UserInteraction,
                StaticNodeType::Llm,
                StaticNodeType::Route,
                StaticNodeType::LoopStart,
                StaticNodeType::Subgraph,
                StaticNodeType::LoopEnd,
                StaticNodeType::Llm,
                StaticNodeType::Route,
                StaticNodeType::Script,
                StaticNodeType::End,
            ]
        );

        let def = &bundle.workflows[0].definition;
        let edge_ids: Vec<&str> = def.edges.iter().map(|e| e.id.as_str()).collect();
        assert!(edge_ids.contains(&"e2b"));
        assert!(edge_ids.contains(&"e9"));
        let loop_back = def.edges.iter().find(|e| e.id == "e9").unwrap();
        assert_eq!(loop_back.source_node_id, "loop_end");
        assert_eq!(loop_back.target_node_id, "loop_start");
        assert_eq!(loop_back.r#type, EdgeType::Conditional);
    }

    #[test]
    fn assemble_without_gates_wires_direct_edges() {
        let assembler = SpecWorkflowResourceAssembler::new();
        let bundle = assembler
            .assemble(&json!({
                "requirement": "Add dark mode",
                "require_spec_gate": false,
                "require_plan_gate": false,
                "use_subgraph_delegate": false,
            }))
            .unwrap();

        let types = node_types(&bundle);
        assert!(!types.contains(&StaticNodeType::UserInteraction));
        assert_eq!(
            types,
            vec![
                StaticNodeType::Start,
                StaticNodeType::Llm,
                StaticNodeType::Llm,
                StaticNodeType::Llm,
                StaticNodeType::Route,
                StaticNodeType::LoopStart,
                StaticNodeType::AgentLoop,
                StaticNodeType::AgentLoop,
                StaticNodeType::LoopEnd,
                StaticNodeType::Llm,
                StaticNodeType::Route,
                StaticNodeType::Script,
                StaticNodeType::End,
            ]
        );

        let def = &bundle.workflows[0].definition;
        for node in def
            .nodes
            .iter()
            .filter(|n| n.node_type == StaticNodeType::AgentLoop)
        {
            let cfg = node.config.as_ref().unwrap();
            assert!(cfg.get("inline_definition").is_some());
        }
    }

    #[test]
    fn assemble_rejects_empty_requirement() {
        let assembler = SpecWorkflowResourceAssembler::new();
        assert!(assembler.assemble(&json!({})).is_err());
        assert!(assembler.assemble(&json!({"requirement": "  "})).is_err());
    }

    #[test]
    fn every_builtin_node_config_passes_engine_validation() {
        let assembler = SpecWorkflowResourceAssembler::new();
        for config in [
            json!({"requirement": "Add dark mode"}),
            json!({
                "requirement": "Add dark mode",
                "require_spec_gate": false,
                "require_plan_gate": false,
                "use_subgraph_delegate": false,
            }),
        ] {
            let bundle = assembler.assemble(&config).unwrap();
            let def = &bundle.workflows[0].definition;
            for node in &def.nodes {
                let issues = wf_config::processor::node_config::validate_node_config(
                    node.node_type.canonical_name(),
                    &node.id,
                    node.config.as_ref(),
                );
                assert!(
                    issues.is_empty(),
                    "node {} ({:?}): {:?}",
                    node.id,
                    node.node_type,
                    issues.iter().map(|i| &i.message).collect::<Vec<_>>()
                );
            }
        }
    }
}
