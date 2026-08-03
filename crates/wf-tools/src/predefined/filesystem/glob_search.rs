//! Definition of the glob_search tool.

use wf_types::tool::ToolType;

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static GLOB_SEARCH: ToolDefinition = ToolDefinition {
    id: "glob_search",
    tool_type: ToolType::Stateless,
    category: "filesystem",
    tags: &["glob", "search"],
    description: "Find files matching a glob pattern. Returns matching file paths relative to the search path.",
    parameters: &[
        ToolParameter { name: "pattern", r#type: "string", required: true, description: "The glob pattern to match", default_json: None },
        ToolParameter { name: "path", r#type: "string", required: true, description: "The directory to search in", default_json: None },
    ],
    tips: None,
    examples: Some(&["glob_search(\"**/*.rs\", \"/home/user/project\")"]),
};
