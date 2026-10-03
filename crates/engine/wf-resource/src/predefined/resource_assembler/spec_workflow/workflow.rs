use std::collections::HashMap;

use serde_json::{json, Value};
use wf_types::node::configs::LoopVariableInput;
use wf_types::node::{BaseStaticNode, StaticNodeType};
use wf_types::workflow::{EdgeType, WorkflowDefinition, WorkflowMetadata, WorkflowTemplate};
use wf_types::workflow_execution::{VariableDefinition, VariableValueType};

use crate::predefined::resource_assembler::goal_review::{
    agent::{goal_review_executor, goal_review_reviewer},
    workflow::GOAL_REVIEW_WORKFLOW_ID,
};
use crate::resource_assembler::{workflow_edge, AgentTemplateBuilder, LoopWorkflowBuilder};

use super::config::SpecWorkflowConfig;

pub const SPEC_WORKFLOW_ID: &str = "@standard/spec-workflow";

pub use super::prompts::STAGE_PROMPT_IDS;

const BREAK_CONDITION: &str = "or(eq(status,\"completed\"),eq(status,\"stuck\"))";
const CONTINUE_CONDITION: &str = "eq(nextIteration,true)";

const SPEC_EXECUTOR_PROMPT: &str = "You implement tasks from the change tasks.md file.\nYou have full file access. Make changes, run tests, and call attempt_completion when the task is done.\nEvery change must trace back to a requirement in the change specs/ directory; do not invent behavior outside the spec.";

const SPEC_REVIEWER_PROMPT: &str = "You are a strict reviewer for spec-driven changes.\nReview all changes against proposal.md, specs/, and design.md. For each file, assign a score (1-10) and actionable feedback.\n\nCall attempt_completion with:\n  data: { judges: [{ file, score, comment, resolved }] }\n  variables: { complete: boolean, status: \"completed\"|\"reviewing\"|\"stuck\", converged: boolean }\n\nResolved field: set resolved=false for each new defect initially.\nSet status to \"completed\" and converged to true only if ALL requirements are met.\nIf review results are highly similar to previous rounds (same files, same scores, same issues), set status to \"stuck\".";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn variable(
    name: &str,
    value: Value,
    value_type: VariableValueType,
    readonly: bool,
    description: &str,
) -> VariableDefinition {
    VariableDefinition {
        name: name.into(),
        value,
        r#type: Some(value_type),
        scope: None,
        readonly: Some(readonly),
        metadata: Some(HashMap::from([(
            "description".into(),
            Value::String(description.into()),
        )])),
    }
}

fn llm_node(id: &str, name: &str, profile_id: &str) -> BaseStaticNode {
    BaseStaticNode {
        id: id.into(),
        node_type: StaticNodeType::Llm,
        name: Some(name.into()),
        description: None,
        config: Some(json!({
            "profile_id": profile_id,
            "context_id": "default",
        })),
        execution_config: None,
    }
}

fn gate_node(id: &str, name: &str, prompt: &str) -> BaseStaticNode {
    BaseStaticNode {
        id: id.into(),
        node_type: StaticNodeType::UserInteraction,
        name: Some(name.into()),
        description: None,
        config: Some(json!({
            "prompt": prompt,
            "operation_type": "update_variables",
        })),
        execution_config: None,
    }
}

fn agent_node(
    id: &str,
    name: &str,
    inline_definition: wf_types::agent::AgentDefinition,
) -> BaseStaticNode {
    BaseStaticNode {
        id: id.into(),
        node_type: StaticNodeType::AgentLoop,
        name: Some(name.into()),
        description: None,
        config: Some(json!({
            "inline_definition": inline_definition,
            "message_inputs": [
                {"source_context_id": "default", "internal_name": "system-context"},
            ],
            "message_outputs": [
                {"internal_name": "system-context", "target_context_id": "default"},
            ],
        })),
        execution_config: None,
    }
}

fn loop_variable_inputs() -> Vec<LoopVariableInput> {
    for_source(&[
        ("status", true),
        ("complete", true),
        ("judges", true),
        ("converged", true),
        ("requirement", true),
    ])
}

fn for_source(entries: &[(&str, bool)]) -> Vec<LoopVariableInput> {
    entries
        .iter()
        .map(|(name, required)| LoopVariableInput {
            source_path: (*name).into(),
            internal_name: (*name).into(),
            required: Some(*required),
            default_value: None,
            description: None,
        })
        .collect()
}

pub(crate) fn build_workflow(config: &SpecWorkflowConfig) -> Result<WorkflowTemplate, String> {
    let t = now_ms();

    let variables = vec![
        variable(
            "requirement",
            Value::String(config.requirement.clone()),
            VariableValueType::String,
            true,
            "Original requirement text, injected into every stage",
        ),
        variable(
            "changeId",
            Value::String(config.change_id.clone()),
            VariableValueType::String,
            true,
            "Change directory name under the spec root",
        ),
        variable(
            "specDir",
            Value::String(config.spec_dir.clone()),
            VariableValueType::String,
            true,
            "Spec artifact root directory",
        ),
        variable(
            "specPhase",
            Value::String("specifying".into()),
            VariableValueType::String,
            false,
            "Current pipeline phase",
        ),
        variable(
            "specApproved",
            Value::Bool(false),
            VariableValueType::Boolean,
            false,
            "Spec gate decision, set by the spec reviewer",
        ),
        variable(
            "planApproved",
            Value::Bool(false),
            VariableValueType::Boolean,
            false,
            "Plan gate decision, set by the plan reviewer",
        ),
        variable(
            "tasksReady",
            Value::Bool(false),
            VariableValueType::Boolean,
            false,
            "Task list readiness flag, set by the task decomposer",
        ),
        variable(
            "converged",
            Value::Bool(false),
            VariableValueType::Boolean,
            false,
            "Convergence flag, set by the reviewer or converge check",
        ),
        variable(
            "status",
            Value::String("specifying".into()),
            VariableValueType::String,
            false,
            "Implement loop status: specifying | implementing | reviewing | completed | stuck",
        ),
        variable(
            "complete",
            Value::Bool(false),
            VariableValueType::Boolean,
            false,
            "Loop exit flag, set by reviewer agent",
        ),
        variable(
            "judges",
            Value::Array(Vec::new()),
            VariableValueType::Array,
            false,
            "Review judgment records, appended each iteration",
        ),
        variable(
            "iterationCount",
            Value::Number(0.into()),
            VariableValueType::Number,
            false,
            "Current iteration counter",
        ),
    ];

    let mut nodes = vec![BaseStaticNode {
        id: "start".into(),
        node_type: StaticNodeType::Start,
        name: Some("Start".into()),
        description: None,
        config: Some(json!({
            "data_inputs": [
                {"parent_field": "requirement", "internal_name": "requirement", "required": true},
                {"parent_field": "changeId", "internal_name": "changeId", "required": false},
            ],
        })),
        execution_config: None,
    }];
    let mut edges = vec![workflow_edge(
        "e0",
        "start",
        "spec_writer",
        EdgeType::Default,
        None,
    )];

    nodes.push(llm_node(
        "spec_writer",
        "Spec Writer",
        &config.spec_profile_id,
    ));

    let spec_gate_on = config.require_spec_gate;
    if spec_gate_on {
        nodes.push(gate_node(
            "spec_gate",
            "Spec Review Gate",
            "Review proposal.md and specs/. Reply approve to set specApproved=true, or reject with comments to set specApproved=false.",
        ));
        edges.push(workflow_edge(
            "e1",
            "spec_writer",
            "spec_gate",
            EdgeType::Default,
            None,
        ));
        edges.push(workflow_edge(
            "e2",
            "spec_gate",
            "plan_writer",
            EdgeType::Default,
            None,
        ));
        edges.push(workflow_edge(
            "e2b",
            "spec_gate",
            "spec_writer",
            EdgeType::Conditional,
            Some("eq(specApproved,false)".into()),
        ));
    } else {
        edges.push(workflow_edge(
            "e1",
            "spec_writer",
            "plan_writer",
            EdgeType::Default,
            None,
        ));
    }

    nodes.push(llm_node(
        "plan_writer",
        "Plan Writer",
        &config.plan_profile_id,
    ));

    let plan_gate_on = config.require_plan_gate;
    if plan_gate_on {
        nodes.push(gate_node(
            "plan_gate",
            "Plan Review Gate",
            "Review design.md. Reply approve to set planApproved=true, or reject with comments to set planApproved=false.",
        ));
        edges.push(workflow_edge(
            "e3",
            "plan_writer",
            "plan_gate",
            EdgeType::Default,
            None,
        ));
        edges.push(workflow_edge(
            "e4",
            "plan_gate",
            "task_decomposer",
            EdgeType::Default,
            None,
        ));
        edges.push(workflow_edge(
            "e4b",
            "plan_gate",
            "plan_writer",
            EdgeType::Conditional,
            Some("eq(planApproved,false)".into()),
        ));
    } else {
        edges.push(workflow_edge(
            "e3",
            "plan_writer",
            "task_decomposer",
            EdgeType::Default,
            None,
        ));
    }

    nodes.push(llm_node(
        "task_decomposer",
        "Task Decomposer",
        &config.tasks_profile_id,
    ));
    nodes.push(BaseStaticNode {
        id: "tasks_route".into(),
        node_type: StaticNodeType::Route,
        name: Some("Tasks Ready Check".into()),
        description: None,
        config: Some(json!({
            "conditions": [
                {"expression": "eq(tasksReady,true)", "target_node_id": "loop_start"},
            ],
            "default_target_node_id": "task_decomposer",
        })),
        execution_config: None,
    });
    edges.push(workflow_edge(
        "e5",
        "task_decomposer",
        "tasks_route",
        EdgeType::Default,
        None,
    ));
    edges.push(workflow_edge(
        "e6",
        "tasks_route",
        "loop_start",
        EdgeType::Default,
        None,
    ));
    edges.push(workflow_edge(
        "e6b",
        "tasks_route",
        "task_decomposer",
        EdgeType::Default,
        None,
    ));

    let mut loop_builder = LoopWorkflowBuilder::new("spec-implement-loop", "loop_start", "loop_end")
        .loop_start_name("Implement Loop")
        .loop_end_name("Implement Loop Check")
        .max_iterations(config.max_iterations)
        .variable_inputs(loop_variable_inputs())
        .break_condition(BREAK_CONDITION)
        .continue_condition(CONTINUE_CONDITION)
        .loop_back_edge_id("e9");

    if config.use_subgraph_delegate {
        loop_builder = loop_builder.body_node(BaseStaticNode {
            id: "goal_delegate".into(),
            node_type: StaticNodeType::Subgraph,
            name: Some("Goal Review Delegate".into()),
            description: None,
            config: Some(json!({
                "subgraph_id": GOAL_REVIEW_WORKFLOW_ID,
                "variable_inputs": [
                    {"source_path": "requirement", "internal_name": "rootRequirement"},
                ],
                "variable_outputs": [
                    {"internal_name": "judges", "target_path": "judges"},
                    {"internal_name": "status", "target_path": "status"},
                    {"internal_name": "complete", "target_path": "complete"},
                ],
            })),
            execution_config: None,
        });
        loop_builder = loop_builder
            .body_edge(workflow_edge(
                "e7",
                "loop_start",
                "goal_delegate",
                EdgeType::Default,
                None,
            ))
            .body_edge(workflow_edge(
                "e8",
                "goal_delegate",
                "loop_end",
                EdgeType::Default,
                None,
            ));
    } else {
        let executor_inline = AgentTemplateBuilder::new(goal_review_executor())
            .maybe_profile(config.executor_profile_id.clone())
            .with_system_prompt(SPEC_EXECUTOR_PROMPT)
            .maybe_max_iterations(config.executor_max_iterations)
            .maybe_tools(config.executor_tools.clone())
            .build_inline();
        let reviewer_inline = AgentTemplateBuilder::new(goal_review_reviewer())
            .maybe_profile(config.reviewer_profile_id.clone())
            .with_system_prompt(SPEC_REVIEWER_PROMPT)
            .maybe_max_iterations(config.reviewer_max_iterations)
            .maybe_tools(config.reviewer_tools.clone())
            .build_inline();
        loop_builder = loop_builder
            .body_node(agent_node("implementer", "Implementer", executor_inline))
            .body_node(agent_node("spec_reviewer", "Spec Reviewer", reviewer_inline));
        loop_builder = loop_builder
            .body_edge(workflow_edge(
                "e7",
                "loop_start",
                "implementer",
                EdgeType::Default,
                None,
            ))
            .body_edge(workflow_edge(
                "e8a",
                "implementer",
                "spec_reviewer",
                EdgeType::Default,
                None,
            ))
            .body_edge(workflow_edge(
                "e8b",
                "spec_reviewer",
                "loop_end",
                EdgeType::Default,
                None,
            ));
    }

    let (loop_nodes, mut loop_edges) = loop_builder.build()?;
    nodes.extend(loop_nodes);
    edges.append(&mut loop_edges);

    nodes.push(llm_node(
        "converge_check",
        "Converge Check",
        &config.converge_profile_id,
    ));
    nodes.push(BaseStaticNode {
        id: "converge_route".into(),
        node_type: StaticNodeType::Route,
        name: Some("Converge Route".into()),
        description: None,
        config: Some(json!({
            "conditions": [
                {"expression": "eq(converged,true)", "target_node_id": "archive"},
            ],
            "default_target_node_id": "end",
        })),
        execution_config: None,
    });
    nodes.push(BaseStaticNode {
        id: "archive".into(),
        node_type: StaticNodeType::Script,
        name: Some("Archive Change".into()),
        description: None,
        config: Some(json!({
            "script_name": "spec_archive",
            "risk": "medium",
            "params": {"change_id": config.change_id, "spec_dir": config.spec_dir},
        })),
        execution_config: None,
    });
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
                {"internal_name": "converged", "output_key": "converged"},
            ],
        })),
        execution_config: None,
    });

    edges.push(workflow_edge(
        "e10",
        "loop_end",
        "converge_check",
        EdgeType::Default,
        None,
    ));
    edges.push(workflow_edge(
        "e11",
        "converge_check",
        "converge_route",
        EdgeType::Default,
        None,
    ));
    edges.push(workflow_edge(
        "e12",
        "converge_route",
        "archive",
        EdgeType::Default,
        None,
    ));
    edges.push(workflow_edge(
        "e12b",
        "converge_route",
        "end",
        EdgeType::Default,
        None,
    ));
    edges.push(workflow_edge(
        "e13",
        "archive",
        "end",
        EdgeType::Default,
        None,
    ));

    Ok(WorkflowTemplate {
        id: SPEC_WORKFLOW_ID.into(),
        name: "Spec Workflow".into(),
        description: "Spec-driven pipeline: specify -> plan -> tasks -> implement -> converge -> archive"
            .into(),
        definition: WorkflowDefinition {
            id: SPEC_WORKFLOW_ID.into(),
            name: "Spec Workflow".into(),
            description: Some(
                "Spec-driven pipeline: specify -> plan -> tasks -> implement -> converge -> archive"
                    .into(),
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
                tags: Some(vec!["spec-driven".into(), "workflow".into()]),
                category: Some("spec-driven".into()),
            }),
            available_tools: None,
            created_at: t,
            updated_at: t,
            hooks: None,
        },
        template_category: Some("spec-driven".into()),
        template_tags: Some(vec!["spec-driven".into(), "pipeline".into()]),
        is_public: Some(true),
        enabled: Some(true),
    })
}
