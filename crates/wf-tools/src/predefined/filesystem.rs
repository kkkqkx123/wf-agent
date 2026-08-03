//! Predefined filesystem tools: definitions + handler wiring.
//!
//! Tools: read_file, write_file, edit_file, apply_patch, apply_diff,
//! list_files, grep_search, glob_search.

use wf_types::tool::ToolType;

use super::schema::{ToolDefinition, ToolParameter};
use crate::error::ToolResult;
use crate::filesystem::FsToolHandlers;
use crate::registry::ToolRegistry;

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

pub static EDIT_FILE: ToolDefinition = ToolDefinition {
    id: "edit_file",
    tool_type: ToolType::Stateless,
    category: "filesystem",
    tags: &["edit", "file"],
    description: "Perform an exact string replacement in a file. Replaces the first occurrence of old_string with new_string.",
    parameters: &[
        ToolParameter { name: "file_path", r#type: "string", required: true, description: "Absolute path to the file to edit", default_json: None },
        ToolParameter { name: "old_string", r#type: "string", required: true, description: "The exact text to search for (must be unique)", default_json: None },
        ToolParameter { name: "new_string", r#type: "string", required: true, description: "The replacement text", default_json: None },
    ],
    tips: Some(&["Use unique context around the edit target"]),
    examples: Some(&["edit_file(\"src/main.rs\", \"old_function()\", \"new_function()\")"]),
};

pub static APPLY_PATCH: ToolDefinition = ToolDefinition {
    id: "apply_patch",
    tool_type: ToolType::Stateless,
    category: "filesystem",
    tags: &["patch", "diff"],
    description: "Apply a Codex-style patch to the filesystem. The patch is a sequence of Add File, Delete File and Update File operations delimited by '*** Begin Patch' and '*** End Patch'.",
    parameters: &[
        ToolParameter { name: "patch", r#type: "string", required: true, description: "The patch content in Codex apply_patch format", default_json: None },
    ],
    tips: Some(&["Use Update File with @@ context markers for reliable matches"]),
    examples: Some(&["apply_patch(\"*** Begin Patch\\n*** Add File: new.txt\\n+hello\\n*** End Patch\")"]),
};

pub static APPLY_DIFF: ToolDefinition = ToolDefinition {
    id: "apply_diff",
    tool_type: ToolType::Stateless,
    category: "filesystem",
    tags: &["diff", "search-replace"],
    description: "Apply SEARCH/REPLACE blocks to modify a file. Each block contains a SEARCH section and a REPLACE section delimited by '<<<<<<< SEARCH' and '>>>>>>> REPLACE'.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "Absolute path to the file to modify", default_json: None },
        ToolParameter { name: "diff", r#type: "string", required: true, description: "The SEARCH/REPLACE diff content", default_json: None },
    ],
    tips: None,
    examples: None,
};

pub static LIST_FILES: ToolDefinition = ToolDefinition {
    id: "list_files",
    tool_type: ToolType::Stateless,
    category: "filesystem",
    tags: &["list", "file"],
    description: "List files and directories at the given path. Can be recursive.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "Absolute path to the directory", default_json: None },
        ToolParameter { name: "recursive", r#type: "boolean", required: false, description: "Whether to list recursively", default_json: None },
    ],
    tips: None,
    examples: Some(&["list_files(\"/home/user/project/src\")"]),
};

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

/// All filesystem tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[
    &READ_FILE,
    &WRITE_FILE,
    &EDIT_FILE,
    &APPLY_PATCH,
    &APPLY_DIFF,
    &LIST_FILES,
    &GREP_SEARCH,
    &GLOB_SEARCH,
];

/// Register the filesystem tool handlers (including apply_patch and
/// apply_diff) into the registry.
pub fn register_handlers(registry: &ToolRegistry, handlers: &FsToolHandlers) -> ToolResult<()> {
    for tool_name in [
        "read_file",
        "write_file",
        "edit_file",
        "apply_patch",
        "apply_diff",
        "list_files",
        "grep_search",
        "glob_search",
    ] {
        let handler = handlers.handler(tool_name)?;
        registry.register_stateless_handler(tool_name, handler);
    }
    Ok(())
}
