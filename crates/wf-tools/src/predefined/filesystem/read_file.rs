//! Definition of the read_file tool.

use wf_types::tool::ToolType;

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static READ_FILE: ToolDefinition = ToolDefinition {
    id: "read_file",
    tool_type: ToolType::Stateless,
    category: "filesystem",
    tags: &["read", "file"],
    description: "Read the contents of a file at the given path. Returns the file content as text. Supports line-range and offset-based slicing.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "Absolute path to the file", default_json: None },
        ToolParameter { name: "offset", r#type: "number", required: false, description: "Line number to start reading from (1-indexed)", default_json: None },
        ToolParameter { name: "limit", r#type: "number", required: false, description: "Maximum number of lines to read", default_json: None },
    ],
    tips: Some(&["Use absolute paths whenever possible"]),
    examples: Some(&["read_file(\"/home/user/project/src/main.rs\")"]),
};
