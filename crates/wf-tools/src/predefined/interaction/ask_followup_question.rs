//! Definition of the ask_followup_question tool (builtin type). It signals
//! the agent loop to pause for user input.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static ASK_FOLLOWUP_QUESTION: ToolDefinition = ToolDefinition {
    id: "ask_followup_question",
    tool_type: ToolType::BuiltIn,
    risk_level: ToolRiskLevel::Interaction,
    create_checkpoint: None,
    category: "interaction",
    tags: &["ask"],
    description:
        "Ask the user follow-up questions when more information is needed to complete the task.",
    parameters: &[
        ToolParameter {
            name: "question",
            r#type: "string",
            required: true,
            description: "The question to ask the user",
            default_json: None,
        },
        ToolParameter {
            name: "options",
            r#type: "array",
            required: false,
            description: "Optional multiple choice options",
            default_json: None,
        },
    ],
    tips: Some(&["Be concise, ask one question at a time when possible"]),
    examples: Some(&["ask_followup_question(\"What is the target directory?\")"]),
};
