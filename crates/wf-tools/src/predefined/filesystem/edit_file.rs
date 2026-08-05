//! Definition of the edit_file tool.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static EDIT_FILE: ToolDefinition = ToolDefinition {
    id: "edit_file",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::Write,
    create_checkpoint: None,
    category: "filesystem",
    tags: &["edit", "file"],
    description: "Perform an exact string replacement in a file. Replaces the first occurrence of old_string with new_string.",
    parameters: &[
        ToolParameter { name: "file_path", r#type: "string", required: true, description: "Absolute path to the file to edit", default_json: None },
        ToolParameter { name: "old_string", r#type: "string", required: true, description: "The exact text to search for (must be unique)", default_json: None },
        ToolParameter { name: "new_string", r#type: "string", required: true, description: "The replacement text", default_json: None },
    ],
    tips: Some(&["Use unique context around the edit target"]),
    examples: Some(&["edit_file(\"src/main.rs\", \"old_function()\", \"new_function()\")"]),
};
