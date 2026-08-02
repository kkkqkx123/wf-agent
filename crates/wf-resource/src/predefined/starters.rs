use std::collections::HashMap;

use serde_json::{json, Value};
use wf_types::agent::AgentDefinition;
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::{BaseStaticNode, StaticNodeType};
use wf_types::tool::AvailableTools;
use wf_types::workflow::{Edge, EdgeType, WorkflowDefinition, WorkflowMetadata, WorkflowTemplate};
use wf_types::workflow_execution::{VariableDefinition, VariableValueType};
use wf_types::PromptTemplate;

use crate::starter::{
    Bundle, Starter, StarterConfigField, StarterConfigFieldType, StarterMetadata,
};

pub const GOAL_REVIEW_STARTER_ID: &str = "@standard/goal-review-agent";
pub const GOAL_REVIEW_WORKFLOW_ID: &str = "@standard/goal-review-agent-workflow";
pub const GOAL_REVIEW_PLANNER_PROMPT_ID: &str = "@standard/goal-review-planner";

pub const GOAL_REVIEW_EXECUTOR_TEMPLATE_ID: &str = "@standard/goal-review-executor";
pub const GOAL_REVIEW_REVIEWER_TEMPLATE_ID: &str = "@standard/goal-review-reviewer";

const DEFAULT_MAX_ITERATIONS: u32 = 10;
const DEFAULT_PLANNER_PROFILE_ID: &str = "gpt-4o-mini";

const DEFAULT_PLANNER_PROMPT: &str = "You are a task planner for a goal-driven review loop.
Read the root requirement, the conversation history, and the unresolved review defects.
Output a single clear task description for the executor to work on next.";

const BREAK_CONDITION: &str = "or(eq(status,\"completed\"),eq(status,\"stuck\"))";
const CONTINUE_CONDITION: &str = "eq(nextIteration,true)";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

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
                "GoalReviewStarter config requires 'root_requirement' (string)".to_string()
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

/// Goal-driven review loop starter: planner -> executor -> reviewer -> loop
/// check, mirroring the TS `GoalReviewStarter` (id `@standard/goal-review-agent`).
pub struct GoalReviewStarter;

impl GoalReviewStarter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GoalReviewStarter {
    fn default() -> Self {
        Self::new()
    }
}

impl Starter for GoalReviewStarter {
    fn metadata(&self) -> StarterMetadata {
        StarterMetadata {
            id: GOAL_REVIEW_STARTER_ID.into(),
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
                    StarterConfigField {
                        r#type: StarterConfigFieldType::Number,
                        description: "Maximum review loop iterations".into(),
                        default: Some(json!(DEFAULT_MAX_ITERATIONS)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "planner_profile_id".into(),
                    StarterConfigField {
                        r#type: StarterConfigFieldType::String,
                        description: "LLM profile for task planning (lightweight model)".into(),
                        default: Some(json!(DEFAULT_PLANNER_PROFILE_ID)),
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "executor_profile_id".into(),
                    StarterConfigField {
                        r#type: StarterConfigFieldType::String,
                        description: "LLM profile for executor (default from template)".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_profile_id".into(),
                    StarterConfigField {
                        r#type: StarterConfigFieldType::String,
                        description: "LLM profile for reviewer (default from template)".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "planner_system_prompt".into(),
                    StarterConfigField {
                        r#type: StarterConfigFieldType::String,
                        description: "Custom system prompt for the task planner".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "executor_system_prompt".into(),
                    StarterConfigField {
                        r#type: StarterConfigFieldType::String,
                        description: "Override system prompt for the executor agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_system_prompt".into(),
                    StarterConfigField {
                        r#type: StarterConfigFieldType::String,
                        description: "Override system prompt for the reviewer agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "executor_tools".into(),
                    StarterConfigField {
                        r#type: StarterConfigFieldType::Array,
                        description: "Override tools for the executor agent".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                    },
                ),
                (
                    "reviewer_tools".into(),
                    StarterConfigField {
                        r#type: StarterConfigFieldType::Array,
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

    fn assemble(&self, config: &Value) -> Result<Bundle, String> {
        let config = GoalReviewConfig::from_value(config)?;
        let mut bundle = Bundle::new();
        bundle.workflows.push(build_workflow(&config)?);
        bundle.prompts.push(build_planner_prompt(&config));
        Ok(bundle)
    }
}

fn build_planner_prompt(config: &GoalReviewConfig) -> PromptTemplate {
    PromptTemplate {
        id: GOAL_REVIEW_PLANNER_PROMPT_ID.into(),
        name: "Goal Review Planner Prompt".into(),
        description: "System prompt for the task planner LLM node".into(),
        category: "system".into(),
        content: config
            .planner_system_prompt
            .clone()
            .unwrap_or_else(|| DEFAULT_PLANNER_PROMPT.to_string()),
        variables: None,
        fragments: None,
    }
}

fn build_workflow(config: &GoalReviewConfig) -> Result<WorkflowTemplate, String> {
    let t = now_ms();

    let variables = vec![
        VariableDefinition {
            name: "rootRequirement".into(),
            value: Value::String(config.root_requirement.clone()),
            r#type: Some(VariableValueType::String),
            scope: None,
            readonly: Some(true),
            metadata: Some(HashMap::from([(
                "description".into(),
                Value::String(
                    "Original goal, injected into planner and reviewer each iteration".into(),
                ),
            )])),
        },
        VariableDefinition {
            name: "status".into(),
            value: Value::String("planning".into()),
            r#type: Some(VariableValueType::String),
            scope: None,
            readonly: None,
            metadata: Some(HashMap::from([(
                "description".into(),
                Value::String(
                    "Current loop status: planning | executing | reviewing | completed | stuck"
                        .into(),
                ),
            )])),
        },
        VariableDefinition {
            name: "complete".into(),
            value: Value::Bool(false),
            r#type: Some(VariableValueType::Boolean),
            scope: None,
            readonly: None,
            metadata: Some(HashMap::from([(
                "description".into(),
                Value::String("Loop exit flag, set by reviewer agent".into()),
            )])),
        },
        VariableDefinition {
            name: "judges".into(),
            value: Value::Array(Vec::new()),
            r#type: Some(VariableValueType::Array),
            scope: None,
            readonly: None,
            metadata: Some(HashMap::from([(
                "description".into(),
                Value::String("Review judgment records, appended each iteration".into()),
            )])),
        },
        VariableDefinition {
            name: "iterationCount".into(),
            value: Value::Number(0.into()),
            r#type: Some(VariableValueType::Number),
            scope: None,
            readonly: None,
            metadata: Some(HashMap::from([(
                "description".into(),
                Value::String("Current iteration counter".into()),
            )])),
        },
    ];

    let executor_inline = build_inline_definition(
        GOAL_REVIEW_EXECUTOR_TEMPLATE_ID,
        config.executor_profile_id.clone(),
        config.executor_system_prompt.clone(),
        config.executor_max_iterations,
        config.executor_tools.clone(),
    )?;
    let reviewer_inline = build_inline_definition(
        GOAL_REVIEW_REVIEWER_TEMPLATE_ID,
        config.reviewer_profile_id.clone(),
        config.reviewer_system_prompt.clone(),
        config.reviewer_max_iterations,
        config.reviewer_tools.clone(),
    )?;

    let start_messages = config.initial_messages.clone().unwrap_or_else(|| {
        vec![Message {
            id: String::new(),
            role: MessageRole::System,
            content: MessageContentValue::Text(DEFAULT_PLANNER_PROMPT.to_string()),
            timestamp: t,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }]
    });

    let nodes = vec![
        BaseStaticNode {
            id: "start".into(),
            node_type: StaticNodeType::Start,
            name: Some("Start".into()),
            description: None,
            config: Some(json!({
                "message_inputs": [{
                    "source_context_id": "initial",
                    "internal_name": "default",
                    "required": true,
                    "default_messages": start_messages,
                }],
                "data_inputs": [
                    {"parent_field": "rootRequirement", "internal_name": "rootRequirement", "required": true},
                    {"parent_field": "targetPath", "internal_name": "targetPath", "required": false},
                ],
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "loop_start".into(),
            node_type: StaticNodeType::LoopStart,
            name: Some("Review Loop".into()),
            description: None,
            config: Some(json!({
                "loop_id": "review-loop",
                "max_iterations": config.max_iterations,
                "variable_inputs": [
                    {"source_path": "status", "internal_name": "status", "required": true},
                    {"source_path": "complete", "internal_name": "complete", "required": true},
                    {"source_path": "judges", "internal_name": "judges", "required": true},
                    {"source_path": "rootRequirement", "internal_name": "rootRequirement", "required": true},
                    {"source_path": "iterationCount", "internal_name": "iterationCount", "required": false, "default_value": 0},
                ],
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "task_planner".into(),
            node_type: StaticNodeType::Llm,
            name: Some("Task Planner".into()),
            description: None,
            config: Some(json!({
                "profile_id": config.planner_profile_id,
                "context_id": "default",
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "executor_agent".into(),
            node_type: StaticNodeType::AgentLoop,
            name: Some("Executor Agent".into()),
            description: None,
            config: Some(json!({
                "inline_definition": executor_inline,
                "message_inputs": [
                    {"source_context_id": "default", "internal_name": "system-context"},
                ],
                "message_outputs": [
                    {"internal_name": "system-context", "target_context_id": "default"},
                ],
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "reviewer_agent".into(),
            node_type: StaticNodeType::AgentLoop,
            name: Some("Reviewer Agent".into()),
            description: None,
            config: Some(json!({
                "inline_definition": reviewer_inline,
                "data_inputs": [
                    {"parent_field": "judges", "internal_name": "previous_judges"},
                ],
                "message_inputs": [
                    {"source_context_id": "default", "internal_name": "review-context"},
                ],
                "message_outputs": [
                    {"internal_name": "review-context", "target_context_id": "default"},
                ],
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "loop_end".into(),
            node_type: StaticNodeType::LoopEnd,
            name: Some("Loop End Check".into()),
            description: None,
            config: Some(json!({
                "loop_id": "review-loop",
                "break_condition": BREAK_CONDITION,
                "loop_start_node_id": "loop_start",
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "end".into(),
            node_type: StaticNodeType::End,
            name: Some("End".into()),
            description: None,
            config: Some(json!({
                "data_outputs": [
                    {"internal_name": "judges", "output_key": "judges"},
                    {"internal_name": "status", "output_key": "status"},
                    {"internal_name": "complete", "output_key": "complete"},
                ],
            })),
            execution_config: None,
        },
    ];

    let edges = vec![
        edge("e0", "start", "loop_start", EdgeType::Default, None),
        edge("e2", "loop_start", "task_planner", EdgeType::Default, None),
        edge(
            "e3",
            "task_planner",
            "executor_agent",
            EdgeType::Default,
            None,
        ),
        edge(
            "e4",
            "executor_agent",
            "reviewer_agent",
            EdgeType::Default,
            None,
        ),
        edge("e5", "reviewer_agent", "loop_end", EdgeType::Default, None),
        edge("e6", "loop_end", "end", EdgeType::Default, None),
        // Loop-back edge: the Rust engine treats LOOP_END -> LOOP_START as
        // the legal loop continuation (the LOOP_END handler jumps back via
        // loop_start_node_id); TS instead routes back to the loop body start.
        edge(
            "e7",
            "loop_end",
            "loop_start",
            EdgeType::Conditional,
            Some(CONTINUE_CONDITION.to_string()),
        ),
    ];

    Ok(WorkflowTemplate {
        id: GOAL_REVIEW_WORKFLOW_ID.into(),
        name: "Goal Review Agent Workflow".into(),
        description: "Goal-driven review loop: planner -> executor -> reviewer -> loop check"
            .into(),
        definition: WorkflowDefinition {
            id: GOAL_REVIEW_WORKFLOW_ID.into(),
            name: "Goal Review Agent Workflow".into(),
            description: Some(
                "Goal-driven review loop: planner -> executor -> reviewer -> loop check".into(),
            ),
            r#type: Some(wf_types::workflow::WorkflowDefinitionType::Standalone),
            version: Some("1.0.0".into()),
            nodes,
            edges,
            config: Some(wf_types::workflow::WorkflowConfig {
                timeout: Some(600_000),
                max_steps: None,
                checkpoint: Some(wf_types::checkpoint::workflow::WorkflowCheckpointConfig {
                    enabled: true,
                    interval_nodes: None,
                    on_error: None,
                    on_completion: None,
                    content: None,
                }),
                retry_policy: None,
                tool_approval: None,
                available_tools: None,
                initial_messages: None,
                system_prompt_template_id: None,
                system_prompt_template_variables: None,
                system_prompt: None,
                static_contexts: None,
            }),
            variables: Some(variables),
            triggers: None,
            triggered_subworkflow_config: None,
            metadata: Some(WorkflowMetadata {
                author: Some("system".into()),
                tags: Some(vec![
                    "review".into(),
                    "goal-driven".into(),
                    "agent-loop".into(),
                ]),
                category: Some("code-review".into()),
            }),
            available_tools: None,
            created_at: t,
            updated_at: t,
        },
        template_category: Some("code-review".into()),
        template_tags: Some(vec!["review".into(), "goal-driven".into()]),
        is_public: Some(true),
        enabled: Some(true),
    })
}

fn edge(id: &str, source: &str, target: &str, r#type: EdgeType, condition: Option<String>) -> Edge {
    Edge {
        id: id.into(),
        source_node_id: source.into(),
        target_node_id: target.into(),
        r#type,
        condition,
        label: None,
        description: None,
        weight: None,
        metadata: None,
    }
}

/// Merge the user-supplied overrides into the builtin agent template and
/// return the inline `AgentDefinition` the AGENT_LOOP handler requires
/// (mirrors the TS `inlineConfig` merge semantics).
fn build_inline_definition(
    template_id: &str,
    profile_id: Option<String>,
    system_prompt: Option<String>,
    max_iterations: Option<u32>,
    tools: Option<Vec<String>>,
) -> Result<AgentDefinition, String> {
    let templates = crate::predefined::agent_templates::builtin_agent_templates();
    let tmpl = templates
        .iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| format!("Builtin agent template \"{template_id}\" not found"))?;

    let mut definition = tmpl.definition.clone();
    if let Some(config) = definition.config.as_mut() {
        if let Some(id) = profile_id {
            config.profile_id = Some(id);
        }
        if let Some(prompt) = system_prompt {
            config.system_prompt = Some(prompt);
        }
        if let Some(iterations) = max_iterations {
            config.max_iterations = Some(iterations);
        }
        if let Some(tools) = tools {
            let available = config
                .available_tools
                .get_or_insert_with(|| AvailableTools {
                    available: Vec::new(),
                    initial: None,
                    require_approval: None,
                    allowed_workflows: None,
                });
            available.available = tools;
        }
    }
    Ok(definition)
}

/// All built-in starters, registered into the bundle registry during the
/// resource registration pipeline.
pub fn builtin_starters() -> Vec<Box<dyn Starter>> {
    vec![Box::new(GoalReviewStarter::new())]
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::node::StaticNodeType;

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
        let starter = GoalReviewStarter::new();
        let metadata = starter.metadata();
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
        let starter = GoalReviewStarter::new();
        let bundle = starter
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

        // Node types match the TS layout.
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

        // Loop wiring matches the TS break/continue semantics.
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

        // 5 workflow variables mirror the TS set.
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
        let starter = GoalReviewStarter::new();
        let bundle = starter
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
