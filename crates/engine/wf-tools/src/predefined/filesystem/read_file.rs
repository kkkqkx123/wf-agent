//! Definition of the read_file tool.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static READ_FILE: ToolDefinition = ToolDefinition {
    id: "read_file",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "filesystem",
    tags: &["read", "file"],
    description: "Read the contents of a file at the given path. Returns the file content as text. Supports line-range and offset-based slicing. Direct reads bypass discovery ignore rules; execution scope still applies.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "Path to the file (relative paths resolve against the execution working directory)", default_json: None, constraints: None },
        ToolParameter { name: "offset", r#type: "number", required: false, description: "Line number to start reading from (1-indexed)", default_json: None, constraints: None },
        ToolParameter { name: "limit", r#type: "number", required: false, description: "Maximum number of lines to read", default_json: None, constraints: None },
    ],
    tips: Some(&["Prefer relative paths anchored at the working directory"]),
    examples: Some(&["read_file(\"/home/user/project/src/main.rs\")"]),
};
