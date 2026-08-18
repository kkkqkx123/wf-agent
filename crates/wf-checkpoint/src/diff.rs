use similar::{ChangeTag, TextDiff};

/// Kind of a single diff operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOpKind {
    Equal,
    Delete,
    Insert,
}

/// One line-level diff operation with 1-based line numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffOp {
    pub kind: DiffOpKind,
    pub value: String,
    /// Line number in the old content (equal/delete).
    pub old_line: Option<usize>,
    /// Line number in the new content (equal/insert).
    pub new_line: Option<usize>,
}

/// Diff result with metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffResult {
    pub ops: Vec<DiffOp>,
    pub equal_count: usize,
    pub delete_count: usize,
    pub insert_count: usize,
    pub has_changes: bool,
}

/// Kind of a unified diff hunk line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkLineKind {
    Context,
    Delete,
    Insert,
}

/// One line inside a unified diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkLine {
    pub kind: HunkLineKind,
    pub value: String,
}

/// A unified diff hunk with 1-based start positions and counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<HunkLine>,
}

/// Diff statistics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffStats {
    pub added_lines: usize,
    pub removed_lines: usize,
    pub changed_lines: usize,
    pub similarity: f64,
}

/// Text diff engine based on `similar::TextDiff` (the same engine used by
/// layertwine), replacing the earlier simplified line-by-line comparison with
/// a full Myers O(ND) implementation producing ops with line numbers, hunks
/// and unified diff output.
pub struct DiffEngine {
    pub context_lines: usize,
    pub trim_lines: bool,
    pub ignore_blank_lines: bool,
}

impl Default for DiffEngine {
    fn default() -> Self {
        Self {
            context_lines: 3,
            trim_lines: false,
            ignore_blank_lines: false,
        }
    }
}

impl DiffEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_context_lines(mut self, context_lines: usize) -> Self {
        self.context_lines = context_lines;
        self
    }

    /// Preprocess content into lines: trim, drop blanks, drop the trailing
    /// empty line, then re-join.
    fn preprocess(&self, content: &str) -> String {
        let mut lines: Vec<&str> = content.split('\n').collect();
        if self.trim_lines {
            lines = lines.into_iter().map(|l| l.trim()).collect();
        }
        if self.ignore_blank_lines {
            lines.retain(|l| !l.is_empty());
        }
        if lines.last() == Some(&"") {
            lines.pop();
        }
        lines.join("\n")
    }

    /// Compute the diff between two contents.
    pub fn diff(&self, old_content: &str, new_content: &str) -> DiffResult {
        let old = self.preprocess(old_content);
        let new = self.preprocess(new_content);
        if old == new {
            let equal_count = old.lines().count();
            return DiffResult {
                ops: old
                    .lines()
                    .enumerate()
                    .map(|(i, value)| DiffOp {
                        kind: DiffOpKind::Equal,
                        value: value.to_string(),
                        old_line: Some(i + 1),
                        new_line: Some(i + 1),
                    })
                    .collect(),
                equal_count,
                delete_count: 0,
                insert_count: 0,
                has_changes: false,
            };
        }

        let diff = TextDiff::from_lines(old.as_str(), new.as_str());
        let mut ops = Vec::new();
        let mut equal_count = 0usize;
        let mut delete_count = 0usize;
        let mut insert_count = 0usize;

        for change in diff.iter_all_changes() {
            let (kind, is_delete, is_insert) = match change.tag() {
                ChangeTag::Equal => (DiffOpKind::Equal, false, false),
                ChangeTag::Delete => (DiffOpKind::Delete, true, false),
                ChangeTag::Insert => (DiffOpKind::Insert, false, true),
            };
            if is_delete {
                delete_count += 1;
            } else if is_insert {
                insert_count += 1;
            } else {
                equal_count += 1;
            }
            ops.push(DiffOp {
                kind,
                value: strip_newline(change.value()),
                old_line: change.old_index().map(|i| i + 1),
                new_line: change.new_index().map(|i| i + 1),
            });
        }

        DiffResult {
            ops,
            equal_count,
            delete_count,
            insert_count,
            has_changes: delete_count > 0 || insert_count > 0,
        }
    }

    /// Group diff ops into hunks with context lines.
    pub fn hunks(&self, old_content: &str, new_content: &str) -> Vec<DiffHunk> {
        let old = self.preprocess(old_content);
        let new = self.preprocess(new_content);
        if old == new {
            return Vec::new();
        }
        let diff = TextDiff::from_lines(old.as_str(), new.as_str());
        diff.grouped_ops(self.context_lines)
            .into_iter()
            .map(|group| {
                let first = group.first().expect("group has at least one op");
                let last = group.last().expect("group has at least one op");
                let old_start = first.old_range().start + 1;
                let old_end = last.old_range().end;
                let new_start = first.new_range().start + 1;
                let new_end = last.new_range().end;
                let lines = group
                    .iter()
                    .flat_map(|op| diff.iter_changes(op))
                    .map(|change| {
                        let kind = match change.tag() {
                            ChangeTag::Equal => HunkLineKind::Context,
                            ChangeTag::Delete => HunkLineKind::Delete,
                            ChangeTag::Insert => HunkLineKind::Insert,
                        };
                        HunkLine {
                            kind,
                            value: strip_newline(change.value()),
                        }
                    })
                    .collect();
                DiffHunk {
                    old_start,
                    old_count: old_end - first.old_range().start,
                    new_start,
                    new_count: new_end - first.new_range().start,
                    lines,
                }
            })
            .collect()
    }

    /// Unified diff text with `---`/`+++` headers (when paths are given) and
    /// `@@ -s,c +s,c @@` hunks. Returns an empty string when there are no
    /// changes.
    pub fn unified_diff(
        &self,
        old_content: &str,
        new_content: &str,
        old_path: Option<&str>,
        new_path: Option<&str>,
    ) -> String {
        let hunks = self.hunks(old_content, new_content);
        if hunks.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        if let (Some(old_path), Some(new_path)) = (old_path, new_path) {
            out.push_str(&format!("--- {old_path}\n+++ {new_path}\n"));
        }
        for hunk in hunks {
            out.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
            ));
            for line in hunk.lines {
                match line.kind {
                    HunkLineKind::Context => out.push_str(&format!(" {}\n", line.value)),
                    HunkLineKind::Delete => out.push_str(&format!("-{}\n", line.value)),
                    HunkLineKind::Insert => out.push_str(&format!("+{}\n", line.value)),
                }
            }
        }
        out
    }

    /// Diff statistics with similarity ratio.
    pub fn get_stats(&self, old_content: &str, new_content: &str) -> DiffStats {
        let result = self.diff(old_content, new_content);
        let total = result.equal_count + result.delete_count + result.insert_count;
        let similarity = if total > 0 {
            result.equal_count as f64 / total as f64
        } else {
            1.0
        };
        DiffStats {
            added_lines: result.insert_count,
            removed_lines: result.delete_count,
            changed_lines: result.delete_count + result.insert_count,
            similarity,
        }
    }
}

/// Strip a trailing line terminator from a diff value.
fn strip_newline(s: &str) -> String {
    s.trim_end_matches(['\n', '\r']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_detects_added_deleted_and_unchanged() {
        let engine = DiffEngine::new();
        let result = engine.diff("a\nb\nc\n", "a\nx\nc\n");
        assert!(result.has_changes);
        assert_eq!(result.equal_count, 2);
        assert_eq!(result.delete_count, 1);
        assert_eq!(result.insert_count, 1);
        assert_eq!(result.ops.len(), 4);
    }

    #[test]
    fn diff_ops_carry_line_numbers() {
        let engine = DiffEngine::new();
        let result = engine.diff("a\nb\n", "a\nx\nb\n");
        let insert = result
            .ops
            .iter()
            .find(|op| op.kind == DiffOpKind::Insert)
            .unwrap();
        assert_eq!(insert.value, "x");
        assert_eq!(insert.new_line, Some(2));
        assert_eq!(insert.old_line, None);
        let equal = result
            .ops
            .iter()
            .find(|op| op.kind == DiffOpKind::Equal)
            .unwrap();
        assert_eq!(equal.old_line, Some(1));
        assert_eq!(equal.new_line, Some(1));
    }

    #[test]
    fn diff_identical_content_has_no_changes() {
        let engine = DiffEngine::new();
        let result = engine.diff("same\n", "same\n");
        assert!(!result.has_changes);
        assert_eq!(result.equal_count, 1);
        assert_eq!(result.ops.len(), 1);
        assert_eq!(result.ops[0].kind, DiffOpKind::Equal);
    }

    #[test]
    fn diff_empty_old_content_is_all_insert() {
        let engine = DiffEngine::new();
        let result = engine.diff("", "a\nb\n");
        assert!(result.has_changes);
        assert_eq!(result.insert_count, 2);
        assert!(result.ops.iter().all(|op| op.kind == DiffOpKind::Insert));
    }

    #[test]
    fn unified_diff_produces_hunks_with_line_numbers() {
        let engine = DiffEngine::new();
        let diff = engine.unified_diff(
            "l1\nl2\nl3\nl4\nl5\n",
            "l1\nl2\nCHANGED\nl4\nl5\n",
            Some("old.txt"),
            Some("new.txt"),
        );
        assert!(diff.starts_with("--- old.txt\n+++ new.txt\n"));
        assert!(diff.contains("@@ -1,5 +1,5 @@"), "got: {diff}");
        assert!(diff.contains("-l3"));
        assert!(diff.contains("+CHANGED"));
        assert!(diff.contains(" l2"));
        assert!(diff.contains(" l4"));
    }

    #[test]
    fn unified_diff_is_empty_when_unchanged() {
        let engine = DiffEngine::new();
        assert!(engine
            .unified_diff("x\n", "x\n", Some("a"), Some("b"))
            .is_empty());
    }

    #[test]
    fn unified_diff_splits_distant_hunks() {
        let engine = DiffEngine::new();
        let old: Vec<String> = (1..=30).map(|i| format!("line{i}")).collect();
        let mut new = old.clone();
        new[1] = "line2_edit".to_string();
        new[28] = "line29_edit".to_string();
        let diff = engine.unified_diff(&old.join("\n"), &new.join("\n"), None, None);
        // Two separate hunks — changes at lines 2 and 29 with 3 context lines.
        assert!(diff.contains("@@ -1,5 +1,5 @@"));
        assert!(diff.contains("@@ -26,5 +26,5 @@"));
        assert!(diff.contains("line2_edit"));
        assert!(diff.contains("line29_edit"));
        assert_eq!(diff.matches("@@ -").count(), 2, "got:\n{diff}");
    }

    #[test]
    fn get_stats_reports_similarity() {
        let engine = DiffEngine::new();
        let stats = engine.get_stats("a\nb\nc\n", "a\nb\nd\n");
        assert_eq!(stats.added_lines, 1);
        assert_eq!(stats.removed_lines, 1);
        assert_eq!(stats.changed_lines, 2);
        assert!((stats.similarity - 0.5).abs() < 1e-9);
    }

    #[test]
    fn trim_and_ignore_blank_lines_options() {
        let engine = DiffEngine {
            trim_lines: true,
            ignore_blank_lines: true,
            ..DiffEngine::default()
        };
        let result = engine.diff(" a \n\nb", "a\nb\n");
        assert!(!result.has_changes);
    }
}
