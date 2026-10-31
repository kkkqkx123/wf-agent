//! Definition of the glob_search tool.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static GLOB_SEARCH: ToolDefinition = ToolDefinition {
    id: "glob_search",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "filesystem",
    tags: &["glob", "search"],
    description: "Find files matching a glob pattern. Returns matching file paths relative to the search path. Entries hidden by ignore rules are skipped and counted; pass include_ignored=true to reveal them.",
    parameters: &[
        ToolParameter { name: "pattern", r#type: "string", required: true, description: "The glob pattern to match", default_json: None, constraints: None },
        ToolParameter { name: "path", r#type: "string", required: true, description: "The directory to search in (relative paths resolve against the execution working directory)", default_json: None, constraints: None },
        ToolParameter { name: "include_ignored", r#type: "boolean", required: false, description: "Reveal entries hidden by ignore rules (default false); execution scope still applies", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["glob_search(\"**/*.rs\", \"/home/user/project\")"]),
};
