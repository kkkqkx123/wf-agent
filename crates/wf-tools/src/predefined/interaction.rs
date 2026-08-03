//! Predefined interaction tools (builtin type): definitions only. These
//! signal the agent loop to pause for user input or to complete the task.

use wf_types::tool::ToolType;

use super::schema::{ToolDefinition, ToolParameter};

pub static ASK_FOLLOWUP_QUESTION: ToolDefinition = ToolDefinition {
    id: "ask_followup_question",
    tool_type: ToolType::BuiltIn,
    category: "interaction",
    tags: &["ask"],
    description: "Ask the user follow-up questions when more information is needed to complete the task.",
    parameters: &[
        ToolParameter { name: "question", r#type: "string", required: true, description: "The question to ask the user", default_json: None },
        ToolParameter { name: "options", r#type: "array", required: false, description: "Optional multiple choice options", default_json: None },
    ],
    tips: Some(&["Be concise, ask one question at a time when possible"]),
    examples: Some(&["ask_followup_question(\"What is the target directory?\")"]),
};

pub static ATTEMPT_COMPLETION: ToolDefinition = ToolDefinition {
    id: "attempt_completion",
    tool_type: ToolType::BuiltIn,
    category: "interaction",
    tags: &["complete"],
    description: "Signal that the task is complete and present the final result to the user.",
    parameters: &[
        ToolParameter { name: "result", r#type: "string", required: true, description: "Summary of what was accomplished", default_json: None },
        ToolParameter { name: "variables", r#type: "object", required: false, description: "State variable changes", default_json: None },
    ],
    tips: Some(&["Only call this when the task is fully complete"]),
    examples: Some(&["attempt_completion(\"Task complete. Created 3 files.\")"]),
};

/// All interaction tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&ASK_FOLLOWUP_QUESTION, &ATTEMPT_COMPLETION];
