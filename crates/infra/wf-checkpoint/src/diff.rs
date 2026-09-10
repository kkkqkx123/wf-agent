use layertwine::engine::diff as engine;

/// Threshold for deciding whether a content slice should be treated as binary
/// when heuristically sampled (presence of NUL byte).
const PROBE_LEN: usize = 8192;

/// Heuristic binary detection. Files containing a NUL byte in the first
/// `PROBE_LEN` bytes are treated as binary; text diffs are skipped for them.
pub fn is_binary(content: &[u8]) -> bool {
    if content.is_empty() {
        return false;
    }
    let limit = content.len().min(PROBE_LEN);
    let mut probe = content.iter().take(limit);
    probe.any(|b| *b == 0)
}

/// Compute a hex-encoded SHA-256 digest of `content`. Thin wrapper around
/// [`crate::sha256_hex`] so callers inside the diff module don't re-import it.
pub fn content_hash(content: &[u8]) -> String {
    crate::sha256_hex(content)
}

/// Line-level diff statistics for a pair of text contents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffStats {
    pub added_lines: usize,
    pub removed_lines: usize,
    pub changed_lines: usize,
    pub similarity: f64,
}

/// Unified diff text with `---`/`+++` headers (when paths are given) and
/// `@@ -s,c +s,c @@` hunks. Returns an empty string when there are no
/// changes.
///
/// The Myers diff itself runs exactly once inside
/// `layertwine::engine::diff`; this wrapper only prepends optional path
/// headers so display call sites share a single algorithm.
pub fn unified_diff_text(
    before: &str,
    after: &str,
    context_lines: usize,
    old_path: Option<&str>,
    new_path: Option<&str>,
) -> String {
    if before == after {
        return String::new();
    }
    let body = engine::format_unified_diff(before, after, context_lines);
    if body.is_empty() {
        return String::new();
    }
    match (old_path, new_path) {
        (Some(old_path), Some(new_path)) => {
            format!("--- {old_path}\n+++ {new_path}\n{body}")
        }
        _ => body,
    }
}

/// Line statistics for a pair of text contents, derived from the same
/// single layertwine diff pass as [`unified_diff_text`].
pub fn diff_stats_for_text(before: &str, after: &str) -> DiffStats {
    let (added, removed, equal) = engine::diff_stat_counts(before, after);
    let total = added + removed + equal;
    let similarity = if total > 0 {
        equal as f64 / total as f64
    } else {
        1.0
    };
    DiffStats {
        added_lines: added,
        removed_lines: removed,
        changed_lines: added + removed,
        similarity,
    }
}

/// Word-level inline diff for a single replaced line pair, formatted with
/// `[-old-]` / `{+new+}` markers for frontend row highlighting.
///
/// Delegates to `layertwine::engine::word_diff`; returns `None` when the
/// lines are identical so callers render the plain line.
pub fn inline_word_diff(old_line: &str, new_line: &str) -> Option<String> {
    layertwine::engine::word_diff::diff_words(old_line, new_line).map(|d| d.format())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_produces_empty_diff() {
        assert!(unified_diff_text("x\n", "x\n", 3, None, None).is_empty());
        let stats = diff_stats_for_text("x\n", "x\n");
        assert_eq!(stats.added_lines, 0);
        assert_eq!(stats.removed_lines, 0);
        assert!((stats.similarity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn changed_content_produces_hunks_with_markers() {
        let diff = unified_diff_text(
            "l1\nl2\nl3\nl4\nl5\n",
            "l1\nl2\nCHANGED\nl4\nl5\n",
            3,
            None,
            None,
        );
        assert!(diff.contains("@@ -1,5 +1,5 @@"), "got: {diff}");
        assert!(diff.contains("-l3"));
        assert!(diff.contains("+CHANGED"));
        assert!(diff.contains(" l2"));
    }

    #[test]
    fn path_headers_only_when_both_paths_given() {
        let with_paths = unified_diff_text("a\n", "b\n", 3, Some("old.txt"), Some("new.txt"));
        assert!(with_paths.starts_with("--- old.txt\n+++ new.txt\n"));
        let without_paths = unified_diff_text("a\n", "b\n", 3, None, None);
        assert!(!without_paths.starts_with("---"));
    }

    #[test]
    fn distant_changes_split_into_two_hunks() {
        let old: Vec<String> = (1..=30).map(|i| format!("line{i}")).collect();
        let mut new = old.clone();
        new[1] = "line2_edit".to_string();
        new[28] = "line29_edit".to_string();
        let diff = unified_diff_text(&old.join("\n"), &new.join("\n"), 3, None, None);
        assert_eq!(diff.matches("@@ -").count(), 2, "got:\n{diff}");
    }

    #[test]
    fn stats_report_added_removed_and_similarity() {
        let stats = diff_stats_for_text("a\nb\nc\n", "a\nb\nd\n");
        assert_eq!(stats.added_lines, 1);
        assert_eq!(stats.removed_lines, 1);
        assert_eq!(stats.changed_lines, 2);
        assert!((stats.similarity - 0.5).abs() < 1e-9);
    }

    #[test]
    fn binary_detection_uses_nul_probe() {
        assert!(is_binary(b"a\0b"));
        assert!(!is_binary(b"plain text\n"));
        assert!(!is_binary(b""));
    }

    #[test]
    fn content_hash_is_stable_sha256_hex() {
        assert_eq!(content_hash(b"hello"), content_hash(b"hello"));
        assert_ne!(content_hash(b"hello"), content_hash(b"world"));
        assert_eq!(content_hash(b"hello").len(), 64);
    }

    #[test]
    fn inline_word_diff_marks_changed_words() {
        assert_eq!(inline_word_diff("same", "same"), None);
        let marked = inline_word_diff("hello world", "hello rust").unwrap();
        assert!(marked.contains("[-world-]"), "got: {marked}");
        assert!(marked.contains("{+rust+}"), "got: {marked}");
    }
}
