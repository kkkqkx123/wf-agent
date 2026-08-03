//! Definition of the write_file tool.

use wf_types::tool::ToolType;

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static WRITE_FILE: ToolDefinition = ToolDefinition {
    id: "write_file",
    tool_type: ToolType::Stateless,
    category: "filesystem",
    tags: &["write", "file"],
    description: "Write content to a file at the given path. Creates the file and any missing parent directories; overwrites existing content.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "Absolute path to the file", default_json: None },
        ToolParameter { name: "content", r#type: "string", required: true, description: "Content to write to the file", default_json: None },
    ],
    tips: Some(&["Always read the file first before overwriting"]),
    examples: None,
};
