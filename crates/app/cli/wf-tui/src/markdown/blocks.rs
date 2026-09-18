//! Markdown block-structure analysis.
//!
//! Pure, stateless string analysis that detects the boundaries and in-flight
//! state of top-level markdown blocks (fenced code, tables, reference
//! definitions, blank-line settlement). Used by [`MarkdownStream`] to split a
//! growing source into a settled (committed) region and an in-flight
//! (streaming) region. No rendering and no buffers live here.

use pulldown_cmark::{Event, Parser};

/// Locate the byte offset where the last top-level block begins.
///
/// Walks the offset iterator tracking tag depth; the start offset of the
/// final depth-0 block is the committed/streaming boundary.
pub(super) fn last_top_level_block_start(src: &str) -> Option<usize> {
    let mut depth = 0u32;
    let mut last = None;
    for (event, range) in Parser::new(src).into_offset_iter() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    last = Some(range.start);
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    last
}

/// Turn a raw last-block split into the committed boundary by applying the
/// settlement and newline gates.
///
/// The in-flight block is committed in full when it has already settled (the
/// buffer ends on a blank line and no fence is open, or the tail is empty, or
/// the last line is a closing fence); otherwise the boundary stays at the
/// block start. In every case the commit point is capped at the last newline
/// so a half line is never committed.
pub(super) fn settle_boundary(src: &str, split: usize) -> usize {
    let streaming_text = &src[split..];
    let mut boundary = if streaming_text.is_empty()
        || last_line_is_fence(streaming_text)
        || (ends_with_blank_line(src) && !fence_open(streaming_text))
    {
        src.len()
    } else {
        split
    };
    let last_newline = src.rfind('\n').map(|i| i + 1).unwrap_or(0);
    if boundary > last_newline {
        boundary = last_newline;
    }
    boundary
}

/// True when `s` ends with a blank line (at least two consecutive newlines
/// at the end, tolerant of CRLF).
pub(crate) fn ends_with_blank_line(s: &str) -> bool {
    let trimmed = s.trim_end_matches(['\r', '\n']);
    s[trimmed.len()..].chars().filter(|c| *c == '\n').count() >= 2
}

/// True when `s` has an unclosed code fence (an odd number of fence lines).
pub(super) fn fence_open(s: &str) -> bool {
    let mut open = false;
    for line in s.lines() {
        let t = line.trim();
        let mut chars = t.chars();
        let Some(first) = chars.next() else { continue };
        if first != '`' && first != '~' {
            continue;
        }
        let len = t.chars().take_while(|c| *c == first).count();
        if len < 3 {
            continue;
        }
        open = !open;
    }
    open
}

/// True when the buffer carries a reference-style link definition
/// (`[label]: destination`). Such definitions rewrite earlier link targets,
/// so incremental splitting is skipped and the whole source stays streaming
/// until finalize.
pub(super) fn has_reference_definition(src: &str) -> bool {
    src.lines()
        .any(|line| line.trim_start().starts_with('[') && line.contains("]:"))
}

/// Table holdback: when the buffer tail holds an *unclosed* table (a header
/// row and a delimiter row are already present and table rows are still
/// continuing), return the byte offset of the table header start so the whole
/// table stays on the streaming side until finalize. Returns `None` when the
/// tail is not a table or the table already closed (a blank line follows the
/// last row). Callers must only invoke this outside fenced code blocks.
pub(super) fn unclosed_table_start(src: &str) -> Option<usize> {
    let lines: Vec<&str> = src.split('\n').collect();
    // Skip trailing blank lines; the last non-blank line must be a table row.
    let mut i = lines.len();
    while i > 0 && lines[i - 1].trim().is_empty() {
        i -= 1;
    }
    if i == 0 || !is_table_line(lines[i - 1]) {
        return None;
    }
    // Collect the contiguous run of table rows upward and locate the
    // delimiter row (`| --- |`). The header row precedes the delimiter.
    let mut start = i;
    let mut delimiter: Option<usize> = None;
    let mut j = i;
    while j > 0 && is_table_line(lines[j - 1]) {
        if is_table_delimiter(lines[j - 1]) {
            delimiter = Some(j - 1);
        }
        start = j - 1;
        j -= 1;
    }
    let delimiter = delimiter?;
    if delimiter == start {
        return None; // delimiter row is the first row: no header, no table
    }
    let mut offset = 0usize;
    for line in lines.iter().take(start) {
        offset += line.len() + 1; // +1 for the newline separator
    }
    Some(offset)
}

/// A table row: the trimmed line starts with `|` (GFM tables may also keep
/// a closing `|` after the cells).
fn is_table_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') || (t.ends_with('|') && t.contains('|'))
}

/// A table delimiter row: only `|`, `-`, `:` and whitespace, with at least
/// one `-` (e.g. `| --- | :---: |`).
fn is_table_delimiter(line: &str) -> bool {
    let t = line.trim();
    t.contains('-') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// True when the last line of `s` is a pure closing fence and an opening
/// fence exists earlier in the stream (i.e. the fenced code block settled).
fn last_line_is_fence(s: &str) -> bool {
    let Some((prefix, last)) = s.rsplit_once('\n') else {
        return false; // single line: an opening fence, not a closing one
    };
    let t = last.trim();
    let Some(first) = t.chars().next().filter(|c| *c == '`' || *c == '~') else {
        return false;
    };
    let pure = t.len() >= 3 && t.chars().all(|c| c == first);
    if !pure {
        return false;
    }
    prefix.lines().any(|l| {
        l.trim()
            .chars()
            .next()
            .is_some_and(|c| c == '`' || c == '~')
    })
}

/// Language tag of the in-flight fenced code block, if any.
pub(super) fn extract_code_lang(streaming: &str) -> Option<String> {
    for line in streaming.lines() {
        let t = line.trim();
        let mut chars = t.chars();
        let Some(first) = chars.next() else { continue };
        if first != '`' && first != '~' {
            continue;
        }
        let fence_len = t.chars().take_while(|c| *c == first).count();
        if fence_len < 3 {
            continue;
        }
        let rest: String = t.chars().skip(fence_len).collect();
        let rest = rest.trim();
        if !rest.is_empty() {
            return rest.split_whitespace().next().map(str::to_string);
        }
    }
    None
}
