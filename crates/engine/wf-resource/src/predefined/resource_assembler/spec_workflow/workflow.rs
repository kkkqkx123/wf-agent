use std::collections::HashMap;

use serde_json::{json, Value};
use wf_types::node::{BaseStaticNode, StaticNodeType};
use wf_types::workflow::{EdgeType, WorkflowDefinition, WorkflowMetadata, WorkflowTemplate};
use wf_types::workflow_execution::{VariableDefinition, VariableValueType};

use crate::resource_assembler::workflow_edge;

use super::config::SpecWorkflowConfig;
use super::prompts::{PLAN_PROMPT_ID, SPECIFY_PROMPT_ID, TASKS_PROMPT_ID};

pub const SPEC_WORKFLOW_ID: &str = "@standard/spec-workflow";

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

fn llm_node(id: &str, name: &str, profile_id: &str, prompt_id: &str) -> BaseStaticNode {
    BaseStaticNode {
        id: id.into(),
        node_type: StaticNodeType::Llm,
        name: Some(name.into()),
        description: None,
        config: Some(json!({
            "profile_id": profile_id,
            "context_id": "default",
            "system_prompt_template_id": prompt_id,
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
        SPECIFY_PROMPT_ID,
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
        PLAN_PROMPT_ID,
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
        TASKS_PROMPT_ID,
    ));
    nodes.push(BaseStaticNode {
        id: "tasks_route".into(),
        node_type: StaticNodeType::Route,
        name: Some("Tasks Ready Check".into()),
        description: None,
        config: Some(json!({
            "conditions": [
                {"expression": "eq(tasksReady,true)", "target_node_id": "end"},
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
        "end",
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

    nodes.push(BaseStaticNode {
        id: "end".into(),
        node_type: StaticNodeType::End,
        name: Some("End".into()),
        description: None,
        config: Some(json!({
            "data_outputs": [
                {"internal_name": "tasksReady", "output_key": "tasksReady"},
            ],
        })),
        execution_config: None,
    });

    Ok(WorkflowTemplate {
        id: SPEC_WORKFLOW_ID.into(),
        name: "Spec Workflow".into(),
        description: "Spec-driven planning pipeline: specify -> plan -> tasks".into(),
        template_tags: Some(vec!["spec-driven".into(), "planning".into()]),
        template_category: Some("spec-driven".into()),
        is_public: Some(true),
        enabled: Some(true),
        definition: WorkflowDefinition {
            id: SPEC_WORKFLOW_ID.into(),
            name: "Spec Workflow".into(),
            description: Some("Spec-driven planning pipeline: specify -> plan -> tasks".into()),
            r#type: None,
            version: None,
            nodes,
            edges,
            config: None,
            variables: Some(variables),
            triggered_subworkflow_config: None,
            metadata: Some(WorkflowMetadata {
                author: None,
                tags: Some(vec!["spec-driven".into(), "planning".into()]),
                category: Some("spec-driven".into()),
            }),
            created_at: t,
            updated_at: t,
            available_tools: None,
            hooks: None,
        },
    })
}
