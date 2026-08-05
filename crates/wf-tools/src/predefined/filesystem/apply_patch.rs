//! Definition of the apply_patch tool.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static APPLY_PATCH: ToolDefinition = ToolDefinition {
    id: "apply_patch",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::Write,
    create_checkpoint: None,
    category: "filesystem",
    tags: &["patch", "diff"],
    description: "Apply a Codex-style patch to the filesystem. The patch is a sequence of Add File, Delete File and Update File operations delimited by '*** Begin Patch' and '*** End Patch'.",
    parameters: &[
        ToolParameter { name: "patch", r#type: "string", required: true, description: "The patch content in Codex apply_patch format", default_json: None },
    ],
    tips: Some(&["Use Update File with @@ context markers for reliable matches"]),
    examples: Some(&["apply_patch(\"*** Begin Patch\\n*** Add File: new.txt\\n+hello\\n*** End Patch\")"]),
};
