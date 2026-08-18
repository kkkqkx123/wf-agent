//! Definition of the apply_diff tool.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static APPLY_DIFF: ToolDefinition = ToolDefinition {
    id: "apply_diff",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::Write,
    create_checkpoint: Some(wf_types::tool::CheckpointTiming::Before),
    category: "filesystem",
    tags: &["diff", "search-replace"],
    description: "Apply SEARCH/REPLACE blocks to modify a file. Each block contains a SEARCH section and a REPLACE section delimited by '<<<<<<< SEARCH' and '>>>>>>> REPLACE'.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "Absolute path to the file to modify", default_json: None, constraints: None },
        ToolParameter { name: "diff", r#type: "string", required: true, description: "The SEARCH/REPLACE diff content", default_json: None, constraints: None },
    ],
    tips: None,
    examples: None,
};
