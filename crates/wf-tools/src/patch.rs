//! Patch parsing and application for the apply_patch and apply_diff tools.
//!
//! - `apply_patch` uses the Codex-style patch format:
//!   `*** Begin Patch` / `*** Add File:` / `*** Delete File:` /
//!   `*** Update File:` / `*** Move to:` / `@@ context` / `*** End of File` /
//!   `*** End Patch`.
//! - `apply_diff` uses the SEARCH/REPLACE block format:
//!   `<<<<<<< SEARCH` / `=======` / `>>>>>>> REPLACE`.

use crate::error::{ToolError, ToolResult};
use crate::sequence_matcher::seek_sequence;

// ── apply_patch (Codex format) ─────────────────────────────

const BEGIN_PATCH_MARKER: &str = "*** Begin Patch";
const END_PATCH_MARKER: &str = "*** End Patch";
const ADD_FILE_MARKER: &str = "*** Add File: ";
const DELETE_FILE_MARKER: &str = "*** Delete File: ";
const UPDATE_FILE_MARKER: &str = "*** Update File: ";
const MOVE_TO_MARKER: &str = "*** Move to: ";
const EOF_MARKER: &str = "*** End of File";
const CHANGE_CONTEXT_MARKER: &str = "@@ ";
const EMPTY_CHANGE_CONTEXT_MARKER: &str = "@@";

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

fn validate_path(path: &str, line_number: usize) -> ToolResult<()> {
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

fn check_patch_boundaries(lines: &[String]) -> ToolResult<()> {
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

// ── apply_diff (SEARCH/REPLACE) ────────────────────────────

const SEARCH_MARKER: &str = "<<<<<<< SEARCH";
const SEPARATOR_MARKER: &str = "=======";
const REPLACE_MARKER: &str = ">>>>>>> REPLACE";

/// A parsed SEARCH/REPLACE block.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchReplaceBlock {
    pub search_content: String,
    pub replace_content: String,
    pub start_line: Option<usize>,
    pub context_hint: Option<String>,
}

fn unescape_markers(line: &str) -> String {
    if let Some(rest) = line.strip_prefix("\\<<<<<<<") {
        format!("<<<<<<<{}", rest)
    } else if let Some(rest) = line.strip_prefix("\\=======") {
        format!("======={}", rest)
    } else if let Some(rest) = line.strip_prefix("\\>>>>>>>") {
        format!(">>>>>>>{}", rest)
    } else if let Some(rest) = line.strip_prefix("\\-------") {
        format!("-------{}", rest)
    } else if let Some(rest) = line.strip_prefix("\\:end_line:") {
        format!(":end_line:{}", rest)
    } else if let Some(rest) = line.strip_prefix("\\:start_line:") {
        format!(":start_line:{}", rest)
    } else {
        line.to_string()
    }
}

/// Extract hint lines from the beginning of search content. Supports
/// `# line: N` and `# context: text`.
fn extract_hints(lines: &[String]) -> (Option<usize>, Option<String>, usize) {
    let mut start_line = None;
    let mut context_hint = None;
    let mut lines_removed = 0usize;

    for line in lines {
        let trimmed = line.trim();
        if let Some(num) = trimmed
            .strip_prefix("# line:")
            .or_else(|| trimmed.strip_prefix("#line:"))
        {
            if let Ok(n) = num.trim().parse::<usize>() {
                start_line = Some(n);
                lines_removed += 1;
                continue;
            }
        }
        if let Some(ctx) = trimmed
            .strip_prefix("# context:")
            .or_else(|| trimmed.strip_prefix("#context:"))
        {
            context_hint = Some(ctx.trim().to_string());
            lines_removed += 1;
            continue;
        }
        break;
    }

    (start_line, context_hint, lines_removed)
}

/// Validate marker sequencing to detect malformed diffs.
pub fn validate_marker_sequencing(diff_content: &str) -> Result<(), String> {
    #[derive(PartialEq, Clone, Copy)]
    enum State {
        Start,
        AfterSearch,
        AfterSeparator,
    }

    let mut state = State::Start;
    for (line_number, raw) in diff_content.split('\n').enumerate() {
        let marker = raw.trim();
        match state {
            State::Start => {
                if marker == SEPARATOR_MARKER || marker == REPLACE_MARKER {
                    return Err(format!(
                        "ERROR: Invalid marker '{}' at line {}. Expected SEARCH marker first.",
                        marker,
                        line_number + 1
                    ));
                }
                if marker.starts_with(SEARCH_MARKER) {
                    state = State::AfterSearch;
                }
            }
            State::AfterSearch => {
                if marker == SEPARATOR_MARKER {
                    state = State::AfterSeparator;
                } else if marker.starts_with(SEARCH_MARKER) || marker == REPLACE_MARKER {
                    return Err(format!(
                        "ERROR: Invalid marker sequence at line {}.",
                        line_number + 1
                    ));
                }
            }
            State::AfterSeparator => {
                if marker == REPLACE_MARKER {
                    state = State::Start;
                } else if marker == SEPARATOR_MARKER || marker.starts_with(SEARCH_MARKER) {
                    return Err(format!(
                        "ERROR: Invalid marker sequence at line {}.",
                        line_number + 1
                    ));
                }
            }
        }
    }

    if state != State::Start {
        return Err("ERROR: Incomplete SEARCH/REPLACE block. Missing closing marker.".into());
    }
    Ok(())
}

/// Parse SEARCH/REPLACE blocks from diff content.
pub fn parse_search_replace_blocks(diff_content: &str) -> Result<Vec<SearchReplaceBlock>, String> {
    let lines: Vec<String> = diff_content.split('\n').map(String::from).collect();
    let mut blocks = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let marker = lines[i].trim();
        if !(marker == SEARCH_MARKER || marker.starts_with(SEARCH_MARKER)) {
            i += 1;
            continue;
        }

        // Collect search lines until the separator.
        let mut search_lines: Vec<String> = Vec::new();
        let mut j = i + 1;
        let mut found_separator = false;
        while j < lines.len() {
            let l = lines[j].trim();
            if l == SEPARATOR_MARKER {
                found_separator = true;
                break;
            }
            if l == REPLACE_MARKER || l == SEARCH_MARKER {
                return Err(format!(
                    "ERROR: Invalid marker sequence at line {}. Missing '=======' separator.",
                    j + 1
                ));
            }
            search_lines.push(unescape_markers(&lines[j]));
            j += 1;
        }
        if !found_separator {
            return Err("ERROR: Missing '=======' separator in SEARCH/REPLACE block.".into());
        }
        j += 1; // skip separator

        // Collect replace lines until the replace marker.
        let mut replace_lines: Vec<String> = Vec::new();
        let mut found_replace = false;
        while j < lines.len() {
            let l = lines[j].trim();
            if l == REPLACE_MARKER {
                found_replace = true;
                break;
            }
            if l == SEARCH_MARKER || l == SEPARATOR_MARKER {
                return Err(format!(
                    "ERROR: Invalid marker sequence at line {}. Missing '>>>>>>> REPLACE'.",
                    j + 1
                ));
            }
            replace_lines.push(unescape_markers(&lines[j]));
            j += 1;
        }
        if !found_replace {
            return Err("ERROR: Missing '>>>>>>> REPLACE' closing marker.".into());
        }

        // Legacy format: optional :start_line:N / :end_line:N / ------- header.
        let mut start_line = None;
        while let Some(first) = search_lines.first() {
            if let Some(num) = first.trim().strip_prefix(":start_line:") {
                if let Ok(n) = num.trim().parse::<usize>() {
                    start_line = Some(n);
                    search_lines.remove(0);
                    continue;
                }
            }
            if first.trim().starts_with(":end_line:") {
                search_lines.remove(0);
                continue;
            }
            if first.trim() == "-------" {
                search_lines.remove(0);
                continue;
            }
            break;
        }

        // Extract inline hints from the new format.
        let (hint_line, context_hint, lines_removed) = extract_hints(&search_lines);
        if hint_line.is_some() {
            start_line = hint_line;
        }
        if lines_removed > 0 {
            search_lines.drain(0..lines_removed);
        }

        blocks.push(SearchReplaceBlock {
            search_content: search_lines.join("\n"),
            replace_content: replace_lines.join("\n"),
            start_line,
            context_hint,
        });

        i = j + 1;
    }

    if blocks.is_empty() {
        return Err("Invalid diff format - missing required SEARCH/REPLACE sections".into());
    }
    Ok(blocks)
}

/// Find the first line index where the hint text appears (case-insensitive).
fn find_context_hint(lines: &[String], hint: &str) -> Option<usize> {
    let lower = hint.to_lowercase();
    lines.iter().position(|l| l.to_lowercase().contains(&lower))
}

/// Preserve indentation when applying replacements: calculates relative
/// indentation levels and adjusts accordingly.
fn preserve_indentation(
    matched_lines: &[String],
    search_lines: &[String],
    replace_lines: &[String],
) -> Vec<String> {
    let leading_ws =
        |s: &str| -> String { s.chars().take_while(|c| *c == ' ' || *c == '\t').collect() };

    let matched_indent = leading_ws(matched_lines.first().map(|s| s.as_str()).unwrap_or(""));
    let search_base_indent = leading_ws(search_lines.first().map(|s| s.as_str()).unwrap_or(""));
    let search_base_level = search_base_indent.len();

    replace_lines
        .iter()
        .map(|line| {
            let current_indent = leading_ws(line);
            let current_level = current_indent.len();
            let relative_level = current_level as isize - search_base_level as isize;

            let final_indent = if relative_level < 0 {
                let cut = matched_indent
                    .len()
                    .saturating_sub(relative_level.unsigned_abs());
                matched_indent[..cut].to_string()
            } else {
                format!("{}{}", matched_indent, &current_indent[search_base_level..])
            };

            format!("{}{}", final_indent, line.trim())
        })
        .collect()
}

/// Apply a single SEARCH/REPLACE block to the content lines. Returns the new
/// delta on success.
pub fn apply_block(
    result_lines: &[String],
    block: &SearchReplaceBlock,
    delta: isize,
) -> Result<(Vec<String>, isize), String> {
    let start_line = block.start_line.map(|l| l as isize + delta).unwrap_or(0);

    let search_lines: Vec<String> = if block.search_content.is_empty() {
        Vec::new()
    } else {
        block.search_content.split('\n').map(String::from).collect()
    };
    let replace_lines: Vec<String> = if block.replace_content.is_empty() {
        Vec::new()
    } else {
        block
            .replace_content
            .split('\n')
            .map(String::from)
            .collect()
    };

    if search_lines.is_empty() {
        return Err("Empty search content is not allowed".into());
    }

    let mut search_start_index = 0usize;
    if start_line > 0 {
        search_start_index = (start_line - 1) as usize;
    }

    // If a context hint is provided, search near the context first.
    let mut match_index: Option<usize> = None;
    if let Some(hint) = &block.context_hint {
        if let Some(context_index) = find_context_hint(result_lines, hint) {
            let context_start = context_index.saturating_sub(10);
            let context_end = (context_index + search_lines.len() + 10).min(result_lines.len());
            let context_lines = &result_lines[..context_end];
            match_index = seek_sequence(context_lines, &search_lines, context_start, false);
        }
    }

    // Fall back to a full search if the context hint search failed.
    if match_index.is_none() {
        match_index = seek_sequence(result_lines, &search_lines, search_start_index, false);
    }

    let Some(match_index) = match_index else {
        let line_range = if start_line > 0 {
            format!(" near line: {}", start_line)
        } else {
            String::new()
        };
        let context: Vec<String> = if start_line > 0 {
            let from = (start_line as usize).saturating_sub(3);
            let to = ((start_line as usize) + 4).min(result_lines.len());
            result_lines[from..to].to_vec()
        } else {
            result_lines[..result_lines.len().min(7)].to_vec()
        };
        let context_str = context
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let line_num = if start_line > 0 {
                    start_line - 3 + i as isize
                } else {
                    i as isize + 1
                };
                format!("{}: {}", line_num, l)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let context_label = block
            .context_hint
            .as_ref()
            .map(|h| format!(" near \"{}\"", h))
            .unwrap_or_default();
        return Err(format!(
            "No match found{}{}.\nNearby lines:\n{}",
            line_range, context_label, context_str
        ));
    };

    let matched_lines = &result_lines[match_index..match_index + search_lines.len()];
    let indented_replace = preserve_indentation(matched_lines, &search_lines, &replace_lines);

    let mut new_lines = result_lines[..match_index].to_vec();
    new_lines.extend(indented_replace);
    new_lines.extend_from_slice(&result_lines[match_index + search_lines.len()..]);

    let new_delta = delta - matched_lines.len() as isize + replace_lines.len() as isize;
    Ok((new_lines, new_delta))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── apply_patch parser ───────────────────────────────

    #[test]
    fn test_parse_add_file() {
        let patch = "*** Begin Patch\n*** Add File: new.txt\n+hello\n+world\n*** End Patch";
        let hunks = parse_patch(patch).unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].op, PatchOp::AddFile);
        assert_eq!(hunks[0].path, "new.txt");
        assert_eq!(hunks[0].contents, "hello\nworld\n");
    }

    #[test]
    fn test_parse_delete_file() {
        let patch = "*** Begin Patch\n*** Delete File: old.txt\n*** End Patch";
        let hunks = parse_patch(patch).unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].op, PatchOp::DeleteFile);
        assert_eq!(hunks[0].path, "old.txt");
    }

    #[test]
    fn test_parse_update_file() {
        let patch =
            "*** Begin Patch\n*** Update File: a.txt\n@@ context\n-old\n+new\n*** End Patch";
        let hunks = parse_patch(patch).unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].op, PatchOp::UpdateFile);
        assert_eq!(hunks[0].path, "a.txt");
        let chunk = &hunks[0].chunks[0];
        assert_eq!(chunk.change_context.as_deref(), Some("context"));
        assert_eq!(chunk.old_lines, vec!["old"]);
        assert_eq!(chunk.new_lines, vec!["new"]);
    }

    #[test]
    fn test_parse_update_file_move() {
        let patch = "*** Begin Patch\n*** Update File: a.txt\n*** Move to: b.txt\n@@\n-old\n+new\n*** End Patch";
        let hunks = parse_patch(patch).unwrap();
        assert_eq!(hunks[0].move_path.as_deref(), Some("b.txt"));
    }

    #[test]
    fn test_parse_multiple_hunks_and_missing_context() {
        let patch = "*** Begin Patch\n*** Add File: x\n+a\n*** Update File: y.txt\n+added-line\n*** Delete File: z\n*** End Patch";
        let hunks = parse_patch(patch).unwrap();
        assert_eq!(hunks.len(), 3);
        assert_eq!(hunks[0].op, PatchOp::AddFile);
        assert_eq!(hunks[1].op, PatchOp::UpdateFile);
        assert_eq!(hunks[2].op, PatchOp::DeleteFile);
        // First chunk of an update may omit the @@ context marker.
        assert_eq!(hunks[1].chunks[0].old_lines.len(), 0);
        assert_eq!(hunks[1].chunks[0].new_lines, vec!["added-line"]);
    }

    #[test]
    fn test_parse_end_of_file_marker() {
        let patch =
            "*** Begin Patch\n*** Update File: a.txt\n@@\n+tail\n*** End of File\n*** End Patch";
        let hunks = parse_patch(patch).unwrap();
        assert!(hunks[0].chunks[0].is_end_of_file);
    }

    #[test]
    fn test_parse_rejects_bad_boundaries() {
        assert!(parse_patch("*** Begin Patch\n*** End Patch\n").is_ok());
        assert!(parse_patch("foo\n*** End Patch").is_err());
        assert!(parse_patch("*** Begin Patch\nfoo").is_err());
    }

    #[test]
    fn test_parse_rejects_absolute_or_traversal_path() {
        let patch = "*** Begin Patch\n*** Add File: /etc/passwd\n+x\n*** End Patch";
        assert!(parse_patch(patch).is_err());
        let patch = "*** Begin Patch\n*** Add File: ../evil.txt\n+x\n*** End Patch";
        assert!(parse_patch(patch).is_err());
    }

    #[test]
    fn test_parse_heredoc_lenient() {
        let patch = "<<EOF\n*** Begin Patch\n*** Add File: x.txt\n+line\n*** End Patch\nEOF";
        let hunks = parse_patch(patch).unwrap();
        assert_eq!(hunks.len(), 1);
    }

    // ── apply_patch applier ──────────────────────────────

    #[test]
    fn test_apply_pure_insert_at_end() {
        let chunks = vec![UpdateFileChunk {
            change_context: None,
            old_lines: vec![],
            new_lines: vec!["tail".into()],
            is_end_of_file: false,
        }];
        let result = apply_chunks_to_content("a\nb\n", "f.txt", &chunks).unwrap();
        assert_eq!(result, "a\nb\ntail\n");
    }

    #[test]
    fn test_apply_update_replaces_old_lines() {
        let chunks = vec![UpdateFileChunk {
            change_context: None,
            old_lines: vec!["old".into()],
            new_lines: vec!["new".into()],
            is_end_of_file: false,
        }];
        let result = apply_chunks_to_content("before\nold\nafter\n", "f.txt", &chunks).unwrap();
        assert_eq!(result, "before\nnew\nafter\n");
    }

    #[test]
    fn test_apply_old_lines_not_found() {
        let chunks = vec![UpdateFileChunk {
            change_context: None,
            old_lines: vec!["missing".into()],
            new_lines: vec!["x".into()],
            is_end_of_file: false,
        }];
        assert!(apply_chunks_to_content("hello\n", "f.txt", &chunks).is_err());
    }

    #[test]
    fn test_apply_trim_end_match() {
        let chunks = vec![UpdateFileChunk {
            change_context: None,
            old_lines: vec!["foo".into()],
            new_lines: vec!["bar".into()],
            is_end_of_file: false,
        }];
        let result = apply_chunks_to_content("foo  \n", "f.txt", &chunks).unwrap();
        assert_eq!(result, "bar\n");
    }

    #[test]
    fn test_apply_trailing_newline_pattern_retry() {
        let chunks = vec![UpdateFileChunk {
            change_context: None,
            old_lines: vec!["line".into(), String::new()],
            new_lines: vec!["replaced".into(), String::new()],
            is_end_of_file: false,
        }];
        let result = apply_chunks_to_content("a\nline", "f.txt", &chunks).unwrap();
        assert_eq!(result, "a\nreplaced\n");
    }

    #[test]
    fn test_apply_context_anchors() {
        let chunks = vec![UpdateFileChunk {
            change_context: Some("anchor".into()),
            old_lines: vec!["hit".into()],
            new_lines: vec!["changed".into()],
            is_end_of_file: false,
        }];
        let result = apply_chunks_to_content("anchor\nhit\n", "f.txt", &chunks).unwrap();
        assert_eq!(result, "anchor\nchanged\n");
    }

    #[test]
    fn test_apply_multiple_chunks_sequentially() {
        let chunks = vec![
            UpdateFileChunk {
                change_context: None,
                old_lines: vec!["one".into()],
                new_lines: vec!["ONE".into()],
                is_end_of_file: false,
            },
            UpdateFileChunk {
                change_context: None,
                old_lines: vec!["two".into()],
                new_lines: vec!["TWO".into()],
                is_end_of_file: false,
            },
        ];
        let result = apply_chunks_to_content("one\ntwo\n", "f.txt", &chunks).unwrap();
        assert_eq!(result, "ONE\nTWO\n");
    }

    // ── apply_diff parser ────────────────────────────────

    #[test]
    fn test_parse_basic_search_replace() {
        let diff = "<<<<<<< SEARCH\nfoo\n=======\nbar\n>>>>>>> REPLACE";
        let blocks = parse_search_replace_blocks(diff).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].search_content, "foo");
        assert_eq!(blocks[0].replace_content, "bar");
    }

    #[test]
    fn test_parse_multiple_blocks() {
        let diff = "<<<<<<< SEARCH\na\n=======\nb\n>>>>>>> REPLACE\n<<<<<<< SEARCH\nc\n=======\nd\n>>>>>>> REPLACE";
        let blocks = parse_search_replace_blocks(diff).unwrap();
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn test_parse_legacy_start_line_and_separator() {
        let diff = "<<<<<<< SEARCH\n:start_line:10\n-------\nfoo\n=======\nbar\n>>>>>>> REPLACE";
        let blocks = parse_search_replace_blocks(diff).unwrap();
        assert_eq!(blocks[0].start_line, Some(10));
        assert_eq!(blocks[0].search_content, "foo");
    }

    #[test]
    fn test_parse_hints() {
        let diff =
            "<<<<<<< SEARCH\n# line: 5\n# context: fn main\nfoo\n=======\nbar\n>>>>>>> REPLACE";
        let blocks = parse_search_replace_blocks(diff).unwrap();
        assert_eq!(blocks[0].start_line, Some(5));
        assert_eq!(blocks[0].context_hint.as_deref(), Some("fn main"));
        assert_eq!(blocks[0].search_content, "foo");
    }

    #[test]
    fn test_parse_escaped_markers() {
        let diff = "<<<<<<< SEARCH\n\\=======\nliteral\n=======\n\\\\>>>>>>>\n>>>>>>> REPLACE";
        let blocks = parse_search_replace_blocks(diff).unwrap();
        assert!(blocks[0].search_content.contains("======="));
        assert!(blocks[0].replace_content.contains("\\>>>>>>>"));
    }

    #[test]
    fn test_parse_invalid_missing_separator() {
        let diff = "<<<<<<< SEARCH\nfoo\n>>>>>>> REPLACE";
        assert!(parse_search_replace_blocks(diff).is_err());
    }

    #[test]
    fn test_validate_marker_sequencing() {
        assert!(
            validate_marker_sequencing("<<<<<<< SEARCH\na\n=======\nb\n>>>>>>> REPLACE").is_ok()
        );
        assert!(validate_marker_sequencing("=======\nb\n>>>>>>> REPLACE").is_err());
        assert!(validate_marker_sequencing("<<<<<<< SEARCH\na\n=======\nb").is_err());
    }

    // ── apply_diff applier ───────────────────────────────

    #[test]
    fn test_apply_block_basic() {
        let lines: Vec<String> = vec!["a".into(), "foo".into(), "b".into()];
        let block = SearchReplaceBlock {
            search_content: "foo".into(),
            replace_content: "bar".into(),
            start_line: None,
            context_hint: None,
        };
        let (new_lines, delta) = apply_block(&lines, &block, 0).unwrap();
        assert_eq!(new_lines, vec!["a", "bar", "b"]);
        assert_eq!(delta, 0);
    }

    #[test]
    fn test_apply_block_no_match_error() {
        let lines: Vec<String> = vec!["a".into(), "b".into()];
        let block = SearchReplaceBlock {
            search_content: "zzz".into(),
            replace_content: "x".into(),
            start_line: None,
            context_hint: None,
        };
        assert!(apply_block(&lines, &block, 0).is_err());
    }

    #[test]
    fn test_apply_block_empty_search_error() {
        let lines: Vec<String> = vec!["a".into()];
        let block = SearchReplaceBlock {
            search_content: String::new(),
            replace_content: "x".into(),
            start_line: None,
            context_hint: None,
        };
        assert!(apply_block(&lines, &block, 0).is_err());
    }

    #[test]
    fn test_apply_block_indentation_preserved() {
        let lines: Vec<String> = vec![
            "    fn foo() {".into(),
            "        old".into(),
            "    }".into(),
        ];
        let block = SearchReplaceBlock {
            search_content: "    old".into(),
            replace_content: "    new1\n    new2".into(),
            start_line: None,
            context_hint: None,
        };
        let (new_lines, delta) = apply_block(&lines, &block, 0).unwrap();
        assert_eq!(
            new_lines,
            vec!["    fn foo() {", "        new1", "        new2", "    }"]
        );
        assert_eq!(delta, 1);
    }

    #[test]
    fn test_apply_block_context_hint() {
        let lines: Vec<String> = vec![
            "fn helper() {}".into(),
            "x".into(),
            "fn main() {}".into(),
            "target".into(),
        ];
        let block = SearchReplaceBlock {
            search_content: "target".into(),
            replace_content: "changed".into(),
            start_line: None,
            context_hint: Some("fn main".into()),
        };
        let (new_lines, _) = apply_block(&lines, &block, 0).unwrap();
        assert_eq!(new_lines[3], "changed");
    }

    #[test]
    fn test_apply_block_start_line_anchored() {
        let lines: Vec<String> = vec!["a".into(), "b".into(), "target".into()];
        let block = SearchReplaceBlock {
            search_content: "target".into(),
            replace_content: "changed".into(),
            start_line: Some(3),
            context_hint: None,
        };
        let (new_lines, _) = apply_block(&lines, &block, 0).unwrap();
        assert_eq!(new_lines[2], "changed");
    }
}
