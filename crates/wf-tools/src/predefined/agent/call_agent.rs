//! Definition of the call_agent tool (builtin type). Execution is handled
//! by the BuiltinExecutor through the registered ExecutionCallback.

use wf_types::tool::ToolType;

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static CALL_AGENT: ToolDefinition = ToolDefinition {
    id: "call_agent",
    tool_type: ToolType::BuiltIn,
    category: "agent",
    tags: &["call"],
    description:
        "Delegate a task to a sub-agent with a specific profile. The agent runs autonomously.",
    parameters: &[
        ToolParameter {
            name: "agent_profile_id",
            r#type: "string",
            required: true,
            description: "The agent profile ID to invoke",
            default_json: None,
        },
        ToolParameter {
            name: "prompt",
            r#type: "string",
            required: true,
            description: "The task prompt for the agent",
            default_json: None,
        },
        ToolParameter {
            name: "wait",
            r#type: "boolean",
            required: false,
            description: "Whether to wait for the agent to complete",
            default_json: Some("true"),
        },
    ],
    tips: None,
    examples: Some(&["call_agent(\"code-reviewer\", \"Review the code in src/\")"]),
};
