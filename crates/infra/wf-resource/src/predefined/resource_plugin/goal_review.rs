use std::collections::HashMap;

use serde_json::{json, Value};
use wf_types::message::Message;

use crate::resource_plugin::{
    ResourceBundle, ResourcePlugin, ResourcePluginConfigField, ResourcePluginConfigFieldType,
    ResourcePluginMetadata,
};

use super::workflow::{build_planner_prompt, build_workflow};

pub const GOAL_REVIEW_RESOURCE_PLUGIN_ID: &str = "@standard/goal-review-agent";
pub const GOAL_REVIEW_WORKFLOW_ID: &str = "@standard/goal-review-agent-workflow";
pub const GOAL_REVIEW_PLANNER_PROMPT_ID: &str = "@standard/goal-review-planner";

pub use crate::predefined::agent_templates::{
    GOAL_REVIEW_EXECUTOR_TEMPLATE_ID, GOAL_REVIEW_REVIEWER_TEMPLATE_ID,
};

const DEFAULT_MAX_ITERATIONS: u32 = 10;
const DEFAULT_PLANNER_PROFILE_ID: &str = "gpt-4o-mini";

#[derive(Debug, Clone)]
pub struct GoalReviewConfig {
    pub root_requirement: String,
    pub target_path: Option<String>,
    pub max_iterations: u32,
    pub planner_profile_id: String,
    pub executor_profile_id: Option<String>,
    pub reviewer_profile_id: Option<String>,
    pub planner_system_prompt: Option<String>,
    pub executor_system_prompt: Option<String>,
    pub reviewer_system_prompt: Option<String>,
    pub executor_tools: Option<Vec<String>>,
    pub reviewer_tools: Option<Vec<String>>,
    pub executor_max_iterations: Option<u32>,
    pub reviewer_max_iterations: Option<u32>,
    pub initial_messages: Option<Vec<Message>>,
}

impl GoalReviewConfig {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let root_requirement = value
            .get("root_requirement")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                "GoalReviewResourcePlugin config requires 'root_requirement' (string)".to_string()
            })?
            .to_string();

        let executor_tools = value
            .get("executor_tools")
            .map(|v| {
                v.as_array()
                    .ok_or_else(|| "'executor_tools' must be an array of tool names".to_string())?
                    .iter()
                    .map(|t| {
                        t.as_str()
                            .map(|s| s.to_string())
                            .ok_or_else(|| "'executor_tools' entries must be strings".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;

        let reviewer_tools = value
            .get("reviewer_tools")
            .map(|v| {
                v.as_array()
                    .ok_or_else(|| "'reviewer_tools' must be an array of tool names".to_string())?
                    .iter()
                    .map(|t| {
                        t.as_str()
                            .map(|s| s.to_string())
                            .ok_or_else(|| "'reviewer_tools' entries must be strings".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;

        let initial_messages = value
            .get("initial_messages")
            .map(|v| {
                serde_json::from_value::<Vec<Message>>(v.clone())
                    .map_err(|e| format!("invalid 'initial_messages': {e}"))
            })
            .transpose()?;

        Ok(Self {
            root_requirement,
            target_path: value
                .get("target_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            max_iterations: value
                .get("max_iterations")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32)
                .unwrap_or(DEFAULT_MAX_ITERATIONS),
            planner_profile_id: value
                .get("planner_profile_id")
                .and_then(|v| v.as_str())
                .unwrap_or(DEFAULT_PLANNER_PROFILE_ID)
                .to_string(),
            executor_profile_id: value
                .get("executor_profile_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            reviewer_profile_id: value
                .get("reviewer_profile_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            planner_system_prompt: value
                .get("planner_system_prompt")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            executor_system_prompt: value
                .get("executor_system_prompt")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            reviewer_system_prompt: value
                .get("reviewer_system_prompt")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            executor_tools,
            reviewer_tools,
            executor_max_iterations: value
                .get("executor_max_iterations")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            reviewer_max_iterations: value
                .get("reviewer_max_iterations")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            initial_messages,
        })
    }
}

/// Goal-driven review loop resource plugin: planner -> executor -> reviewer -> loop
/// check (id `@standard/goal-review-agent`).
pub struct GoalReviewResourcePlugin;

impl GoalReviewResourcePlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GoalReviewResourcePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourcePlugin for GoalReviewResourcePlugin {
    fn metadata(&self) -> ResourcePluginMetadata {
        ResourcePluginMetadata {
            id: GOAL_REVIEW_RESOURCE_PLUGIN_ID.into(),
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
                    ResourcePluginConfigField {
                        r#type: ResourcePluginConfigFieldType::Number,
                        description: "Maximum review loop iterations".into(),
                        default: Some(json!(DEFAULT_MAX_ITERATIONS)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "planner_profile_id".into(),
                    ResourcePluginConfigField {
                        r#type: ResourcePluginConfigFieldType::String,
                        description: "LLM profile for task planning (lightweight model)".into(),
                        default: Some(json!(DEFAULT_PLANNER_PROFILE_ID)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "executor_profile_id".into(),
                    ResourcePluginConfigField {
                        r#type: ResourcePluginConfigFieldType::String,
                        description: "LLM profile for executor (default from template)".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_profile_id".into(),
                    ResourcePluginConfigField {
                        r#type: ResourcePluginConfigFieldType::String,
                        description: "LLM profile for reviewer (default from template)".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "planner_system_prompt".into(),
                    ResourcePluginConfigField {
                        r#type: ResourcePluginConfigFieldType::String,
                        description: "Custom system prompt for the task planner".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "executor_system_prompt".into(),
                    ResourcePluginConfigField {
                        r#type: ResourcePluginConfigFieldType::String,
                        description: "Override system prompt for the executor agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_system_prompt".into(),
                    ResourcePluginConfigField {
                        r#type: ResourcePluginConfigFieldType::String,
                        description: "Override system prompt for the reviewer agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "executor_tools".into(),
                    ResourcePluginConfigField {
                        r#type: ResourcePluginConfigFieldType::Array,
                        description: "Override tools for the executor agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_tools".into(),
                    ResourcePluginConfigField {
                        r#type: ResourcePluginConfigFieldType::Array,
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
        let config = GoalReviewConfig::from_value(config)?;
        let mut bundle = ResourceBundle::new();
        bundle.workflows.push(build_workflow(&config)?);
        bundle.prompts.push(build_planner_prompt(&config));
        Ok(bundle)
    }
}

/// All built-in resource plugins, registered into the bundle registry during the
/// resource registration pipeline.
pub fn builtin_resource_plugins() -> Vec<Box<dyn ResourcePlugin>> {
    vec![Box::new(GoalReviewResourcePlugin::new())]
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::agent::AgentDefinition;
    use wf_types::node::StaticNodeType;
    use wf_types::workflow::EdgeType;

    #[test]
    fn config_parsing_defaults() {
        let config =
            GoalReviewConfig::from_value(&json!({"root_requirement": "fix the bug"})).unwrap();
        assert_eq!(config.root_requirement, "fix the bug");
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.planner_profile_id, "gpt-4o-mini");
        assert!(config.target_path.is_none());
        assert!(config.executor_tools.is_none());
    }

    #[test]
    fn config_parsing_requires_root_requirement() {
        let err = GoalReviewConfig::from_value(&json!({})).unwrap_err();
        assert!(err.contains("root_requirement"));
    }

    #[test]
    fn config_parsing_full() {
        let config = GoalReviewConfig::from_value(&json!({
            "root_requirement": "fix",
            "target_path": "src/lib.rs",
            "max_iterations": 5,
            "planner_profile_id": "mock",
            "executor_profile_id": "exec",
            "executor_tools": ["read_file"],
            "executor_max_iterations": 3,
        }))
        .unwrap();
        assert_eq!(config.max_iterations, 5);
        assert_eq!(config.executor_profile_id.as_deref(), Some("exec"));
        assert_eq!(config.executor_tools.as_ref().unwrap().len(), 1);
        assert_eq!(config.executor_max_iterations, Some(3));
    }

    #[test]
    fn metadata_matches_ts_schema() {
        let plugin = GoalReviewResourcePlugin::new();
        let metadata = plugin.metadata();
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
        let plugin = GoalReviewResourcePlugin::new();
        let bundle = plugin
            .assemble(&json!({"root_requirement": "review this"}))
            .unwrap();

        assert_eq!(bundle.workflows.len(), 1);
        assert_eq!(bundle.prompts.len(), 1);
        assert_eq!(bundle.prompts[0].id, "@standard/goal-review-planner");

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
        let plugin = GoalReviewResourcePlugin::new();
        let bundle = plugin
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
