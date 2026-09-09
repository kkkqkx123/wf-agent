//! Word-level diff engine for fine-grained change visualization.
//!
//! When a line-level diff shows a Replace operation, the entire line appears
//! changed. Word-level diff drills into those replaced lines to show exactly
//! which tokens (words) changed, similar to `git diff --word-diff`.

use similar::{ChangeTag, TextDiff};

/// A single word-level change within a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordChange {
    /// The unchanged text before this change.
    pub prefix: String,
    /// The old word(s) that were removed (empty for insertions).
    pub old: String,
    /// The new word(s) that were added (empty for deletions).
    pub new: String,
}

/// Word-level diff result for a single replaced line pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordDiff {
    /// The line-level changes expressed as word-level operations.
    pub changes: Vec<WordChange>,
}

impl WordDiff {
    /// Format as a human-readable string with change markers.
    /// Deletions are wrapped in `[-old-]`, insertions in `{+new+}`.
    pub fn format(&self) -> String {
        let mut out = String::new();
        for change in &self.changes {
            out.push_str(&change.prefix);
            if !change.old.is_empty() {
                out.push_str("[-");
                out.push_str(&change.old);
                out.push_str("-]");
            }
            if !change.new.is_empty() {
                out.push_str("{+");
                out.push_str(&change.new);
                out.push_str("+}");
            }
        }
        out
    }

    /// Format as a simple inline diff (old → new) without markers.
    /// Includes the unchanged prefix for each change.
    pub fn format_inline(&self) -> String {
        let mut out = String::new();
        for change in &self.changes {
            out.push_str(&change.prefix);
            out.push_str(&change.new);
        }
        out
    }
}

/// Compute word-level diff between two lines (single-line texts).
///
/// Uses `similar::TextDiff::from_words` to split each line into words and
/// produce a word-level diff. Returns `None` if the inputs are identical.
///
/// This is useful when a line-level Replace operation hides the actual
/// token-level changes within the line.
pub fn diff_words(old_line: &str, new_line: &str) -> Option<WordDiff> {
    if old_line == new_line {
        return None;
    }

    let diff = TextDiff::from_words(old_line, new_line);

    // Collect all changes in order using iter_changes for accurate text values
    let mut changes = Vec::new();
    let mut current_prefix = String::new();
    let mut current_old = String::new();
    let mut current_new = String::new();
    let mut in_change = false;

    for op in diff.ops() {
        let equal_text: String = diff
            .iter_changes(op)
            .filter(|c| c.tag() == ChangeTag::Equal)
            .map(|c| c.value().to_string())
            .collect();

        let delete_text: String = diff
            .iter_changes(op)
            .filter(|c| c.tag() == ChangeTag::Delete)
            .map(|c| c.value().to_string())
            .collect();

        let insert_text: String = diff
            .iter_changes(op)
            .filter(|c| c.tag() == ChangeTag::Insert)
            .map(|c| c.value().to_string())
            .collect();

        if op.tag() == similar::DiffTag::Equal {
            // If we were in a change, flush it
            if in_change {
                changes.push(WordChange {
                    prefix: current_prefix.clone(),
                    old: current_old.clone(),
                    new: current_new.clone(),
                });
                current_prefix.clear();
                current_old.clear();
                current_new.clear();
                in_change = false;
            }
            current_prefix.push_str(&equal_text);
        } else {
            // Start or continue a change
            current_old.push_str(&delete_text);
            current_new.push_str(&insert_text);
            in_change = true;
        }
    }

    // Flush the last change
    if in_change || !current_old.is_empty() || !current_new.is_empty() {
        changes.push(WordChange {
            prefix: current_prefix,
            old: current_old,
            new: current_new,
        });
    }

    if changes.is_empty() {
        None
    } else {
        Some(WordDiff { changes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_words_identical() {
        assert!(diff_words("hello world", "hello world").is_none());
    }

    #[test]
    fn test_diff_words_single_change() {
        let result = diff_words("hello world", "hello rust").unwrap();
        assert!(!result.changes.is_empty());
        let formatted = result.format();
        assert!(formatted.contains("[-world-]"));
        assert!(formatted.contains("{+rust+}"));
    }

    #[test]
    fn test_diff_words_insertion() {
        let result = diff_words("hello", "hello beautiful world").unwrap();
        assert!(!result.changes.is_empty());
    }

    #[test]
    fn test_diff_words_deletion() {
        let result = diff_words("hello beautiful world", "hello").unwrap();
        assert!(!result.changes.is_empty());
    }

    #[test]
    fn test_diff_words_multiple_changes() {
        let result = diff_words("the cat sat", "the dog ran").unwrap();
        assert!(result.changes.len() >= 2);
    }

    #[test]
    fn test_word_diff_format_inline() {
        let result = diff_words("hello world", "hello rust").unwrap();
        let inline = result.format_inline();
        // The inline format should produce the new text
        assert_eq!(inline, "hello rust");
    }

    #[test]
    fn test_diff_words_empty_old() {
        let result = diff_words("", "new content").unwrap();
        assert!(!result.changes.is_empty());
    }

    #[test]
    fn test_diff_words_empty_new() {
        let result = diff_words("old content", "").unwrap();
        assert!(!result.changes.is_empty());
    }
}
