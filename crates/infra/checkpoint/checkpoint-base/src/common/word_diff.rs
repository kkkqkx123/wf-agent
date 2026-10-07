use similar::{ChangeTag, TextDiff};

/// A single word-level change within a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordChange {
    pub prefix: String,
    pub old: String,
    pub new: String,
}

/// Word-level diff result for a single replaced line pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordDiff {
    pub changes: Vec<WordChange>,
}

impl WordDiff {
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

    pub fn format_inline(&self) -> String {
        let mut out = String::new();
        for change in &self.changes {
            out.push_str(&change.prefix);
            out.push_str(&change.new);
        }
        out
    }
}

/// Compute word-level diff between two lines.
/// Returns `None` if the inputs are identical.
pub fn diff_words(old_line: &str, new_line: &str) -> Option<WordDiff> {
    if old_line == new_line {
        return None;
    }

    let diff = TextDiff::from_words(old_line, new_line);

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
            current_old.push_str(&delete_text);
            current_new.push_str(&insert_text);
            in_change = true;
        }
    }

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
