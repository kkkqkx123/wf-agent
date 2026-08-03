//! Definition of the attempt_completion tool (builtin type). It signals the
//! agent loop that the task is complete.

use wf_types::tool::ToolType;

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static ATTEMPT_COMPLETION: ToolDefinition = ToolDefinition {
    id: "attempt_completion",
    tool_type: ToolType::BuiltIn,
    category: "interaction",
    tags: &["complete"],
    description: "Signal that the task is complete and present the final result to the user.",
    parameters: &[
        ToolParameter {
            name: "result",
            r#type: "string",
            required: true,
            description: "Summary of what was accomplished",
            default_json: None,
        },
        ToolParameter {
            name: "variables",
            r#type: "object",
            required: false,
            description: "State variable changes",
            default_json: None,
        },
    ],
    tips: Some(&["Only call this when the task is fully complete"]),
    examples: Some(&["attempt_completion(\"Task complete. Created 3 files.\")"]),
};
