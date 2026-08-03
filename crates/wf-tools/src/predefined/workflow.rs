//! Predefined workflow tools (builtin type): definitions only. Execution is
//! handled by the BuiltinExecutor through the registered ExecutionCallback.

use wf_types::tool::ToolType;

use super::schema::{ToolDefinition, ToolParameter};

pub static EXECUTE_WORKFLOW: ToolDefinition = ToolDefinition {
    id: "execute_workflow",
    tool_type: ToolType::BuiltIn,
    category: "workflow",
    tags: &["execute"],
    description: "Execute a predefined workflow by ID. Workflows are multi-step automation sequences.",
    parameters: &[
        ToolParameter { name: "workflow_id", r#type: "string", required: true, description: "The ID of the workflow to execute", default_json: None },
        ToolParameter { name: "input", r#type: "object", required: false, description: "Input parameters for the workflow", default_json: None },
        ToolParameter { name: "wait", r#type: "boolean", required: false, description: "Whether to wait for the workflow to finish", default_json: Some("true") },
    ],
    tips: None,
    examples: Some(&["execute_workflow(\"llm_summary\", {\"text\": \"...\"})"]),
};

pub static QUERY_WORKFLOW_STATUS: ToolDefinition = ToolDefinition {
    id: "query_workflow_status",
    tool_type: ToolType::BuiltIn,
    category: "workflow",
    tags: &["query", "status"],
    description: "Query the status of a running or completed workflow.",
    parameters: &[
        ToolParameter { name: "workflow_id", r#type: "string", required: true, description: "The ID of the workflow to query", default_json: None },
        ToolParameter { name: "execution_id", r#type: "string", required: false, description: "Optional execution ID to query", default_json: None },
    ],
    tips: None,
    examples: Some(&["query_workflow_status(\"wf-123\")"]),
};

pub static CANCEL_WORKFLOW: ToolDefinition = ToolDefinition {
    id: "cancel_workflow",
    tool_type: ToolType::BuiltIn,
    category: "workflow",
    tags: &["cancel"],
    description: "Cancel a running workflow by ID.",
    parameters: &[
        ToolParameter { name: "workflow_id", r#type: "string", required: true, description: "The ID of the workflow to cancel", default_json: None },
        ToolParameter { name: "execution_id", r#type: "string", required: false, description: "Optional execution ID to cancel", default_json: None },
    ],
    tips: None,
    examples: Some(&["cancel_workflow(\"wf-123\")"]),
};

/// All workflow tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[
    &EXECUTE_WORKFLOW,
    &QUERY_WORKFLOW_STATUS,
    &CANCEL_WORKFLOW,
];
