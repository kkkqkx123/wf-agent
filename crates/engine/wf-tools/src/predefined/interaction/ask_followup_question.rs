//! Definition of the ask_followup_question tool (builtin type). It signals
//! the agent loop to pause for user input.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter, ToolParameterConstraint};

static OPTIONS_ITEM: ToolParameter = ToolParameter {
    name: "option",
    r#type: "string",
    required: false,
    description: "One multiple choice option",
    default_json: None,
    constraints: None,
};

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
            constraints: None,
        },
        ToolParameter {
            name: "options",
            r#type: "array",
            required: false,
            description: "Optional multiple choice options",
            default_json: None,
            constraints: Some(&ToolParameterConstraint {
                enum_values: None,
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                min_items: None,
                max_items: None,
                items: Some(&OPTIONS_ITEM),
            }),
        },
    ],
    tips: Some(&["Be concise, ask one question at a time when possible"]),
    examples: Some(&["ask_followup_question(\"What is the target directory?\")"]),
};
