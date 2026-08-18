//! Definition of the query_workflow_status tool (builtin type). Execution
//! is handled by the BuiltinExecutor through the registered
//! ExecutionCallback.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static QUERY_WORKFLOW_STATUS: ToolDefinition = ToolDefinition {
    id: "query_workflow_status",
    tool_type: ToolType::BuiltIn,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "workflow",
    tags: &["query", "status"],
    description: "Query the status of a running or completed workflow.",
    parameters: &[
        ToolParameter {
            name: "workflow_id",
            r#type: "string",
            required: true,
            description: "The ID of the workflow to query",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "execution_id",
            r#type: "string",
            required: false,
            description: "Optional execution ID to query",
            default_json: None,
            constraints: None,
        },
    ],
    tips: None,
    examples: Some(&["query_workflow_status(\"wf-123\")"]),
};
