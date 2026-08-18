//! Definition of the execute_workflow tool (builtin type). Execution is
//! handled by the BuiltinExecutor through the registered ExecutionCallback.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static EXECUTE_WORKFLOW: ToolDefinition = ToolDefinition {
    id: "execute_workflow",
    tool_type: ToolType::BuiltIn,
    risk_level: ToolRiskLevel::System,
    create_checkpoint: None,
    category: "workflow",
    tags: &["execute"],
    description:
        "Execute a predefined workflow by ID. Workflows are multi-step automation sequences.",
    parameters: &[
        ToolParameter {
            name: "workflow_id",
            r#type: "string",
            required: true,
            description: "The ID of the workflow to execute",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "input",
            r#type: "object",
            required: false,
            description: "Input parameters for the workflow",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "wait",
            r#type: "boolean",
            required: false,
            description: "Whether to wait for the workflow to finish",
            default_json: Some("true"),
            constraints: None,
        },
    ],
    tips: None,
    examples: Some(&["execute_workflow(\"llm_summary\", {\"text\": \"...\"})"]),
};
