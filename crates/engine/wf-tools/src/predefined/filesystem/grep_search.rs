//! Definition of the grep_search tool.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static GREP_SEARCH: ToolDefinition = ToolDefinition {
    id: "grep_search",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "filesystem",
    tags: &["grep", "search"],
    description: "Search file contents using a regular expression pattern. Returns matching file paths and line numbers. Files hidden by ignore rules are skipped and counted; pass include_ignored=true to search them too.",
    parameters: &[
        ToolParameter { name: "pattern", r#type: "string", required: true, description: "The regex pattern to search for", default_json: None, constraints: None },
        ToolParameter { name: "path", r#type: "string", required: true, description: "The directory to search in", default_json: None, constraints: None },
        ToolParameter { name: "include", r#type: "string", required: false, description: "File glob pattern to include (e.g. *.rs)", default_json: None, constraints: None },
        ToolParameter { name: "include_ignored", r#type: "boolean", required: false, description: "Search files hidden by ignore rules too (default false); execution scope still applies", default_json: None, constraints: None },
    ],
    tips: Some(&["Use specific patterns to narrow results"]),
    examples: Some(&["grep_search(\"fn main\", \".\", \"*.rs\")"]),
};
