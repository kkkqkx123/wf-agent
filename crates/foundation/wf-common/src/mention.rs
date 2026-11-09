use std::path::Path;

/// Upper bound of files collected by one workspace scan.
pub const MENTION_SCAN_LIMIT: usize = 2000;

/// Upper bound of directory depth descended by one workspace scan.
pub const MENTION_SCAN_MAX_DEPTH: usize = 8;

/// Path segments never emitted by mention file scans.
pub fn is_ignored_path_part(part: &str) -> bool {
    matches!(part, ".git" | "node_modules" | "target" | ".next" | "dist")
}

/// Scan `root` for workspace-relative file and directory candidates.
///
/// Directories emit with a trailing slash, results sort for stable panels,
/// and `query` filters case-insensitively when present. Missing roots read
/// as empty so callers keep a single empty-list path.
pub fn scan_files(root: &Path, query: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    let needle = query.map(|q| q.to_ascii_lowercase());
    let mut stack: Vec<(std::path::PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > MENTION_SCAN_MAX_DEPTH || out.len() >= MENTION_SCAN_LIMIT {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = match path.strip_prefix(root) {
                Ok(rel) => rel.to_string_lossy().to_string(),
                Err(_) => continue,
            };
            if rel.split('/').any(is_ignored_path_part) {
                continue;
            }
            if path.is_dir() {
                let emit = match &needle {
                    Some(q) => rel.to_ascii_lowercase().contains(q.as_str()),
                    None => true,
                };
                if emit && out.len() < MENTION_SCAN_LIMIT {
                    out.push(format!("{rel}/"));
                }
                stack.push((path, depth + 1));
            } else if path.is_file() {
                if let Some(q) = &needle {
                    if !rel.to_ascii_lowercase().contains(q.as_str()) {
                        continue;
                    }
                }
                out.push(rel);
                if out.len() >= MENTION_SCAN_LIMIT {
                    break;
                }
            }
        }
    }
    out.sort();
    out
}

/// Whether `value` is a valid mention kind prefix.
pub fn is_mention_kind(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => (),
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Canonical display kind for a raw mention kind.
pub fn normalize_mention_kind(kind: &str) -> &str {
    match kind {
        "wf" => "workflow",
        "agent_loop" => "agent",
        _ => kind,
    }
}

fn is_reference_value_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '.' | '-' | '#')
}

fn parse_range_numbers(start_raw: &str, end_raw: Option<&str>) -> Option<(u32, u32)> {
    let start: u32 = start_raw.parse().ok()?;
    if start == 0 {
        return None;
    }
    let end: u32 = match end_raw {
        Some(raw) => raw.parse().ok()?,
        None => start,
    };
    if end < start {
        return Some((start, start));
    }
    Some((start, end))
}

/// Split a file value into its path and optional one-based line range.
///
/// The canonical separator is a hash suffix, while a colon suffix stays
/// accepted as an alias so existing inputs keep resolving. Generation
/// always emits the hash form.
pub fn split_file_range(raw: &str) -> (String, Option<(u32, u32)>) {
    if raw.is_empty() || !raw.ends_with(|c: char| c.is_ascii_digit()) {
        return (raw.to_string(), None);
    }
    let bytes = raw.as_bytes();
    let mut end_digits_end = bytes.len();
    let mut pos = bytes.len();
    while pos > 0 && bytes[pos - 1].is_ascii_digit() {
        pos -= 1;
    }
    let end_raw = &raw[pos..end_digits_end];
    let mut start_raw = end_raw;
    let mut sep_pos = pos;
    if pos > 0 && bytes[pos - 1] == b'-' {
        let dash = pos - 1;
        let mut start = dash;
        while start > 0 && bytes[start - 1].is_ascii_digit() {
            start -= 1;
        }
        if start == dash {
            return (raw.to_string(), None);
        }
        start_raw = &raw[start..dash];
        sep_pos = start;
        end_digits_end = dash;
        let _ = end_digits_end;
    }
    if sep_pos == 0 {
        return (raw.to_string(), None);
    }
    let sep = bytes[sep_pos - 1] as char;
    let mut path_end = sep_pos - 1;
    if sep == '#' {
        if path_end > 0 && bytes[path_end - 1] == b':' {
            path_end -= 1;
        }
    } else if sep != ':' {
        return (raw.to_string(), None);
    }
    let end_raw_opt = if start_raw == end_raw {
        None
    } else {
        Some(end_raw)
    };
    let Some((start, end)) = parse_range_numbers(start_raw, end_raw_opt) else {
        return (raw.to_string(), None);
    };
    let path = &raw[..path_end];
    if path.is_empty() {
        return (raw.to_string(), None);
    }
    (path.to_string(), Some((start, end)))
}

/// Parse a single mention token without its leading `@`.
///
/// Canonical `kind:value` references resolve first, then path shorthand
/// containing a slash or dot resolves as a file. Bare words without a
/// separator are queries rather than references and resolve to nothing.
pub fn parse_reference(token: &str) -> Option<(String, String)> {
    if token.is_empty()
        || token.contains([' ', '\t', '\n', '\r', '@'])
        || token.ends_with(':')
    {
        return None;
    }
    if let Some(colon) = token.find(':') {
        let kind = &token[..colon];
        let value = &token[colon + 1..];
        if is_mention_kind(kind)
            && !value.is_empty()
            && value.chars().all(is_reference_value_char)
        {
            return Some((kind.to_string(), value.to_string()));
        }
        return None;
    }
    if (token.contains('/') || token.contains('.'))
        && token.chars().all(is_reference_value_char)
    {
        return Some(("file".to_string(), token.to_string()));
    }
    None
}

/// Parts of a staged `[Image N]` or `[File N]` placeholder at the start of
/// `rest`: tag name, one-based index and byte length.
pub fn staged_placeholder_parts(rest: &str) -> Option<(&'static str, usize, usize)> {
    for (prefix, tag) in [("[Image ", "Image"), ("[File ", "File")] {
        let Some(tail) = rest.strip_prefix(prefix) else {
            continue;
        };
        let digits = tail.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 || digits > 6 {
            continue;
        }
        if tail.as_bytes().get(digits) != Some(&b']') {
            continue;
        }
        if tail[..digits].starts_with('0') {
            continue;
        }
        let index: usize = tail[..digits].parse().ok()?;
        if index == 0 {
            continue;
        }
        return Some((tag, index, prefix.len() + digits + 1));
    }
    None
}

/// Byte length of a leading staged placeholder.
pub fn staged_placeholder_len(rest: &str) -> Option<usize> {
    staged_placeholder_parts(rest).map(|(_, _, len)| len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_follow_the_lowercase_rule() {
        assert!(is_mention_kind("skill"));
        assert!(is_mention_kind("agent_loop"));
        assert!(!is_mention_kind("Skill"));
        assert!(!is_mention_kind(""));
        assert!(!is_mention_kind("9lives"));
    }

    #[test]
    fn aliases_fold_to_canonical_kinds() {
        assert_eq!(normalize_mention_kind("wf"), "workflow");
        assert_eq!(normalize_mention_kind("agent_loop"), "agent");
        assert_eq!(normalize_mention_kind("skill"), "skill");
    }

    #[test]
    fn file_ranges_accept_hash_and_colon_aliases() {
        assert_eq!(
            split_file_range("a/b.rs:#10-20"),
            ("a/b.rs".to_string(), Some((10, 20)))
        );
        assert_eq!(
            split_file_range("a/b.rs:10"),
            ("a/b.rs".to_string(), Some((10, 10)))
        );
        assert_eq!(
            split_file_range("a/b.rs#5"),
            ("a/b.rs".to_string(), Some((5, 5)))
        );
        assert_eq!(split_file_range("a/b.rs"), ("a/b.rs".to_string(), None));
        assert_eq!(
            split_file_range("a/b.rs:0"),
            ("a/b.rs:0".to_string(), None)
        );
    }

    #[test]
    fn references_cover_canonical_and_shorthand_forms() {
        assert_eq!(
            parse_reference("skill:pdf"),
            Some(("skill".to_string(), "pdf".to_string()))
        );
        assert_eq!(
            parse_reference("src/main.rs"),
            Some(("file".to_string(), "src/main.rs".to_string()))
        );
        assert_eq!(
            parse_reference("src/main.rs#3"),
            Some(("file".to_string(), "src/main.rs#3".to_string()))
        );
        assert_eq!(parse_reference("plain"), None);
        assert_eq!(parse_reference(""), None);
    }

    #[test]
    fn placeholders_reject_zero_and_bad_shapes() {
        assert_eq!(
            staged_placeholder_parts("[Image 1] rest"),
            Some(("Image", 1, 9))
        );
        assert_eq!(
            staged_placeholder_parts("[File 12]"),
            Some(("File", 12, 9))
        );
        assert_eq!(staged_placeholder_parts("[Image 0]"), None);
        assert_eq!(staged_placeholder_parts("[Image ]"), None);
        assert_eq!(staged_placeholder_parts("[Note 1]"), None);
    }

    #[test]
    fn scans_emit_directories_and_ignore_build_outputs() {
        let base = std::env::temp_dir().join(format!("wf-mention-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("src")).expect("mkdir");
        std::fs::create_dir_all(base.join("target")).expect("mkdir");
        std::fs::write(base.join("src/main.rs"), "fn main(){}").expect("write");
        std::fs::write(base.join("target/out"), "x").expect("write");
        let all = scan_files(&base, None);
        assert!(all.iter().any(|p| p == "src/"));
        assert!(all.iter().any(|p| p == "src/main.rs"));
        assert!(!all.iter().any(|p| p.contains("target")));
        let filtered = scan_files(&base, Some("main"));
        assert_eq!(filtered.len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }
}
