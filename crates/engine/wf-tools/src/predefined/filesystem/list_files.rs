//! Definition of the list_files tool.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static LIST_FILES: ToolDefinition = ToolDefinition {
    id: "list_files",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "filesystem",
    tags: &["list", "file"],
    description: "List files and directories at the given path. Can be recursive. Entries hidden by ignore rules are counted and noted; pass include_ignored=true to reveal them. Out-of-scope entries are counted and noted separately.",
    parameters: &[
        ToolParameter {
            name: "path",
            r#type: "string",
            required: true,
            description: "Absolute path to the directory",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "recursive",
            r#type: "boolean",
            required: false,
            description: "Whether to list recursively",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "include_ignored",
            r#type: "boolean",
            required: false,
            description: "Reveal entries hidden by ignore rules (default false); execution scope still applies",
            default_json: None,
            constraints: None,
        },
    ],
    tips: None,
    examples: Some(&["list_files(\"/home/user/project/src\")"]),
};
