use std::collections::HashMap;

use serde_json::{json, Value};
use wf_types::agent::AgentDefinition;
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::{BaseStaticNode, StaticNodeType};
use wf_types::tool::AvailableTools;
use wf_types::workflow::{Edge, EdgeType, WorkflowDefinition, WorkflowMetadata, WorkflowTemplate};
use wf_types::workflow_execution::{VariableDefinition, VariableValueType};

use super::goal_review::{
    GoalReviewConfig, GOAL_REVIEW_EXECUTOR_TEMPLATE_ID, GOAL_REVIEW_PLANNER_PROMPT_ID,
    GOAL_REVIEW_REVIEWER_TEMPLATE_ID, GOAL_REVIEW_WORKFLOW_ID,
};

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

pub(crate) fn build_planner_prompt(config: &GoalReviewConfig) -> wf_types::Template {
    wf_types::Template {
        id: GOAL_REVIEW_PLANNER_PROMPT_ID.into(),
        name: "Goal Review Planner Prompt".into(),
        description: Some("System prompt for the task planner LLM node".into()),
        category: "system".into(),
        content: config
            .planner_system_prompt
            .clone()
            .unwrap_or_else(|| DEFAULT_PLANNER_PROMPT.to_string()),
        variables: None,
        fragments: None,
    }
}

pub(crate) fn build_workflow(config: &GoalReviewConfig) -> Result<WorkflowTemplate, String> {
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
        // loop_start_node_id).
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
            hooks: None,
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
/// return the inline `AgentDefinition` the AGENT_LOOP handler requires.
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
                    discoverable: None,
                    enable_general_tool: None,
                    hidden: None,
                    require_approval: None,
                    allowed_workflows: None,
                });
            available.available = tools;
        }
    }
    Ok(definition)
}
