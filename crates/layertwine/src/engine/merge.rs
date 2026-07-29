//! Merge Engine - Three Way Merge & Delta Applications
//!
//! Provide two core competencies:
//! 1. apply_deltas: rebuilds the complete file content from the baseline content + Delta chains
//! 2. merge_texts: three-way text merge (with conflict detection)

use std::borrow::Cow;

use crate::core::delta::Delta;
use crate::core::types::DiffOp;
use crate::core::types::LineDiff;
use crate::error::{LayertwineError, Result};

/// Apply Delta sequentially from the baseline content to rebuild the complete file content.
///
/// Each Delta is applied to the current content in turn according to the transformations defined by the internal Hunks.
/// Hunks are processed in order after sorting by old_start, and each Hunk is processed from the old contents of the
/// Locate the `old_start..old_start+old_len` area and execute:
/// - Equal: Retains the corresponding line
/// - Delete: skips the corresponding line
/// - Insert: Inserts a new line
/// - Replace: skips the old line and inserts a new one
pub fn apply_deltas(content: &str, deltas: &[Delta]) -> Result<String> {
    // Fast path: no deltas to apply, avoid split/join overhead entirely
    if deltas.is_empty() {
        return Ok(content.to_string());
    }

    let has_trailing_newline = content.ends_with('\n');
    // Use Cow<str> to avoid cloning unchanged lines through the delta chain.
    // Borrowed for lines that pass through unchanged, Owned for inserted/replaced lines.
    let mut lines: Vec<Cow<str>> = content.lines().map(Cow::Borrowed).collect();

    for delta in deltas {
        lines = apply_line_diff(&lines, &delta.diff)?;
    }

    let result = lines.join("\n");
    // .lines() + .join("\n") strips trailing newlines. Restore so downstream
    // consumers (TextDiff::from_lines, diff_to_line_diff) get correct tokens.
    if has_trailing_newline && !result.is_empty() {
        Ok(result + "\n")
    } else {
        Ok(result)
    }
}

/// Apply a single LineDiff to an array of rows.
///
/// Uses `Cow<str>` so unchanged lines can be borrowed from the input without allocation,
/// while inserted/replaced lines are owned strings. This significantly reduces memory
/// allocation overhead in multi-delta chains.
fn apply_line_diff<'a>(lines: &[Cow<'a, str>], diff: &LineDiff) -> Result<Vec<Cow<'a, str>>> {
    let mut result: Vec<Cow<'a, str>> = Vec::with_capacity(lines.len());
    let mut old_pos = 0usize;

    for hunk in &diff.hunks {
        let hunk_start = (hunk.old_start.saturating_sub(1)) as usize;
        let hunk_end = hunk_start + hunk.old_len as usize;

        // Ensure that hunk locations do not overlap and do not cross boundaries
        if hunk_start < old_pos {
            return Err(LayertwineError::Engine(format!(
                "Overlapping hunk: old_start={}, processed to position {}",
                hunk.old_start, old_pos
            )));
        }
        if hunk_end > lines.len() {
            return Err(LayertwineError::Engine(format!(
                "Hunk out of range: old_start={}, old_len={}, total rows={}",
                hunk.old_start,
                hunk.old_len,
                lines.len()
            )));
        }

        // Copy the unchanged part before the hunk (Cow::Borrowed — no allocation)
        if hunk_start > old_pos {
            result.extend_from_slice(&lines[old_pos..hunk_start]);
        }

        // Handling operations within a hunk
        let mut hunk_pos = hunk_start;
        for op in &hunk.ops {
            match op {
                DiffOp::Equal { count } => {
                    let c = *count as usize;
                    result.extend_from_slice(&lines[hunk_pos..hunk_pos + c]);
                    hunk_pos += c;
                }
                DiffOp::Delete { count, .. } => {
                    hunk_pos += *count as usize;
                }
                DiffOp::Insert {
                    lines: new_lines, ..
                } => {
                    result.extend(new_lines.iter().map(|s| Cow::Owned(s.clone())));
                }
                DiffOp::Replace {
                    old_count,
                    lines: new_lines,
                    ..
                } => {
                    hunk_pos += *old_count as usize;
                    result.extend(new_lines.iter().map(|s| Cow::Owned(s.clone())));
                }
            }
        }

        old_pos = hunk_end;
    }

    // Remaining unchanged rows (Cow::Borrowed — no allocation)
    if old_pos < lines.len() {
        result.extend_from_slice(&lines[old_pos..]);
    }

    Ok(result)
}

/// Merger conflicts
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeConflict {
    /// Starting line number in the final output (0-indexed)
    pub start_line: usize,
    /// Content of the baseline version
    pub base: Vec<String>,
    /// The contents of our version
    pub ours: Vec<String>,
    /// The contents of their version
    pub theirs: Vec<String>,
}

impl MergeConflict {
    /// Generate a Git-like format for conflict markup
    pub fn to_conflict_marker(&self) -> String {
        let mut buf = String::new();
        buf.push_str("<<<<<<< ours\n");
        for line in &self.ours {
            buf.push_str(line);
            buf.push('\n');
        }
        buf.push_str("=======\n");
        for line in &self.theirs {
            buf.push_str(line);
            buf.push('\n');
        }
        buf.push_str(">>>>>>> theirs\n");
        buf
    }
}

/// Three-way text merge
///
/// Calculate the difference between base→ours and base→theirs based on diff_to_line_diff.
/// The two differences are then applied synchronously. Conflicts arise when the same area is modified by two sides in different ways.
///
/// Returns the merged text and a list of conflicts (if any).
pub fn merge_texts(base: &str, ours: &str, theirs: &str) -> (String, Vec<MergeConflict>) {
    use crate::engine::diff::diff_to_line_diff;

    // Fast path: both sides identical → return either one
    if ours == theirs {
        let has_trailing_newline =
            base.ends_with('\n') || ours.ends_with('\n') || theirs.ends_with('\n');
        let result = if has_trailing_newline && !ours.is_empty() && !ours.ends_with('\n') {
            format!("{}\n", ours)
        } else {
            ours.to_string()
        };
        return (result, vec![]);
    }
    // Fast path: only ours changed → return ours
    if base == theirs {
        let has_trailing_newline =
            base.ends_with('\n') || ours.ends_with('\n') || theirs.ends_with('\n');
        let result = if has_trailing_newline && !ours.is_empty() && !ours.ends_with('\n') {
            format!("{}\n", ours)
        } else {
            ours.to_string()
        };
        return (result, vec![]);
    }
    // Fast path: only theirs changed → return theirs
    if base == ours {
        let has_trailing_newline =
            base.ends_with('\n') || ours.ends_with('\n') || theirs.ends_with('\n');
        let result = if has_trailing_newline && !theirs.is_empty() && !theirs.ends_with('\n') {
            format!("{}\n", theirs)
        } else {
            theirs.to_string()
        };
        return (result, vec![]);
    }

    let diff_ours = diff_to_line_diff(base, ours);
    let diff_theirs = diff_to_line_diff(base, theirs);

    let base_lines: Vec<&str> = base.lines().collect();

    // Collect the changes made by both sides on the base
    let mut our_changes: Vec<ChangeRange> = Vec::new();
    let mut their_changes: Vec<ChangeRange> = Vec::new();

    extract_changes_from_line_diff(&diff_ours, &mut our_changes);
    extract_changes_from_line_diff(&diff_theirs, &mut their_changes);

    // Merge changes on both sides (using jj-like line-by-line markup)
    // Pre-allocate capacity based on base line count to reduce reallocations
    let mut result: Vec<String> = Vec::with_capacity(base_lines.len());
    let mut conflicts: Vec<MergeConflict> = Vec::new();
    let mut base_pos = 0usize;

    let mut our_idx = 0usize;
    let mut their_idx = 0usize;

    while our_idx < our_changes.len() || their_idx < their_changes.len() {
        // Takes an earlier change from the current position
        let our_change = our_changes.get(our_idx);
        let their_change = their_changes.get(their_idx);

        match (our_change, their_change) {
            (None, Some(tc)) => {
                // They're the only ones who changed.
                append_unchanged(&mut result, &base_lines, base_pos, tc.base_start);
                append_change(&mut result, tc);
                base_pos = tc.base_start + tc.base_len;
                their_idx += 1;
            }
            (Some(oc), None) => {
                // We're the only ones who changed.
                append_unchanged(&mut result, &base_lines, base_pos, oc.base_start);
                append_change(&mut result, oc);
                base_pos = oc.base_start + oc.base_len;
                our_idx += 1;
            }
            (Some(oc), Some(tc)) => {
                if oc.base_start < tc.base_start {
                    // Our change was earlier.
                    append_unchanged(&mut result, &base_lines, base_pos, oc.base_start);
                    append_change(&mut result, oc);
                    base_pos = oc.base_start + oc.base_len;
                    our_idx += 1;
                } else if tc.base_start < oc.base_start {
                    // They changed earlier.
                    append_unchanged(&mut result, &base_lines, base_pos, tc.base_start);
                    append_change(&mut result, tc);
                    base_pos = tc.base_start + tc.base_len;
                    their_idx += 1;
                } else {
                    // Both sides are modified from the same position - check for conflicts
                    let oc_end = oc.base_start + oc.base_len;
                    let tc_end = tc.base_start + tc.base_len;

                    if oc.base_len == tc.base_len && oc.new_lines == tc.new_lines {
                        // Same modifications made on both sides - applied only once
                        append_unchanged(&mut result, &base_lines, base_pos, oc.base_start);
                        append_change(&mut result, oc);
                        base_pos = oc_end;
                    } else if oc.base_len == 0 && tc.base_len == 0 {
                        // Both are pure insertions at the same position.
                    // If one side is a prefix of the other, take the longer one (no conflict).
                    if tc.new_lines.starts_with(&oc.new_lines) {
                        // Theirs includes all of ours → accept theirs
                        append_unchanged(&mut result, &base_lines, base_pos, oc.base_start);
                        append_change(&mut result, tc);
                        base_pos = oc.base_start;
                    } else if oc.new_lines.starts_with(&tc.new_lines) {
                        // Ours includes all of theirs → accept ours
                        append_unchanged(&mut result, &base_lines, base_pos, oc.base_start);
                        append_change(&mut result, oc);
                        base_pos = oc.base_start;
                        } else {
                            // Different insertions at same position = conflict
                            append_unchanged(&mut result, &base_lines, base_pos, oc.base_start);

                            let conflict_start_line = result.len();
                            for line in &oc.new_lines {
                                result.push(line.clone());
                            }
                            base_pos = oc_end.max(tc_end);

                            conflicts.push(MergeConflict {
                                start_line: conflict_start_line,
                                base: base_lines[oc.base_start..oc.base_start + oc.base_len]
                                    .iter()
                                    .map(|s| s.to_string())
                                    .collect(),
                                ours: oc.new_lines.clone(),
                                theirs: tc.new_lines.clone(),
                            });
                        }
                    } else {
                        // Overlapping or same scope but different content = conflict
                        append_unchanged(&mut result, &base_lines, base_pos, oc.base_start);

                        let conflict_start_line = result.len();
                        // Output ours
                        for line in &oc.new_lines {
                            result.push(line.clone());
                        }
                        base_pos = oc_end.max(tc_end);

                        conflicts.push(MergeConflict {
                            start_line: conflict_start_line,
                            base: base_lines[oc.base_start..oc.base_start + oc.base_len]
                                .iter()
                                .map(|s| s.to_string())
                                .collect(),
                            ours: oc.new_lines.clone(),
                            theirs: tc.new_lines.clone(),
                        });
                    }
                    our_idx += 1;
                    their_idx += 1;
                }
            }
            (None, None) => break,
        }
    }

    // Remaining unchanged rows
    if base_pos < base_lines.len() {
        for line in &base_lines[base_pos..] {
            result.push(line.to_string());
        }
    }

    let result_str = result.join("\n");
    // Preserve trailing newline (consistent with apply_deltas behavior)
    // so downstream consumers get accurate line-terminated text.
    let has_trailing_newline = base.ends_with('\n') || ours.ends_with('\n') || theirs.ends_with('\n');
    if has_trailing_newline && !result_str.is_empty() {
        (result_str + "\n", conflicts)
    } else {
        (result_str, conflicts)
    }
}

/// Scope of the change: a paragraph in the tag base is replaced with the new content.
#[derive(Debug, Clone)]
struct ChangeRange {
    base_start: usize,
    base_len: usize,
    new_lines: Vec<String>,
}

/// Flush accumulated changes into the changes vector and reset accumulators.
fn flush_change(
    changes: &mut Vec<ChangeRange>,
    current_base_start: &mut Option<usize>,
    current_base_len: &mut usize,
    current_new_lines: &mut Vec<String>,
) {
    if let Some(start) = *current_base_start {
        changes.push(ChangeRange {
            base_start: start,
            base_len: *current_base_len,
            new_lines: std::mem::take(current_new_lines),
        });
    }
    *current_base_start = None;
    *current_base_len = 0;
}

/// Extracting changes from LineDiff into ChangeRange list
///
/// Skips the Equal context and extracts only the actual change area (Delete/Insert/Replace).
/// Each hunk may generate multiple ChangeRanges (Equal segregated multiple change segments).
fn extract_changes_from_line_diff(
    diff: &LineDiff,
    changes: &mut Vec<ChangeRange>,
) {
    for hunk in &diff.hunks {
        let has_change = hunk
            .ops
            .iter()
            .any(|op| !matches!(op, DiffOp::Equal { .. }));
        if !has_change {
            continue;
        }

        let base_offset = (hunk.old_start.saturating_sub(1)) as usize;
        let mut old_cursor = 0usize;

        let mut current_base_start: Option<usize> = None;
        let mut current_base_len: usize = 0;
        let mut current_new_lines: Vec<String> = Vec::new();

        for op in &hunk.ops {
            match op {
                DiffOp::Equal { count } => {
                    let c = *count as usize;
                    flush_change(
                        changes,
                        &mut current_base_start,
                        &mut current_base_len,
                        &mut current_new_lines,
                    );
                    old_cursor += c;
                }
                DiffOp::Insert { lines, .. } => {
                    if current_base_start.is_none() {
                        current_base_start = Some(base_offset + old_cursor);
                    }
                    current_new_lines.extend(lines.iter().cloned());
                }
                DiffOp::Delete { count, .. } => {
                    if current_base_start.is_none() {
                        current_base_start = Some(base_offset + old_cursor);
                    }
                    let c = *count as usize;
                    current_base_len += c;
                    old_cursor += c;
                }
                DiffOp::Replace {
                    old_count, lines, ..
                } => {
                    if current_base_start.is_none() {
                        current_base_start = Some(base_offset + old_cursor);
                    }
                    let oc = *old_count as usize;
                    current_base_len += oc;
                    current_new_lines.extend(lines.iter().cloned());
                    old_cursor += oc;
                }
            }
        }

        flush_change(
            changes,
            &mut current_base_start,
            &mut current_base_len,
            &mut current_new_lines,
        );
    }
}
fn append_unchanged(result: &mut Vec<String>, base_lines: &[&str], from: usize, to: usize) {
    if to > from {
        for line in &base_lines[from..to.min(base_lines.len())] {
            result.push((*line).to_string());
        }
    }
}

/// Addition of a change
fn append_change(result: &mut Vec<String>, change: &ChangeRange) {
    for line in &change.new_lines {
        result.push(line.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::file_node::FileNode;
    use crate::core::types::LineDiff;
    use crate::core::types::{DiffOp, Hunk, SourceType};
    use std::path::PathBuf;

    #[test]
    fn test_apply_empty_deltas() {
        let content = "hello\nworld\n";
        let result = apply_deltas(content, &[]).unwrap();
        assert_eq!(result, "hello\nworld\n");
    }

    #[test]
    fn test_apply_single_insert() {
        let content = "line1\nline3\n";
        let hunk = Hunk {
            old_start: 2,
            old_len: 1,
            new_start: 2,
            new_len: 2,
            ops: vec![
                DiffOp::Insert {
                    new_start: 2,
                    lines: vec!["line2".to_string()],
                },
                DiffOp::Equal { count: 1 }, // keep "line3"
            ],
        };
        let diff = LineDiff::new(vec![hunk]);
        let delta = Delta::new(
            FileNode::new(PathBuf::from("test.txt"), b""),
            diff,
            SourceType::Manual,
        );
        let result = apply_deltas(content, &[delta]).unwrap();
        assert_eq!(result, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_apply_delete() {
        let content = "line1\nline2\nline3\n";
        let hunk = Hunk {
            old_start: 2,
            old_len: 1,
            new_start: 2,
            new_len: 0,
            ops: vec![DiffOp::Delete {
                old_start: 2,
                count: 1,
            }],
        };
        let diff = LineDiff::new(vec![hunk]);
        let delta = Delta::new(
            FileNode::new(PathBuf::from("test.txt"), b""),
            diff,
            SourceType::Manual,
        );
        let result = apply_deltas(content, &[delta]).unwrap();
        assert_eq!(result, "line1\nline3\n");
    }

    #[test]
    fn test_apply_replace() {
        let content = "aaa\nbbb\nccc\n";
        let hunk = Hunk {
            old_start: 2,
            old_len: 1,
            new_start: 2,
            new_len: 1,
            ops: vec![DiffOp::Replace {
                old_start: 2,
                old_count: 1,
                new_start: 2,
                lines: vec!["xxx".to_string()],
            }],
        };
        let diff = LineDiff::new(vec![hunk]);
        let delta = Delta::new(
            FileNode::new(PathBuf::from("test.txt"), b""),
            diff,
            SourceType::Manual,
        );
        let result = apply_deltas(content, &[delta]).unwrap();
        assert_eq!(result, "aaa\nxxx\nccc\n");
    }

    #[test]
    fn test_apply_chain() {
        let content = "a\nb\nc\n";
        let hunk1 = Hunk {
            old_start: 2,
            old_len: 1,
            new_start: 2,
            new_len: 2,
            ops: vec![
                DiffOp::Insert {
                    new_start: 2,
                    lines: vec!["x".to_string()],
                },
                DiffOp::Equal { count: 1 }, // keep "b"
            ],
        };
        // Note: After insertion the content becomes a\nx\nb\nc\n
        // Revise line 3 again (former b)
        let hunk2 = Hunk {
            old_start: 3,
            old_len: 1,
            new_start: 3,
            new_len: 1,
            ops: vec![DiffOp::Replace {
                old_start: 3,
                old_count: 1,
                new_start: 3,
                lines: vec!["y".to_string()],
            }],
        };
        let diff1 = LineDiff::new(vec![hunk1]);
        let diff2 = LineDiff::new(vec![hunk2]);
        let delta1 = Delta::new(
            FileNode::new(PathBuf::from("test.txt"), b""),
            diff1,
            SourceType::Manual,
        );
        let delta2 = Delta::new(
            FileNode::new(PathBuf::from("test.txt"), b""),
            diff2,
            SourceType::Manual,
        );
        let result = apply_deltas(content, &[delta1, delta2]).unwrap();
        assert_eq!(result, "a\nx\ny\nc\n");
    }

    #[test]
    fn test_apply_empty_content() {
        let content = "";
        let result = apply_deltas(content, &[]).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_apply_overlapping_hunks_error() {
        let content = "a\nb\nc\n";
        let hunk1 = Hunk {
            old_start: 1,
            old_len: 1,
            new_start: 1,
            new_len: 1,
            ops: vec![DiffOp::Equal { count: 1 }],
        };
        let hunk_overlap = Hunk {
            old_start: 1,
            old_len: 1,
            new_start: 1,
            new_len: 1,
            ops: vec![DiffOp::Equal { count: 1 }],
        };
        let diff = LineDiff::new(vec![hunk1, hunk_overlap]);
        let result = apply_line_diff(
            &content.lines().map(Cow::Borrowed).collect::<Vec<_>>(),
            &diff,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_out_of_range_hunks_error() {
        let content = "a\nb\n";
        let hunk = Hunk {
            old_start: 10,
            old_len: 1,
            new_start: 10,
            new_len: 1,
            ops: vec![DiffOp::Equal { count: 1 }],
        };
        let diff = LineDiff::new(vec![hunk]);
        let delta = Delta::new(
            FileNode::new(PathBuf::from("test.txt"), b""),
            diff,
            SourceType::Manual,
        );
        let result = apply_deltas(content, &[delta]);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_identical() {
        let base = "a\nb\nc\n";
        let (merged, conflicts) = merge_texts(base, base, base);
        assert_eq!(merged, "a\nb\nc\n");
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_merge_non_conflicting() {
        let base = "a\nb\nc\n";
        let ours = "x\nb\nc\n";
        let theirs = "a\nb\ny\n";
        let (merged, conflicts) = merge_texts(base, ours, theirs);
        assert!(conflicts.is_empty());
        assert!(merged.contains('x'));
        assert!(merged.contains('y'));
    }

    #[test]
    fn test_merge_with_conflict() {
        let base = "a\nb\nc\n";
        let ours = "a\nX\nc\n";
        let theirs = "a\nY\nc\n";
        let (_, conflicts) = merge_texts(base, ours, theirs);
        assert!(!conflicts.is_empty());
        assert_eq!(conflicts[0].ours, vec!["X"]);
        assert_eq!(conflicts[0].theirs, vec!["Y"]);
    }

    #[test]
    fn test_merge_same_change() {
        let base = "a\nb\nc\n";
        let ours = "a\nX\nc\n";
        let theirs = "a\nX\nc\n";
        let (merged, conflicts) = merge_texts(base, ours, theirs);
        assert!(conflicts.is_empty());
        assert_eq!(merged, "a\nX\nc\n");
    }

    #[test]
    fn test_merge_multiline_conflict() {
        let base = "a\nb\nc\nd\n";
        let ours = "a\nX\nY\nc\nd\n";
        let theirs = "a\nP\nQ\nc\nd\n";
        let (merged, conflicts) = merge_texts(base, ours, theirs);
        assert!(!conflicts.is_empty(), "should detect conflict");
        assert_eq!(conflicts[0].ours, vec!["X", "Y"]);
        assert_eq!(conflicts[0].theirs, vec!["P", "Q"]);
        assert!(merged.contains("a\n"));
    }

    #[test]
    fn test_merge_multiple_conflicts() {
        let base = "a\nb\nc\nd\ne\nf\n";
        let ours = "a\nX\nc\nd\nY\nf\n";
        let theirs = "a\nP\nc\nd\nQ\nf\n";
        let (_, conflicts) = merge_texts(base, ours, theirs);
        assert_eq!(
            conflicts.len(),
            2,
            "two separate conflicts should be detected"
        );
    }

    #[test]
    fn test_merge_empty_base() {
        let base = "";
        let ours = "a\nb\n";
        let theirs = "";
        let (merged, conflicts) = merge_texts(base, ours, theirs);
        assert!(conflicts.is_empty());
        assert_eq!(merged, "a\nb\n");
    }

    #[test]
    fn test_merge_one_side_no_changes() {
        let base = "a\nb\nc\n";
        let ours = "a\nb\nc\n";
        let theirs = "a\nX\nc\n";
        let (merged, conflicts) = merge_texts(base, ours, theirs);
        assert!(conflicts.is_empty());
        assert_eq!(merged, "a\nX\nc\n");
    }

    #[test]
    fn test_merge_insert_vs_delete() {
        let base = "a\nb\nc\n";
        let ours = "a\nc\n";
        let theirs = "a\nX\nb\nc\n";
        let (_merged, conflicts) = merge_texts(base, ours, theirs);
        // Insert vs delete on overlapping area should produce a conflict
        assert!(
            !conflicts.is_empty(),
            "insert vs delete on same line should conflict"
        );
    }

    #[test]
    fn test_conflict_marker_format() {
        let conflict = MergeConflict {
            start_line: 1,
            base: vec!["b".to_string()],
            ours: vec!["X".to_string()],
            theirs: vec!["Y".to_string()],
        };
        let markers = conflict.to_conflict_marker();
        assert!(markers.contains("<<<<<<< ours"));
        assert!(markers.contains("======="));
        assert!(markers.contains(">>>>>>> theirs"));
        assert!(markers.contains("X"));
        assert!(markers.contains("Y"));
    }
}
