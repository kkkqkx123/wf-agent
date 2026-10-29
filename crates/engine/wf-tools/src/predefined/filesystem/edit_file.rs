//! Definition of the edit_file tool.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static EDIT_FILE: ToolDefinition = ToolDefinition {
    id: "edit_file",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::Write,
    create_checkpoint: Some(wf_types::tool::ToolCheckpointTiming::Before),
    category: "filesystem",
    tags: &["edit", "file"],
    description: "Edit a single file using exact text replacement. Every old_string must match a unique, non-overlapping region of the original file. Accepts one old_string/new_string pair or an edits array of multiple replacements applied atomically in one call.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "Absolute path to the file to edit", default_json: None, constraints: None },
        ToolParameter { name: "old_string", r#type: "string", required: false, description: "The exact text to search for (must be unique). Use this or edits, not both", default_json: None, constraints: None },
        ToolParameter { name: "new_string", r#type: "string", required: false, description: "The replacement text for old_string", default_json: None, constraints: None },
        ToolParameter { name: "edits", r#type: "array", required: false, description: "Multiple replacements applied atomically. Each entry has old_string and new_string; every old_string must match a unique, non-overlapping region. Use this or old_string/new_string, not both", default_json: None, constraints: None },
    ],
    tips: Some(&["Use unique context around the edit target", "When changing multiple separate locations in one file, use one edit call with an edits array instead of multiple edit calls", "If two changes touch the same block or nearby lines, merge them into one edit instead"]),
    examples: Some(&["edit_file(\"src/main.rs\", \"old_function()\", \"new_function()\")"]),
};
