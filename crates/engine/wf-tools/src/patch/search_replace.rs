//! SEARCH/REPLACE block format used by the apply_diff tool.
//!
//! Parses `<<<<<<< SEARCH` / `=======` / `>>>>>>> REPLACE` blocks and applies
//! them to file content lines with indentation preservation.

use crate::sequence_matcher::seek_sequence;

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
