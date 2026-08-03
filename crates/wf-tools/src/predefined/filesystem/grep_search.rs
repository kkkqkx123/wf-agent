//! Definition of the grep_search tool.

use wf_types::tool::ToolType;

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static GREP_SEARCH: ToolDefinition = ToolDefinition {
    id: "grep_search",
    tool_type: ToolType::Stateless,
    category: "filesystem",
    tags: &["grep", "search"],
    description: "Search file contents using a regular expression pattern. Returns matching file paths and line numbers.",
    parameters: &[
        ToolParameter { name: "pattern", r#type: "string", required: true, description: "The regex pattern to search for", default_json: None },
        ToolParameter { name: "path", r#type: "string", required: true, description: "The directory to search in", default_json: None },
        ToolParameter { name: "include", r#type: "string", required: false, description: "File glob pattern to include (e.g. *.rs)", default_json: None },
    ],
    tips: Some(&["Use specific patterns to narrow results"]),
    examples: Some(&["grep_search(\"fn main\", \".\", \"*.rs\")"]),
};
