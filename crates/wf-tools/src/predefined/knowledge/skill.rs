//! Definition of the skill tool (builtin type). Execution is handled by the
//! BuiltinExecutor through the registered ExecutionCallback.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static SKILL: ToolDefinition = ToolDefinition {
    id: "skill",
    tool_type: ToolType::BuiltIn,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "knowledge",
    tags: &["skill"],
    description: "Load and apply a skill by name. Skills provide specialized instructions and workflows for common tasks.",
    parameters: &[
        ToolParameter { name: "skill", r#type: "string", required: true, description: "The skill name to load", default_json: None },
        ToolParameter { name: "args", r#type: "object", required: false, description: "Optional key-value pairs passed as template variables to the skill. Substituted into {{name}} placeholders in the skill content.", default_json: None },
    ],
    tips: None,
    examples: Some(&["skill(\"analyze-data\")"]),
};
