//! Universal fuzzy sequence matching utilities.
//!
//! Implements multi-pass sequence matching (exact, trim-end, trim,
//! Unicode-normalized) to locate patterns within text lines. Shared by the
//! apply_patch and apply_diff tools.

/// Normalize common Unicode punctuation to ASCII equivalents. This allows
/// patches written with plain ASCII to match source files containing
/// typographic characters.
pub fn normalize_unicode(s: &str) -> String {
    s.trim().chars().map(normalize_char).collect()
}

fn normalize_char(c: char) -> char {
    match c {
        // Various dash/hyphen code-points -> ASCII '-'
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{2212}' => '-',
        // Fancy single quotes -> '\''
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
        // Fancy double quotes -> '"'
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
        // Non-breaking space and other odd spaces -> normal space
        '\u{00A0}' | '\u{2002}' | '\u{2003}' | '\u{2004}' | '\u{2005}' | '\u{2006}'
        | '\u{2007}' | '\u{2008}' | '\u{2009}' | '\u{200A}' | '\u{202F}' | '\u{205F}'
        | '\u{3000}' => ' ',
        _ => c,
    }
}

fn exact_match(lines: &[String], pattern: &[String], start_index: usize) -> bool {
    pattern
        .iter()
        .enumerate()
        .all(|(i, p)| lines.get(start_index + i).map(|l| l == p).unwrap_or(false))
}

fn trim_end_match(lines: &[String], pattern: &[String], start_index: usize) -> bool {
    pattern.iter().enumerate().all(|(i, p)| {
        lines
            .get(start_index + i)
            .map(|l| l.trim_end() == p.trim_end())
            .unwrap_or(false)
    })
}

fn trim_match(lines: &[String], pattern: &[String], start_index: usize) -> bool {
    pattern.iter().enumerate().all(|(i, p)| {
        lines
            .get(start_index + i)
            .map(|l| l.trim() == p.trim())
            .unwrap_or(false)
    })
}

fn normalized_match(lines: &[String], pattern: &[String], start_index: usize) -> bool {
    pattern.iter().enumerate().all(|(i, p)| {
        lines
            .get(start_index + i)
            .map(|l| normalize_unicode(l) == normalize_unicode(p))
            .unwrap_or(false)
    })
}

/// Attempt to find the sequence of pattern lines within lines beginning at or
/// after start. Returns the starting index of the match or None if not found.
///
/// Matches are attempted with decreasing strictness:
/// 1. Exact match
/// 2. Ignoring trailing whitespace
/// 3. Ignoring leading and trailing whitespace
/// 4. Unicode-normalized (handles typographic characters)
///
/// When `eof` is true, first try starting at the end of the file (so patterns
/// intended to match file endings are applied at the end), and fall back to
/// searching from start if needed.
pub fn seek_sequence(
    lines: &[String],
    pattern: &[String],
    start: usize,
    eof: bool,
) -> Option<usize> {
    if pattern.is_empty() {
        return Some(start);
    }
    if pattern.len() > lines.len() {
        return None;
    }

    let search_start = if eof {
        lines.len().saturating_sub(pattern.len())
    } else {
        start
    };
    let max_start = lines.len() - pattern.len();
    if search_start > max_start {
        return None;
    }

    for i in search_start..=max_start {
        if exact_match(lines, pattern, i) {
            return Some(i);
        }
    }
    for i in search_start..=max_start {
        if trim_end_match(lines, pattern, i) {
            return Some(i);
        }
    }
    for i in search_start..=max_start {
        if trim_match(lines, pattern, i) {
            return Some(i);
        }
    }
    (search_start..=max_start).find(|&i| normalized_match(lines, pattern, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_exact_match() {
        let lines = v(&["foo", "bar", "baz"]);
        let found = seek_sequence(&lines, &v(&["bar"]), 0, false);
        assert_eq!(found, Some(1));
    }

    #[test]
    fn test_empty_pattern_returns_start() {
        let lines = v(&["foo", "bar"]);
        assert_eq!(seek_sequence(&lines, &[], 1, false), Some(1));
    }

    #[test]
    fn test_pattern_longer_than_lines() {
        let lines = v(&["foo"]);
        assert_eq!(seek_sequence(&lines, &v(&["a", "b"]), 0, false), None);
    }

    #[test]
    fn test_trim_end_and_trim() {
        let lines = v(&["foo  ", "bar"]);
        assert_eq!(seek_sequence(&lines, &v(&["foo"]), 0, false), Some(0));
        let lines = v(&["  foo", "bar"]);
        assert_eq!(seek_sequence(&lines, &v(&["foo"]), 0, false), Some(0));
    }

    #[test]
    fn test_unicode_normalization() {
        let lines = v(&["foo—bar"]);
        assert_eq!(seek_sequence(&lines, &v(&["foo-bar"]), 0, false), Some(0));
        let lines = v(&["\u{201C}quoted\u{201D}"]);
        assert_eq!(
            seek_sequence(&lines, &v(&["\"quoted\""]), 0, false),
            Some(0)
        );
    }

    #[test]
    fn test_eof_mode_prefers_end() {
        let lines = v(&["x", "pat", "y", "pat"]);
        assert_eq!(seek_sequence(&lines, &v(&["pat"]), 0, true), Some(3));
    }

    #[test]
    fn test_not_found() {
        let lines = v(&["foo", "bar"]);
        assert_eq!(seek_sequence(&lines, &v(&["zzz"]), 0, false), None);
    }
}
