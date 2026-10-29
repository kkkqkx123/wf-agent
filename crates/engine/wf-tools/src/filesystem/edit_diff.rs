//! Shared diff computation for single-file edit tools.
//!
//! The diff algorithm itself lives in the checkpoint subsystem; this module
//! only shapes its output for tool results: unified text, line statistics
//! and the first changed line used for editor navigation.

/// Context lines kept around each change hunk in generated diffs.
pub const DIFF_CONTEXT_LINES: usize = 3;

/// UTF-8 byte-order mark. Models never include it in edit text, so it is
/// stripped before matching and restored on write.
pub const UTF8_BOM: char = '\u{FEFF}';

/// Strip a leading BOM for matching. Returns the stripped slice.
pub fn strip_bom(content: &str) -> &str {
    content.strip_prefix(UTF8_BOM).unwrap_or(content)
}

/// Detect the dominant line ending of file content. Defaults to LF when
/// the content has no newlines or no CRLF pair precedes the first LF.
pub fn detect_line_ending(content: &str) -> &'static str {
    let crlf = content.find("\r\n");
    let lf = content.find('\n');
    match (crlf, lf) {
        (_, None) => "\n",
        (None, Some(_)) => "\n",
        (Some(crlf_pos), Some(lf_pos)) if crlf_pos < lf_pos => "\r\n",
        _ => "\n",
    }
}

/// Normalize all line endings to LF for matching.
pub fn normalize_to_lf(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

/// Restore the original line ending style before writing.
pub fn restore_line_endings(text: &str, ending: &str) -> String {
    if ending == "\r\n" {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    }
}

/// Presentation diff for one file edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditDiff {
    /// Unified diff text, empty when there is no change or the content is binary.
    pub diff: String,
    /// Added line count.
    pub additions: usize,
    /// Removed line count.
    pub deletions: usize,
    /// First changed line in the new content, for editor navigation.
    pub first_changed_line: Option<u32>,
    /// True when either side looks binary and no text diff was generated.
    pub binary: bool,
}

/// Parse the hunk header new-file start line (`@@ -a,b +c,d @@`).
/// Returns `None` when the header carries no new-file range.
fn hunk_new_start(header: &str) -> Option<u32> {
    for token in header.split_whitespace() {
        let Some(start) = token.strip_prefix('+') else {
            continue;
        };
        let number: String = start.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(value) = number.parse::<u32>() {
            return Some(value);
        }
    }
    None
}

/// Find the first changed line in the new file by walking unified diff
/// hunks. Context lines advance the cursor; the first added line records
/// the position. Returns `None` when the diff has no hunks.
pub fn first_changed_line(diff_text: &str) -> Option<u32> {
    let mut new_line = 0u32;
    let mut in_hunk = false;
    for line in diff_text.lines() {
        if let Some(header) = line.strip_prefix("@@ ") {
            match hunk_new_start(header) {
                Some(start) => {
                    new_line = start;
                    in_hunk = true;
                }
                None => in_hunk = false,
            }
            continue;
        }
        if !in_hunk {
            continue;
        }
        if line.starts_with('+') || line.starts_with('-') {
            return Some(new_line.max(1));
        }
        if line.starts_with(' ') {
            new_line += 1;
        }
    }
    None
}

/// Count line-level fuzzy matches of old lines inside content lines.
/// Matching strength relaxes from exact to trailing-whitespace, full
/// trim and Unicode normalization, mirroring patch application.
pub fn count_line_matches(content_lines: &[String], old_lines: &[String]) -> usize {
    if old_lines.is_empty() {
        return 0;
    }
    let mut count = 0usize;
    let mut start = 0usize;
    while start + old_lines.len() <= content_lines.len() {
        match crate::sequence_matcher::seek_sequence(content_lines, old_lines, start, false) {
            Some(index) => {
                count += 1;
                start = index + 1;
            }
            None => break,
        }
    }
    count
}

/// Reason a replacement anchor cannot be resolved to a single span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanError {
    /// The old text is empty.
    Empty,
    /// The old text matches more than once; carries the match count.
    Duplicate(usize),
    /// The old text matches nothing.
    NotFound,
}

/// Locate the unique line-level fuzzy match of old lines inside content
/// lines. Returns the starting line index, or the reason the anchor is
/// unusable.
pub fn locate_unique_match(
    content_lines: &[String],
    old_lines: &[String],
) -> Result<usize, SpanError> {
    let count = count_line_matches(content_lines, old_lines);
    if count == 0 {
        return Err(SpanError::NotFound);
    }
    if count > 1 {
        return Err(SpanError::Duplicate(count));
    }
    crate::sequence_matcher::seek_sequence(content_lines, old_lines, 0, false)
        .ok_or(SpanError::NotFound)
}

/// Byte span of one replacement inside file content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditSpan {
    /// Byte offset where the replacement starts.
    pub start: usize,
    /// Byte length of the text being replaced.
    pub len: usize,
}

/// Convert a matched line range to a byte span of the content. The end
/// offset excludes the trailing newline so splicing stays line-exact.
fn line_range_to_span(content: &str, start_line: usize, line_count: usize) -> EditSpan {
    let mut line_starts = vec![0usize];
    for (index, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            line_starts.push(index + 1);
        }
    }
    let start = line_starts
        .get(start_line)
        .copied()
        .unwrap_or(content.len());
    let end = line_starts
        .get(start_line + line_count)
        .map(|next| next.saturating_sub(1))
        .unwrap_or(content.len());
    EditSpan {
        start,
        len: end.saturating_sub(start),
    }
}

/// Resolve one old text to its byte span inside the content, trying exact
/// matching first and line-level fuzzy matching second. All matching
/// happens against the same original content.
pub fn resolve_edit_span(content: &str, old_text: &str) -> Result<EditSpan, SpanError> {
    if old_text.is_empty() {
        return Err(SpanError::Empty);
    }
    let count = content.matches(old_text).count();
    if count > 1 {
        return Err(SpanError::Duplicate(count));
    }
    if count == 1 {
        return match content.find(old_text) {
            Some(start) => Ok(EditSpan {
                start,
                len: old_text.len(),
            }),
            None => Err(SpanError::NotFound),
        };
    }
    let content_lines: Vec<String> = content.split('\n').map(str::to_string).collect();
    let old_lines: Vec<String> = old_text.split('\n').map(str::to_string).collect();
    let start_line = locate_unique_match(&content_lines, &old_lines)?;
    Ok(line_range_to_span(content, start_line, old_lines.len()))
}

/// One normalized replacement pending application. Shared by execution
/// and read-only preview so both observe identical matching semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditPair {
    /// Normalized exact text to search for; must resolve to one span.
    pub old_text: String,
    /// Normalized replacement text.
    pub new_text: String,
}

/// Reason a batch of replacements cannot be applied atomically. The
/// index identifies the offending entry for caller-side messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditApplyError {
    /// The indexed old text is empty.
    Empty { index: usize },
    /// The indexed old text matches more than once; carries the count.
    Duplicate { index: usize, count: usize },
    /// The indexed old text matches nothing.
    NotFound { index: usize },
    /// Two entries overlap; carries both entry indices.
    Overlap { first: usize, second: usize },
}

/// Resolve every replacement against the same original content, reject
/// overlaps, then apply in reverse order so offsets stay stable.
pub fn apply_edits_to_content(content: &str, edits: &[EditPair]) -> Result<String, EditApplyError> {
    let mut spans: Vec<(usize, EditSpan)> = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        match resolve_edit_span(content, &edit.old_text) {
            Ok(span) => spans.push((index, span)),
            Err(SpanError::Empty) => return Err(EditApplyError::Empty { index }),
            Err(SpanError::Duplicate(count)) => {
                return Err(EditApplyError::Duplicate { index, count })
            }
            Err(SpanError::NotFound) => return Err(EditApplyError::NotFound { index }),
        }
    }
    spans.sort_by_key(|(_, span)| span.start);
    for window in spans.windows(2) {
        let (first_index, first) = window[0];
        let (second_index, second) = window[1];
        if second.start < first.start + first.len {
            return Err(EditApplyError::Overlap {
                first: first_index,
                second: second_index,
            });
        }
    }
    let mut result = content.to_string();
    for (index, span) in spans.iter().rev() {
        result.replace_range(span.start..span.start + span.len, &edits[*index].new_text);
    }
    Ok(result)
}

/// Read-only application of edits: returns the would-be content plus
/// both diff views without touching the disk. Execution previews and
/// approval flows use this so the preview cannot drift from the write.
pub struct EditPreview {
    /// Content that would be written if the edits were applied.
    pub new_content: String,
    /// Unified diff with line statistics for checkpoint-style display.
    pub diff: EditDiff,
    /// Numbered human-readable diff for inline review.
    pub display: DisplayDiff,
}

/// Apply edits without writing and report both diff views.
pub fn preview_edits(before: &str, edits: &[EditPair]) -> Result<EditPreview, EditApplyError> {
    let new_content = apply_edits_to_content(before, edits)?;
    let diff = compute_edit_diff(before, &new_content);
    let display = generate_display_diff(before, &new_content, DISPLAY_CONTEXT_LINES);
    Ok(EditPreview {
        new_content,
        diff,
        display,
    })
}

/// Context lines kept around each change in numbered display diffs.
pub const DISPLAY_CONTEXT_LINES: usize = 4;

/// Numbered human-readable diff with line numbers and collapsed context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayDiff {
    /// Diff text in `+line content` / `-line content` / ` line content`
    /// form with ` ...` separators for collapsed regions.
    pub text: String,
    /// First changed line in the new content, for editor navigation.
    pub first_changed_line: Option<u32>,
}

/// Expand tabs for consistent rendering of display diff rows.
pub fn expand_diff_tabs(text: &str) -> String {
    text.replace('\t', "   ")
}

/// Generate a numbered display diff between two text contents. Context
/// lines around changes are kept, distant regions collapse to ` ...`.
/// Identical inputs produce an empty diff and no first changed line.
pub fn generate_display_diff(before: &str, after: &str, context_lines: usize) -> DisplayDiff {
    use similar::{ChangeTag, TextDiff};
    if before == after {
        return DisplayDiff {
            text: String::new(),
            first_changed_line: None,
        };
    }
    let diff = TextDiff::from_lines(before, after);
    let groups = diff.grouped_ops(context_lines);
    if groups.is_empty() {
        return DisplayDiff {
            text: String::new(),
            first_changed_line: None,
        };
    }
    let width = before
        .split('\n')
        .count()
        .max(after.split('\n').count())
        .to_string()
        .len();
    let mut output: Vec<String> = Vec::new();
    let mut old_line: u32 = 1;
    let mut new_line: u32 = 1;
    let mut first_changed_line: Option<u32> = None;
    let mark_changed = |line: u32, first: &mut Option<u32>| {
        if first.is_none() {
            *first = Some(line.max(1));
        }
    };
    for group in &groups {
        let group_old_start = group
            .first()
            .map(|op| op.old_range().start as u32)
            .unwrap_or(0);
        let group_new_start = group
            .first()
            .map(|op| op.new_range().start as u32)
            .unwrap_or(0);
        // Skipped equal lines between groups collapse to one marker;
        // cursors resync to absolute positions so numbering stays exact.
        if group_old_start + 1 > old_line {
            output.push(format!("{:>width$} ...", ""));
        }
        old_line = group_old_start + 1;
        new_line = group_new_start + 1;
        for op in group {
            for change in diff.iter_changes(op) {
                let content = change.value().strip_suffix('\n').unwrap_or(change.value());
                let content = expand_diff_tabs(content);
                match change.tag() {
                    ChangeTag::Delete => {
                        mark_changed(new_line, &mut first_changed_line);
                        output.push(format!("-{:>width$} {}", old_line, content, width = width));
                        old_line += 1;
                    }
                    ChangeTag::Insert => {
                        mark_changed(new_line, &mut first_changed_line);
                        output.push(format!("+{:>width$} {}", new_line, content, width = width));
                        new_line += 1;
                    }
                    ChangeTag::Equal => {
                        output.push(format!(" {:>width$} {}", old_line, content, width = width));
                        old_line += 1;
                        new_line += 1;
                    }
                }
            }
        }
    }
    let old_total = before.split('\n').count() as u32;
    let new_total = after.split('\n').count() as u32;
    if old_line <= old_total || new_line <= new_total {
        output.push(format!("{:>width$} ...", ""));
    }
    DisplayDiff {
        text: output.join("\n"),
        first_changed_line,
    }
}

/// Compute the presentation diff between two text contents. Identical
/// inputs and binary inputs both produce an empty diff; binary inputs are
/// additionally flagged so callers can report the kind.
pub fn compute_edit_diff(before: &str, after: &str) -> EditDiff {
    if before == after {
        return EditDiff {
            diff: String::new(),
            additions: 0,
            deletions: 0,
            first_changed_line: None,
            binary: false,
        };
    }
    if wf_checkpoint::is_binary(before.as_bytes()) || wf_checkpoint::is_binary(after.as_bytes()) {
        return EditDiff {
            diff: String::new(),
            additions: 0,
            deletions: 0,
            first_changed_line: None,
            binary: true,
        };
    }
    let diff = wf_checkpoint::unified_diff_text(before, after, DIFF_CONTEXT_LINES, None, None);
    let stats = wf_checkpoint::diff_stats_for_text(before, after);
    let first = first_changed_line(&diff);
    EditDiff {
        diff,
        additions: stats.added_lines,
        deletions: stats.removed_lines,
        first_changed_line: first,
        binary: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_produces_empty_diff() {
        let result = compute_edit_diff("same\n", "same\n");
        assert!(result.diff.is_empty());
        assert_eq!(result.additions, 0);
        assert_eq!(result.deletions, 0);
        assert_eq!(result.first_changed_line, None);
        assert!(!result.binary);
    }

    #[test]
    fn single_line_change_reports_stats_and_first_line() {
        let before = "one\ntwo\nthree\n";
        let after = "one\nTWO\nthree\n";
        let result = compute_edit_diff(before, after);
        assert!(result.diff.contains("-two"));
        assert!(result.diff.contains("+TWO"));
        assert_eq!(result.additions, 1);
        assert_eq!(result.deletions, 1);
        assert_eq!(result.first_changed_line, Some(2));
        assert!(!result.binary);
    }

    #[test]
    fn bom_and_line_endings_round_trip() {
        assert_eq!(strip_bom("\u{FEFF}hello"), "hello");
        assert_eq!(strip_bom("hello"), "hello");
        assert_eq!(detect_line_ending("a\r\nb\r\n"), "\r\n");
        assert_eq!(detect_line_ending("a\nb\n"), "\n");
        assert_eq!(detect_line_ending("no newlines"), "\n");
        assert_eq!(normalize_to_lf("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(restore_line_endings("a\nb\n", "\r\n"), "a\r\nb\r\n");
        assert_eq!(restore_line_endings("a\nb\n", "\n"), "a\nb\n");
    }

    #[test]
    fn fuzzy_matching_tolerates_whitespace_and_unicode() {
        let content = vec!["foo  ".to_string(), "bar".to_string()];
        assert_eq!(locate_unique_match(&content, &["foo".to_string()]), Ok(0));
        let quoted = vec!["\u{201C}quoted\u{201D}".to_string()];
        assert_eq!(
            locate_unique_match(&quoted, &["\"quoted\"".to_string()]),
            Ok(0)
        );
        let dashed = vec!["a\u{2014}b".to_string()];
        assert_eq!(locate_unique_match(&dashed, &["a-b".to_string()]), Ok(0));
        let missing = vec!["foo".to_string()];
        assert!(locate_unique_match(&missing, &["zzz".to_string()]).is_err());
        let duplicate = vec!["pat".to_string(), "x".to_string(), "pat".to_string()];
        assert!(locate_unique_match(&duplicate, &["pat".to_string()]).is_err());
        assert_eq!(count_line_matches(&duplicate, &["pat".to_string()]), 2);
    }

    #[test]
    fn binary_content_is_flagged_without_diff() {
        let result = compute_edit_diff("ab\0cd", "ab\0ce");
        assert!(result.binary);
        assert!(result.diff.is_empty());
    }

    #[test]
    fn header_without_new_range_yields_no_line() {
        assert_eq!(first_changed_line("@@ -1,2 @@\n-x\n"), None);
        assert_eq!(first_changed_line("no hunks here"), None);
    }

    fn pair(old: &str, new: &str) -> EditPair {
        EditPair {
            old_text: old.to_string(),
            new_text: new.to_string(),
        }
    }

    #[test]
    fn shared_kernel_applies_disjoint_edits() {
        let content = "one\ntwo\nthree\n";
        let result = apply_edits_to_content(content, &[pair("one", "ONE"), pair("three", "THREE")])
            .expect("disjoint edits apply");
        assert_eq!(result, "ONE\ntwo\nTHREE\n");
    }

    #[test]
    fn shared_kernel_rejects_overlap_and_duplicates() {
        let content = "aaa\nbbb\n";
        assert!(matches!(
            apply_edits_to_content(content, &[pair("aaa\nbbb", "x"), pair("bbb", "y")]),
            Err(EditApplyError::Overlap { .. })
        ));
        assert!(matches!(
            apply_edits_to_content("a\na\n", &[pair("a", "b")]),
            Err(EditApplyError::Duplicate { count: 2, .. })
        ));
        assert!(matches!(
            apply_edits_to_content(content, &[pair("zzz", "y")]),
            Err(EditApplyError::NotFound { .. })
        ));
        assert!(matches!(
            apply_edits_to_content(content, &[pair("", "y")]),
            Err(EditApplyError::Empty { .. })
        ));
    }

    #[test]
    fn preview_matches_execution_without_writing() {
        let before = "one\ntwo\nthree\n";
        let preview = preview_edits(before, &[pair("two", "TWO")]).expect("preview applies");
        assert_eq!(preview.new_content, "one\nTWO\nthree\n");
        assert_eq!(
            preview.new_content,
            apply_edits_to_content(before, &[pair("two", "TWO")]).expect("same kernel")
        );
        assert!(preview.diff.diff.contains("+TWO"));
        assert!(preview.display.text.contains("+2 TWO"));
        assert_eq!(preview.display.first_changed_line, Some(2));
    }

    #[test]
    fn display_diff_collapses_distant_context() {
        let old: Vec<String> = (1..=30).map(|i| format!("line{i}")).collect();
        let mut new = old.clone();
        new[1] = "line2_edit".to_string();
        new[28] = "line29_edit".to_string();
        let display = generate_display_diff(&old.join("\n"), &new.join("\n"), 1);
        assert!(display.text.contains("..."));
        assert!(display.text.contains("+ 2 line2_edit"));
        assert!(display.text.contains("+29 line29_edit"));
        assert_eq!(display.first_changed_line, Some(2));
    }

    #[test]
    fn display_diff_is_empty_without_changes() {
        let display = generate_display_diff("same\n", "same\n", 4);
        assert!(display.text.is_empty());
        assert_eq!(display.first_changed_line, None);
    }

    #[test]
    fn display_diff_expands_tabs() {
        let display = generate_display_diff("a\t\n", "a\tb\n", 1);
        assert!(!display.text.contains('\t'));
        assert_eq!(expand_diff_tabs("a\tb"), "a   b");
    }
}
