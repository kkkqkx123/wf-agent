use std::collections::HashMap;

use serde_json::{json, Value};

use crate::resource_assembler::{
    AssemblerConfig, ResourceAssembler, ResourceAssemblerConfigField,
    ResourceAssemblerConfigFieldType, ResourceAssemblerMetadata, ResourceBundle,
};

use super::config::{GoalReviewConfig, DEFAULT_MAX_ITERATIONS, DEFAULT_PLANNER_PROFILE_ID};
use super::workflow::{build_planner_prompt, build_workflow};

pub const GOAL_REVIEW_RESOURCE_ASSEMBLER_ID: &str = "@standard/goal-review-agent";

/// All built-in resource assemblers, registered into the bundle registry during the
/// resource registration pipeline.
pub fn builtin_resource_assemblers() -> Vec<Box<dyn ResourceAssembler>> {
    vec![Box::new(GoalReviewResourceAssembler::new())]
}

/// Goal-driven review loop resource assembler: planner -> executor -> reviewer -> loop
/// check (id `@standard/goal-review-agent`).
pub struct GoalReviewResourceAssembler;

impl GoalReviewResourceAssembler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GoalReviewResourceAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceAssembler for GoalReviewResourceAssembler {
    fn metadata(&self) -> ResourceAssemblerMetadata {
        ResourceAssemblerMetadata {
            id: GOAL_REVIEW_RESOURCE_ASSEMBLER_ID.into(),
            name: "Goal Review Agent".into(),
            version: "1.0.0".into(),
            description: "Goal-driven review loop with planner, executor, and reviewer agents"
                .into(),
            author: None,
            tags: Some(vec![
                "review".into(),
                "goal-driven".into(),
                "agent-loop".into(),
            ]),
            category: Some("code-review".into()),
            dependencies: None,
            configurable: Some(HashMap::from([
                (
                    "max_iterations".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Number,
                        description: "Maximum review loop iterations".into(),
                        default: Some(json!(DEFAULT_MAX_ITERATIONS)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "planner_profile_id".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::String,
                        description: "LLM profile for task planning (lightweight model)".into(),
                        default: Some(json!(DEFAULT_PLANNER_PROFILE_ID)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "executor_profile_id".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::String,
                        description: "LLM profile for executor (default from template)".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_profile_id".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::String,
                        description: "LLM profile for reviewer (default from template)".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "planner_system_prompt".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::String,
                        description: "Custom system prompt for the task planner".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "executor_system_prompt".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::String,
                        description: "Override system prompt for the executor agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_system_prompt".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::String,
                        description: "Override system prompt for the reviewer agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "executor_tools".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Array,
                        description: "Override tools for the executor agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_tools".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Array,
                        description:
                            "Override tools for the reviewer agent (read-only recommended)".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
            ])),
        }
    }

    fn assemble(&self, config: &Value) -> Result<ResourceBundle, String> {
        let config = GoalReviewConfig::parse(config)?;
        Ok(ResourceBundle::builder()
            .workflow(build_workflow(&config)?)
            .prompt(build_planner_prompt(&config))
            .build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::agent::AgentDefinition;
    use wf_types::node::StaticNodeType;
    use wf_types::workflow::EdgeType;

    #[test]
    fn metadata_matches_ts_schema() {
        let assembler = GoalReviewResourceAssembler::new();
        let metadata = assembler.metadata();
        assert_eq!(metadata.id, "@standard/goal-review-agent");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.category.as_deref(), Some("code-review"));
        let configurable = metadata.configurable.unwrap();
        assert_eq!(configurable.len(), 9);
        assert!(configurable.contains_key("max_iterations"));
        assert!(configurable.contains_key("reviewer_tools"));
    }

    #[test]
    fn assemble_builds_ts_equivalent_bundle() {
        let assembler = GoalReviewResourceAssembler::new();
        let bundle = assembler
            .assemble(&json!({"root_requirement": "review this"}))
            .unwrap();

        assert_eq!(bundle.workflows.len(), 1);
        assert_eq!(bundle.prompts.len(), 1);
        assert_eq!(bundle.prompts[0].id, "prompt.goal-review.planner");

        let wf = &bundle.workflows[0];
        assert_eq!(wf.id, "@standard/goal-review-agent-workflow");

        let def = &wf.definition;
        assert_eq!(def.nodes.len(), 7);
        assert_eq!(def.edges.len(), 7);

        // Node types match the expected layout.
        let types: Vec<StaticNodeType> = def.nodes.iter().map(|n| n.node_type.clone()).collect();
        assert_eq!(
            types,
            vec![
                StaticNodeType::Start,
                StaticNodeType::LoopStart,
                StaticNodeType::Llm,
                StaticNodeType::AgentLoop,
                StaticNodeType::AgentLoop,
                StaticNodeType::LoopEnd,
                StaticNodeType::End,
            ]
        );

        // Loop wiring matches the break/continue semantics.
        let loop_end = def
            .nodes
            .iter()
            .find(|n| n.id == "loop_end")
            .expect("loop_end node");
        let loop_cfg = loop_end.config.as_ref().unwrap();
        assert_eq!(
            loop_cfg["break_condition"].as_str().unwrap(),
            "or(eq(status,\"completed\"),eq(status,\"stuck\"))"
        );
        assert_eq!(
            loop_cfg["loop_start_node_id"].as_str().unwrap(),
            "loop_start"
        );

        // The conditional loop-back edge targets the LOOP_START node
        // (Rust engine loop-back convention).
        let loop_back = def
            .edges
            .iter()
            .find(|e| e.id == "e7")
            .expect("loop-back edge");
        assert_eq!(loop_back.source_node_id, "loop_end");
        assert_eq!(loop_back.target_node_id, "loop_start");
        assert_eq!(loop_back.r#type, EdgeType::Conditional);

        // AGENT_LOOP nodes carry the merged inline definitions.
        for node in def
            .nodes
            .iter()
            .filter(|n| n.node_type == StaticNodeType::AgentLoop)
        {
            let cfg = node.config.as_ref().unwrap();
            assert!(cfg.get("inline_definition").is_some());
        }

        // The workflow declares 5 variables.
        let variables = def.variables.as_ref().unwrap();
        assert_eq!(variables.len(), 5);
        let names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "rootRequirement",
                "status",
                "complete",
                "judges",
                "iterationCount"
            ]
        );
    }

    #[test]
    fn assemble_applies_inline_overrides() {
        let assembler = GoalReviewResourceAssembler::new();
        let bundle = assembler
            .assemble(&json!({
                "root_requirement": "review",
                "executor_profile_id": "custom-exec",
                "executor_max_iterations": 42,
                "executor_tools": ["read_file", "grep"],
                "reviewer_system_prompt": "be strict",
            }))
            .unwrap();

        let def = &bundle.workflows[0].definition;
        let executor = def.nodes.iter().find(|n| n.id == "executor_agent").unwrap();
        let exec_cfg = executor.config.as_ref().unwrap()["inline_definition"].clone();
        let exec_def: AgentDefinition = serde_json::from_value(exec_cfg).unwrap();
        let exec_config = exec_def.config.unwrap();
        assert_eq!(exec_config.profile_id.as_deref(), Some("custom-exec"));
        assert_eq!(exec_config.max_iterations, Some(42));
        assert_eq!(
            exec_config.available_tools.unwrap().available,
            vec!["read_file", "grep"]
        );

        let reviewer = def.nodes.iter().find(|n| n.id == "reviewer_agent").unwrap();
        let rev_cfg = reviewer.config.as_ref().unwrap()["inline_definition"].clone();
        let rev_def: AgentDefinition = serde_json::from_value(rev_cfg).unwrap();
        assert_eq!(
            rev_def.config.unwrap().system_prompt.as_deref(),
            Some("be strict")
        );
    }
}
