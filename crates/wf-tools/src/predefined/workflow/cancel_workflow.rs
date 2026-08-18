//! Definition of the cancel_workflow tool (builtin type). Execution is
//! handled by the BuiltinExecutor through the registered ExecutionCallback.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static CANCEL_WORKFLOW: ToolDefinition = ToolDefinition {
    id: "cancel_workflow",
    tool_type: ToolType::BuiltIn,
    risk_level: ToolRiskLevel::System,
    create_checkpoint: None,
    category: "workflow",
    tags: &["cancel"],
    description: "Cancel a running workflow by ID.",
    parameters: &[
        ToolParameter {
            name: "workflow_id",
            r#type: "string",
            required: true,
            description: "The ID of the workflow to cancel",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "execution_id",
            r#type: "string",
            required: false,
            description: "Optional execution ID to cancel",
            default_json: None,
            constraints: None,
        },
    ],
    tips: None,
    examples: Some(&["cancel_workflow(\"wf-123\")"]),
};
