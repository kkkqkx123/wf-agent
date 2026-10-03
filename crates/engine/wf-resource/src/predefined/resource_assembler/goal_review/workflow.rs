use serde_json::{json, Value};
use std::collections::HashMap;

use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::configs::LoopVariableInput;
use wf_types::node::{BaseStaticNode, StaticNodeType};
use wf_types::workflow::{EdgeType, WorkflowDefinition, WorkflowMetadata, WorkflowTemplate};
use wf_types::workflow_execution::{VariableDefinition, VariableValueType};

use crate::resource_assembler::{workflow_edge, AgentTemplateBuilder, LoopWorkflowBuilder};

use super::agent::{goal_review_executor, goal_review_reviewer};
use super::config::GoalReviewConfig;

pub const GOAL_REVIEW_WORKFLOW_ID: &str = "@standard/goal-review-agent-workflow";
pub const GOAL_REVIEW_PLANNER_PROMPT_ID: &str = "prompt.goal-review.planner";

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

    let executor_inline = AgentTemplateBuilder::new(goal_review_executor())
        .maybe_profile(config.executor_profile_id.clone())
        .maybe_system_prompt(config.executor_system_prompt.clone())
        .maybe_max_iterations(config.executor_max_iterations)
        .maybe_tools(config.executor_tools.clone())
        .build_inline();
    let reviewer_inline = AgentTemplateBuilder::new(goal_review_reviewer())
        .maybe_profile(config.reviewer_profile_id.clone())
        .maybe_system_prompt(config.reviewer_system_prompt.clone())
        .maybe_max_iterations(config.reviewer_max_iterations)
        .maybe_tools(config.reviewer_tools.clone())
        .build_inline();

    let planner_text = config
        .planner_system_prompt
        .clone()
        .unwrap_or_else(|| DEFAULT_PLANNER_PROMPT.to_string());
    let start_messages = config.initial_messages.clone().unwrap_or_else(|| {
        vec![Message {
            id: String::new(),
            role: MessageRole::System,
            content: MessageContentValue::Text(planner_text.clone()),
            timestamp: t,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }]
    });

    let start_node = BaseStaticNode {
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
    };

    let (mut nodes, mut edges) = LoopWorkflowBuilder::new("review-loop", "loop_start", "loop_end")
        .loop_start_name("Review Loop")
        .loop_end_name("Loop End Check")
        .max_iterations(config.max_iterations)
        .variable_inputs(vec![
            LoopVariableInput {
                source_path: "status".into(),
                internal_name: "status".into(),
                required: Some(true),
                default_value: None,
                description: None,
            },
            LoopVariableInput {
                source_path: "complete".into(),
                internal_name: "complete".into(),
                required: Some(true),
                default_value: None,
                description: None,
            },
            LoopVariableInput {
                source_path: "judges".into(),
                internal_name: "judges".into(),
                required: Some(true),
                default_value: None,
                description: None,
            },
            LoopVariableInput {
                source_path: "rootRequirement".into(),
                internal_name: "rootRequirement".into(),
                required: Some(true),
                default_value: None,
                description: None,
            },
            LoopVariableInput {
                source_path: "iterationCount".into(),
                internal_name: "iterationCount".into(),
                required: Some(false),
                default_value: Some(json!(0)),
                description: None,
            },
        ])
        .break_condition(BREAK_CONDITION)
        .continue_condition(CONTINUE_CONDITION)
        .loop_back_edge_id("e7")
        .body_nodes(vec![
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
        ])
        .body_edges(vec![
            workflow_edge("e2", "loop_start", "task_planner", EdgeType::Default, None),
            workflow_edge(
                "e3",
                "task_planner",
                "executor_agent",
                EdgeType::Default,
                None,
            ),
            workflow_edge(
                "e4",
                "executor_agent",
                "reviewer_agent",
                EdgeType::Default,
                None,
            ),
            workflow_edge("e5", "reviewer_agent", "loop_end", EdgeType::Default, None),
        ])
        .build()?;

    nodes.insert(0, start_node);
    nodes.push(BaseStaticNode {
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
    });

    edges.insert(
        0,
        workflow_edge("e0", "start", "loop_start", EdgeType::Default, None),
    );
    edges.push(workflow_edge(
        "e6",
        "loop_end",
        "end",
        EdgeType::Default,
        None,
    ));

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
                error_default: None,
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
