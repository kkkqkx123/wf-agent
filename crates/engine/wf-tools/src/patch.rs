//! Patch parsing and application for the apply_patch and apply_diff tools.
//!
//! - `apply_patch` uses the Codex-style (V4A) patch format:
//!   `*** Begin Patch` / `*** Add File:` / `*** Delete File:` /
//!   `*** Update File:` / `*** Move to:` / `@@ context` / `*** End of File` /
//!   `*** End Patch` ([`v4a`]).
//! - `apply_diff` uses the SEARCH/REPLACE block format:
//!   `<<<<<<< SEARCH` / `=======` / `>>>>>>> REPLACE` ([`search_replace`]).
//!
//! This module owns the shared patch types and the validation helpers used
//! by both format modules; each format parser lives in its own submodule.

pub mod search_replace;
pub mod v4a;

#[cfg(test)]
mod tests;

pub use search_replace::{
    apply_block, parse_search_replace_blocks, validate_marker_sequencing, SearchReplaceBlock,
};
pub use v4a::{apply_chunks_to_content, parse_patch};

use crate::error::{ToolError, ToolResult};

const BEGIN_PATCH_MARKER: &str = "*** Begin Patch";
const END_PATCH_MARKER: &str = "*** End Patch";

/// A single update chunk inside an UpdateFile hunk.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateFileChunk {
    pub change_context: Option<String>,
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
    pub is_end_of_file: bool,
}

/// The kind of file operation in a patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchOp {
    AddFile,
    DeleteFile,
    UpdateFile,
}

/// A parsed file operation from an apply_patch patch.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchHunk {
    pub op: PatchOp,
    pub path: String,
    /// Contents for AddFile hunks (lines joined with '\n').
    pub contents: String,
    /// Optional move destination for UpdateFile hunks.
    pub move_path: Option<String>,
    /// Update chunks for UpdateFile hunks.
    pub chunks: Vec<UpdateFileChunk>,
}

pub(crate) fn validate_path(path: &str, line_number: usize) -> ToolResult<()> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(ToolError::ValidationFailed(format!(
            "Invalid patch path at line {}: path is empty",
            line_number
        )));
    }
    if trimmed.starts_with('/') || trimmed.chars().nth(1) == Some(':') {
        return Err(ToolError::ValidationFailed(format!(
            "Invalid patch path at line {}: absolute paths are not allowed ('{}')",
            line_number, trimmed
        )));
    }
    if trimmed.contains("..") {
        return Err(ToolError::ValidationFailed(format!(
            "Invalid patch path at line {}: path traversal detected ('{}')",
            line_number, trimmed
        )));
    }
    if trimmed
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return Err(ToolError::ValidationFailed(format!(
            "Invalid patch path at line {}: filename contains invalid characters ('{}')",
            line_number, trimmed
        )));
    }
    Ok(())
}

pub(crate) fn check_patch_boundaries(lines: &[String]) -> ToolResult<()> {
    if lines.is_empty() {
        return Err(ToolError::ValidationFailed(
            "Invalid patch: empty patch".into(),
        ));
    }
    let first = lines[0].trim();
    let last = lines[lines.len() - 1].trim();
    if first != BEGIN_PATCH_MARKER {
        return Err(ToolError::ValidationFailed(
            "Invalid patch: the first line must be '*** Begin Patch'".into(),
        ));
    }
    if last != END_PATCH_MARKER {
        return Err(ToolError::ValidationFailed(
            "Invalid patch: the last line must be '*** End Patch'".into(),
        ));
    }
    Ok(())
}
