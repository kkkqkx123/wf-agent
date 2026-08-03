//! Definition of the list_files tool.

use wf_types::tool::ToolType;

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static LIST_FILES: ToolDefinition = ToolDefinition {
    id: "list_files",
    tool_type: ToolType::Stateless,
    category: "filesystem",
    tags: &["list", "file"],
    description: "List files and directories at the given path. Can be recursive.",
    parameters: &[
        ToolParameter {
            name: "path",
            r#type: "string",
            required: true,
            description: "Absolute path to the directory",
            default_json: None,
        },
        ToolParameter {
            name: "recursive",
            r#type: "boolean",
            required: false,
            description: "Whether to list recursively",
            default_json: None,
        },
    ],
    tips: None,
    examples: Some(&["list_files(\"/home/user/project/src\")"]),
};
