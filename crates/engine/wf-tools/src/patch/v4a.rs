//! Codex-style (V4A) patch format: parsing and application.
//!
//! Turns an `*** Begin Patch` … `*** End Patch` document into [`PatchHunk`]s
//! and applies their update chunks to file content.

use crate::error::{ToolError, ToolResult};
use crate::patch::{check_patch_boundaries, validate_path, PatchHunk, PatchOp, UpdateFileChunk};
use crate::sequence_matcher::seek_sequence;

const ADD_FILE_MARKER: &str = "*** Add File: ";
const DELETE_FILE_MARKER: &str = "*** Delete File: ";
const UPDATE_FILE_MARKER: &str = "*** Update File: ";
const MOVE_TO_MARKER: &str = "*** Move to: ";
const EOF_MARKER: &str = "*** End of File";
const CHANGE_CONTEXT_MARKER: &str = "@@ ";
const EMPTY_CHANGE_CONTEXT_MARKER: &str = "@@";

fn parse_update_file_chunk(
    lines: &[String],
    line_number: usize,
    allow_missing_context: bool,
) -> ToolResult<(UpdateFileChunk, usize)> {
    if lines.is_empty() {
        return Err(ToolError::ValidationFailed(format!(
            "Invalid patch at line {}: update hunk does not contain any lines",
            line_number
        )));
    }

    let (change_context, start_index) = if lines[0] == EMPTY_CHANGE_CONTEXT_MARKER {
        (None, 1)
    } else if lines[0].starts_with(CHANGE_CONTEXT_MARKER) {
        (Some(lines[0][CHANGE_CONTEXT_MARKER.len()..].to_string()), 1)
    } else if allow_missing_context {
        (None, 0)
    } else {
        return Err(ToolError::ValidationFailed(format!(
            "Invalid patch at line {}: expected update hunk to start with a '@@' context marker, got '{}'",
            line_number, lines[0]
        )));
    };

    if start_index >= lines.len() {
        return Err(ToolError::ValidationFailed(format!(
            "Invalid patch at line {}: update hunk does not contain any lines",
            line_number + 1
        )));
    }

    let mut chunk = UpdateFileChunk {
        change_context,
        old_lines: Vec::new(),
        new_lines: Vec::new(),
        is_end_of_file: false,
    };
    let mut parsed_lines = 0usize;
    let mut i = start_index;
    while i < lines.len() {
        let line = &lines[i];

        if line == EOF_MARKER {
            if parsed_lines == 0 {
                return Err(ToolError::ValidationFailed(format!(
                    "Invalid patch at line {}: update hunk does not contain any lines",
                    line_number + 1
                )));
            }
            chunk.is_end_of_file = true;
            parsed_lines += 1;
            break;
        }

        let first_char = line.chars().next().unwrap_or(' ');
        if line.is_empty() {
            chunk.old_lines.push(String::new());
            chunk.new_lines.push(String::new());
            parsed_lines += 1;
        } else {
            match first_char {
                ' ' => {
                    chunk.old_lines.push(line[1..].to_string());
                    chunk.new_lines.push(line[1..].to_string());
                    parsed_lines += 1;
                }
                '+' => {
                    chunk.new_lines.push(line[1..].to_string());
                    parsed_lines += 1;
                }
                '-' => {
                    chunk.old_lines.push(line[1..].to_string());
                    parsed_lines += 1;
                }
                _ => {
                    if parsed_lines == 0 {
                        return Err(ToolError::ValidationFailed(format!(
                            "Invalid patch at line {}: unexpected line '{}'. Every line should start with ' ' (context), '+' (added) or '-' (removed)",
                            line_number + 1,
                            line
                        )));
                    }
                    // Assume this starts the next hunk.
                    break;
                }
            }
        }
        i += 1;
    }
    let lines_consumed = parsed_lines + start_index;
    Ok((chunk, lines_consumed))
}

fn parse_one_hunk(lines: &[String], line_number: usize) -> ToolResult<(PatchHunk, usize)> {
    let first_line = lines[0].trim().to_string();

    if let Some(path) = first_line.strip_prefix(ADD_FILE_MARKER) {
        validate_path(path, line_number)?;
        let mut contents = String::new();
        let mut parsed_lines = 1usize;
        for (i, line) in lines.iter().enumerate().skip(1) {
            if let Some(content) = line.strip_prefix('+') {
                contents.push_str(content);
                contents.push('\n');
                parsed_lines += 1;
            } else if !line.trim().is_empty() && !line.starts_with("***") {
                return Err(ToolError::ValidationFailed(format!(
                    "Invalid patch at line {}: unexpected content in Add File section: '{}'",
                    line_number + i,
                    line
                )));
            } else {
                break;
            }
        }
        return Ok((
            PatchHunk {
                op: PatchOp::AddFile,
                path: path.to_string(),
                contents,
                move_path: None,
                chunks: Vec::new(),
            },
            parsed_lines,
        ));
    }

    if let Some(path) = first_line.strip_prefix(DELETE_FILE_MARKER) {
        validate_path(path, line_number)?;
        return Ok((
            PatchHunk {
                op: PatchOp::DeleteFile,
                path: path.to_string(),
                contents: String::new(),
                move_path: None,
                chunks: Vec::new(),
            },
            1,
        ));
    }

    if let Some(path) = first_line.strip_prefix(UPDATE_FILE_MARKER) {
        validate_path(path, line_number)?;
        let mut remaining = lines[1..].to_vec();
        let mut parsed_lines = 1usize;

        let mut move_path: Option<String> = None;
        if let Some(mp) = remaining
            .first()
            .and_then(|l| l.strip_prefix(MOVE_TO_MARKER))
        {
            validate_path(mp, line_number + parsed_lines)?;
            move_path = Some(mp.to_string());
            remaining.remove(0);
            parsed_lines += 1;
        }

        let mut chunks: Vec<UpdateFileChunk> = Vec::new();
        while !remaining.is_empty() {
            if remaining[0].trim().is_empty() {
                parsed_lines += 1;
                remaining.remove(0);
                continue;
            }
            if remaining[0].starts_with("***") {
                break;
            }
            let (chunk, consumed) =
                parse_update_file_chunk(&remaining, line_number + parsed_lines, chunks.is_empty())?;
            chunks.push(chunk);
            parsed_lines += consumed;
            remaining.drain(..consumed);
        }

        if chunks.is_empty() {
            return Err(ToolError::ValidationFailed(format!(
                "Invalid patch at line {}: Update File '{}' does not contain any update hunks",
                line_number, path
            )));
        }

        return Ok((
            PatchHunk {
                op: PatchOp::UpdateFile,
                path: path.to_string(),
                contents: String::new(),
                move_path,
                chunks,
            },
            parsed_lines,
        ));
    }

    Err(ToolError::ValidationFailed(format!(
        "Invalid patch at line {}: unexpected file header '{}'",
        line_number, first_line
    )))
}

/// Parse an apply_patch patch string into structured hunks.
pub fn parse_patch(patch: &str) -> ToolResult<Vec<PatchHunk>> {
    let trimmed = patch.trim();
    let mut lines: Vec<String> = trimmed.split('\n').map(String::from).collect();

    // Handle heredoc-wrapped patches (lenient mode).
    if lines.len() >= 4 {
        let first = lines[0].as_str();
        let last = lines[lines.len() - 1].as_str();
        if matches!(first, "<<EOF" | "<<'EOF'" | "<<\"EOF\"") && last.ends_with("EOF") {
            lines.drain(0..1);
            lines.pop();
        }
    }

    check_patch_boundaries(&lines)?;

    let last_index = lines.len() - 1;
    let mut remaining = lines[1..last_index].to_vec();
    let mut line_number = 2usize;
    let mut hunks = Vec::new();

    while !remaining.is_empty() {
        let (hunk, consumed) = parse_one_hunk(&remaining, line_number)?;
        hunks.push(hunk);
        line_number += consumed;
        remaining.drain(..consumed);
    }

    Ok(hunks)
}

/// Compute the replacements needed to transform original_lines into the new
/// lines. Each replacement is (start_index, old_len, new_lines).
fn compute_replacements(
    original_lines: &[String],
    file_path: &str,
    chunks: &[UpdateFileChunk],
) -> ToolResult<Vec<(usize, usize, Vec<String>)>> {
    let mut replacements = Vec::new();
    let mut line_index = 0usize;

    for chunk in chunks {
        if let Some(context) = &chunk.change_context {
            let idx = seek_sequence(
                original_lines,
                std::slice::from_ref(context),
                line_index,
                false,
            )
            .ok_or_else(|| {
                ToolError::ExecutionError(format!(
                    "Context '{}' not found in '{}'",
                    context, file_path
                ))
            })?;
            line_index = idx + 1;
        }

        if chunk.old_lines.is_empty() {
            // Pure addition (no old lines). Add at the end or before the
            // final empty line.
            let insertion_idx = if !original_lines.is_empty()
                && original_lines[original_lines.len() - 1].is_empty()
            {
                original_lines.len() - 1
            } else {
                original_lines.len()
            };
            replacements.push((insertion_idx, 0, chunk.new_lines.clone()));
            continue;
        }

        // Try to find the old lines in the file.
        let mut pattern = chunk.old_lines.clone();
        let mut new_slice = chunk.new_lines.clone();
        let mut found = seek_sequence(original_lines, &pattern, line_index, chunk.is_end_of_file);

        // If not found and the pattern ends with an empty string (trailing
        // newline), retry without it.
        if found.is_none() && pattern.last().map(|l| l.is_empty()).unwrap_or(false) {
            pattern.pop();
            if new_slice.last().map(|l| l.is_empty()).unwrap_or(false) {
                new_slice.pop();
            }
            found = seek_sequence(original_lines, &pattern, line_index, chunk.is_end_of_file);
        }

        if let Some(idx) = found {
            replacements.push((idx, pattern.len(), new_slice));
            line_index = idx + pattern.len();
        } else {
            return Err(ToolError::ExecutionError(format!(
                "Old lines not found in '{}'. Lines:\n{}",
                file_path,
                chunk.old_lines.join("\n")
            )));
        }
    }

    // Sort replacements by start index so they can be applied in reverse.
    replacements.sort_by_key(|r| r.0);
    Ok(replacements)
}

/// Apply chunks to file content, returning the new content.
pub fn apply_chunks_to_content(
    original_content: &str,
    file_path: &str,
    chunks: &[UpdateFileChunk],
) -> ToolResult<String> {
    let mut original_lines: Vec<String> = original_content.split('\n').map(String::from).collect();
    // Drop the trailing empty element that results from a final newline so
    // line counts match standard diff behavior.
    if original_lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        original_lines.pop();
    }

    let replacements = compute_replacements(&original_lines, file_path, chunks)?;

    let mut result = original_lines;
    // Apply in reverse order so earlier replacements don't shift later indices.
    for (start_idx, old_len, new_segment) in replacements.into_iter().rev() {
        result.splice(start_idx..start_idx + old_len, new_segment);
    }

    // Ensure the file ends with a newline.
    if result.is_empty() || result.last().map(|l| !l.is_empty()).unwrap_or(true) {
        result.push(String::new());
    }

    Ok(result.join("\n"))
}
