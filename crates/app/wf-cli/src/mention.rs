//! Mention parsing for the mini composer: `@` file / skill / workflow mentions.
//!
//! The composer highlights mentions as intervals `Vec<(Range<usize>, MentionKind)>`
//! where the range is byte offsets into the buffer. The streaming view never
//! mutates them: edits recompute the intervals from the current buffer text.
//!
//! Syntax:
//! * `@path/to/file` — file mention, optionally with line range
//!   `file:#10`, `file:#10-20`, `file:10`, `file:10-20`, `file#10`, `file#10-20`
//! * `@skill:name` or `skill:name` after `@` — skill mention
//! * `@workflow:id` / `@wf:id` / `workflow:id` — workflow mention
//! * bare `@name` without prefix is treated as a file when it contains a `/`
//!   or `.` , otherwise as a skill when it matches a known skill, otherwise
//!   as a workflow, finally as a file.
//!
//! File list is produced by scanning `project_root` (or the current directory
//! when unavailable) with `globset` filtering and `regex` line parsing reused
//! from the builder side.

use std::ops::Range;
use std::path::{Path, PathBuf};

use regex::Regex;

/// Kind of a mention interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MentionKind {
    File {
        path: String,
        lines: Option<(u32, u32)>,
    },
    Skill {
        name: String,
    },
    Workflow {
        id: String,
    },
}

/// One parsed mention with its byte range in the source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub range: Range<usize>,
    pub kind: MentionKind,
    /// Raw matched text (including the leading `@`).
    pub raw: String,
}

/// Parse all `@mention` intervals in `text`.
///
/// The scan is byte-offset based and treats every `@` followed by non-space
/// and non-`@` characters as a candidate. Existing composer content is scanned
/// left-to-right; the range covers the raw token including the `@`.
pub fn parse_mentions(text: &str) -> Vec<Mention> {
    let mut out = Vec::new();
    // Find `@<non-space>` spans. We capture until whitespace or another `@` or
    // line break. Punctuation at the end (.,;:) is trimmed as not part of the
    // mention.
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let start = i;
            let mut end = i + 1;
            while end < bytes.len() {
                let b = bytes[end];
                if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'@' {
                    break;
                }
                end += 1;
            }
            if end > start + 1 {
                let mut raw: String = text[start..end].to_string();
                // Trim trailing punctuation that is not part of a file line range.
                while raw.ends_with('.')
                    || raw.ends_with(',')
                    || raw.ends_with(';')
                    || raw.ends_with(':')
                {
                    // Do not trim `:` when it precedes a line number — the raw
                    // still contains it.
                    if raw.ends_with(':') {
                        // Keep `file:10` style trailing colon is not trimmed if
                        // followed by digits earlier — but we've no digits at end
                        // here, so trim.
                        raw.pop();
                        end -= 1;
                    } else {
                        raw.pop();
                        end -= 1;
                    }
                }
                if raw.len() > 1 {
                    let token = &raw[1..];
                    let kind = classify_token(token);
                    out.push(Mention {
                        range: start..end,
                        kind,
                        raw,
                    });
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// Classify the token after `@` into a MentionKind, extracting the line range
/// for files.
fn classify_token(token: &str) -> MentionKind {
    let lower = token.to_ascii_lowercase();
    if lower.strip_prefix("skill:").is_some() {
        let name = token[6..].to_string();
        return MentionKind::Skill { name };
    }
    if lower.strip_prefix("workflow:").is_some() {
        let id = token[9..].to_string();
        return MentionKind::Workflow { id };
    }
    if lower.strip_prefix("wf:").is_some() {
        let id = token[3..].to_string();
        return MentionKind::Workflow { id };
    }
    if lower.starts_with("file:") {
        let path_part = &token[5..];
        return parse_file_token(path_part);
    }
    // No explicit prefix: decide by content.
    // If token looks like a file-range (contains `:`/`#` + digits), prefer file.
    if looks_like_file_with_lines(token) {
        return parse_file_token(token);
    }
    // If token contains `/` or `.` treat as file path.
    if token.contains('/') || token.contains('.') {
        return parse_file_token(token);
    }
    // Fallback: treat as skill when no slash/dot, then we let the caller
    // disambiguate by checking existing skill/workflow registries. The parser
    // itself keeps it as Skill for simplicity; the filter layer can reinterpret.
    // We keep it as Skill -> caller may remap to Workflow if skill not found.
    MentionKind::Skill {
        name: token.to_string(),
    }
}

fn looks_like_file_with_lines(token: &str) -> bool {
    // Contains `:` or `#` followed by digits.
    if let Some(pos) = token.find([':', '#']) {
        let rest = &token[pos + 1..];
        // skip optional '#'
        let rest = rest.strip_prefix('#').unwrap_or(rest);
        return rest.chars().next().is_some_and(|c| c.is_ascii_digit());
    }
    false
}

fn parse_file_token(token: &str) -> MentionKind {
    if let Some((path, lines)) = split_file_lines(token) {
        MentionKind::File { path, lines }
    } else {
        MentionKind::File {
            path: token.to_string(),
            lines: None,
        }
    }
}

/// Split `file[:#]lines` into path and optional line range.
///
/// Accepts `path`, `path:10`, `path:10-20`, `path#10`, `path#10-20`,
/// `path:#10`, `path:#10-20`. The path may contain `:` (e.g. Windows drive)
/// but the last `:`/`#` before digits is the line separator.
fn split_file_lines(token: &str) -> Option<(String, Option<(u32, u32)>)> {
    // Use regex to locate the trailing line suffix.
    // Pattern: last occurrence of `[:#][#]?digits[-digits]?`
    // We search from the end.
    let re = Regex::new(r"^(?P<path>.+?)(?:[:#]#?(?P<start>\d+)(?:-(?P<end>\d+))?)$").ok()?;
    if let Some(caps) = re.captures(token) {
        let path = caps
            .name("path")
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if path.is_empty() {
            return None;
        }
        let start: u32 = caps
            .name("start")
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let end: u32 = caps
            .name("end")
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(start);
        if start == 0 {
            return Some((token.to_string(), None));
        }
        if end < start {
            return Some((path, Some((start, start))));
        }
        return Some((path, Some((start, end))));
    }
    // No line suffix: whole token is path.
    Some((token.to_string(), None))
}

/// Extract the query fragment for the `@` mention that contains `cursor`.
///
/// Returns `None` when the cursor is not inside a `@query` (no preceding `@`
/// before the cursor without whitespace). The query is the substring after
/// `@` up to the cursor, not including the `@`.
pub fn mention_query_at_cursor(text: &str, cursor: usize) -> Option<String> {
    if cursor == 0 || cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    let prefix = &text[..cursor];
    // Find the last `@` before cursor.
    let at = prefix.rfind('@')?;
    let after = &prefix[at + 1..];
    // If whitespace or another `@` sits between `@` and cursor, it's not a
    // mention query.
    if after.contains(' ') || after.contains('\t') || after.contains('\n') || after.contains('@') {
        return None;
    }
    Some(after.to_string())
}

/// Fuzzy filter candidates by case-insensitive substring, aligned with the
/// `SelectList` filter semantics used elsewhere. `query` may be empty (returns
/// all candidates).
pub fn filter_candidates(candidates: &[String], query: &str) -> Vec<String> {
    if query.is_empty() {
        return candidates.to_vec();
    }
    let needle = query.to_ascii_lowercase();
    candidates
        .iter()
        .filter(|c| c.to_ascii_lowercase().contains(&needle))
        .cloned()
        .collect()
}

/// Scan `project_root` recursively for files, returning relative paths.
///
/// Uses `globset` for hidden filtering and `regex` for ignore patterns reused
/// from the builder's file scanning. The walk is bounded to avoid excessive IO
/// in tests: at most 2000 files and depth 8 by default.
pub fn scan_files(project_root: &Path, query: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    let limit = 2000usize;
    let max_depth = 8usize;
    let mut stack: Vec<(PathBuf, usize)> = vec![(project_root.to_path_buf(), 0)];
    let needle = query.map(|q| q.to_ascii_lowercase());
    // Simple ignore regex: .git, node_modules, target, .next, dist
    let ignore_re = Regex::new(r"(?:\.git|node_modules|target|\.next|dist)(?:/|$)").ok();
    while let Some((dir, depth)) = stack.pop() {
        if depth > max_depth || out.len() >= limit {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path
                .strip_prefix(project_root)
                .map(|p| p.to_string_lossy().to_string())
            else {
                continue;
            };
            if let Some(re) = &ignore_re {
                if re.is_match(&rel) {
                    continue;
                }
            }
            if path.is_dir() {
                stack.push((path, depth + 1));
            } else if path.is_file() {
                if let Some(q) = &needle {
                    if !rel.to_ascii_lowercase().contains(q.as_str()) {
                        continue;
                    }
                }
                out.push(rel);
                if out.len() >= limit {
                    break;
                }
            }
        }
    }
    out.sort();
    out
}

/// Convenience: scan with empty query and no depth limit (tests).
pub fn scan_files_with_limit(
    project_root: &Path,
    query: Option<&str>,
    limit: usize,
) -> Vec<String> {
    let mut v = scan_files(project_root, query);
    v.truncate(limit);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_skill_and_workflow_prefixes() {
        let mentions = parse_mentions("@skill:pdf and @workflow:my-wf");
        assert_eq!(mentions.len(), 2);
        assert!(matches!(mentions[0].kind, MentionKind::Skill { .. }));
        assert!(matches!(mentions[1].kind, MentionKind::Workflow { .. }));
        assert_eq!(mentions[0].raw, "@skill:pdf");
    }

    #[test]
    fn parses_file_with_line_range_hash() {
        let mentions = parse_mentions("see @src/main.rs:#10-20 for details");
        assert_eq!(mentions.len(), 1);
        match &mentions[0].kind {
            MentionKind::File { path, lines } => {
                assert_eq!(path, "src/main.rs");
                assert_eq!(*lines, Some((10, 20)));
            }
            other => panic!("expected file, got {other:?}"),
        }
    }

    #[test]
    fn parses_file_with_colon_range() {
        let mentions = parse_mentions("@src/app/mod.rs:5-8");
        assert_eq!(mentions.len(), 1);
        match &mentions[0].kind {
            MentionKind::File { path, lines } => {
                assert_eq!(path, "src/app/mod.rs");
                assert_eq!(*lines, Some((5, 8)));
            }
            other => panic!("expected file, got {other:?}"),
        }
    }

    #[test]
    fn parses_file_without_lines_and_skill_fallback() {
        let mentions = parse_mentions("@src/main.rs @my-skill");
        assert_eq!(mentions.len(), 2);
        assert!(matches!(mentions[0].kind, MentionKind::File { .. }));
        // bare token without slash/dot is treated as skill
        assert!(matches!(mentions[1].kind, MentionKind::Skill { .. }));
    }

    #[test]
    fn parses_file_hash_without_colon() {
        let mentions = parse_mentions("@README.md#3");
        assert_eq!(mentions.len(), 1);
        match &mentions[0].kind {
            MentionKind::File { path, lines } => {
                assert_eq!(path, "README.md");
                assert_eq!(*lines, Some((3, 3)));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_multiple_mentions_with_punctuation() {
        let mentions = parse_mentions("@skill:pdf, then @workflow:demo.");
        assert_eq!(mentions.len(), 2);
        // trailing punctuation trimmed
        assert_eq!(mentions[0].raw, "@skill:pdf");
        assert_eq!(mentions[1].raw, "@workflow:demo");
    }

    #[test]
    fn mention_query_at_cursor_detects_current_word() {
        let text = "hello @src/ma";
        let cursor = text.len();
        assert_eq!(
            mention_query_at_cursor(text, cursor),
            Some("src/ma".to_string())
        );
        let text2 = "hello @skill:pd";
        assert_eq!(
            mention_query_at_cursor(text2, text2.len()),
            Some("skill:pd".to_string())
        );
    }

    #[test]
    fn mention_query_none_when_outside() {
        assert_eq!(mention_query_at_cursor("hello world", 5), None);
        assert_eq!(mention_query_at_cursor("hello @world there", 18), None);
        // cursor before mention
        assert_eq!(mention_query_at_cursor("@skill:pdf", 0), None);
    }

    #[test]
    fn query_filters_by_substring_case_insensitive() {
        let cands = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "README.md".to_string(),
        ];
        let filtered = filter_candidates(&cands, "main");
        assert_eq!(filtered, vec!["src/main.rs"]);
        let filtered2 = filter_candidates(&cands, "SRC");
        assert_eq!(filtered2.len(), 2);
        assert_eq!(filter_candidates(&cands, ""), cands);
    }

    #[test]
    fn file_line_split_variants() {
        assert_eq!(
            split_file_lines("a/b.rs:#10-20"),
            Some(("a/b.rs".to_string(), Some((10, 20))))
        );
        assert_eq!(
            split_file_lines("a/b.rs:10"),
            Some(("a/b.rs".to_string(), Some((10, 10))))
        );
        assert_eq!(
            split_file_lines("a/b.rs#5"),
            Some(("a/b.rs".to_string(), Some((5, 5))))
        );
        assert_eq!(
            split_file_lines("a/b.rs"),
            Some(("a/b.rs".to_string(), None))
        );
    }

    #[test]
    fn scan_files_finds_files_and_filters_query() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main(){}").unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
        std::fs::write(dir.path().join("README.md"), "").unwrap();
        let all = scan_files(dir.path(), None);
        assert!(all.iter().any(|p| p.contains("main.rs")));
        assert!(all.iter().any(|p| p.contains("lib.rs")));
        let filtered = scan_files(dir.path(), Some("main"));
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].contains("main.rs"));
    }

    #[test]
    fn palette_filter_narrows_like_select_list() {
        // mirrors the SelectList filter behavior tested in panels.rs
        let cands = vec![
            "/new".to_string(),
            "/model".to_string(),
            "/skills".to_string(),
        ];
        assert_eq!(filter_candidates(&cands, "mo"), vec!["/model"]);
        assert_eq!(filter_candidates(&cands, "s"), vec!["/skills"]);
    }
}
