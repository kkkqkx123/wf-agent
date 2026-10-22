
/// Collect every literal keyword that can trigger a deny decision from the
/// effective dangerous patterns and deny rules. Missing a keyword here only
/// means the command falls back to the full analysis — never a bypass.
pub fn quick_reject_keywords(patterns: &[String], commands: &[String]) -> Vec<String> {
    let mut set = std::collections::HashSet::new();
    for p in patterns {
        for t in super::normalize::extract_literal_tokens(p) {
            set.insert(t);
        }
    }
    for c in commands {
        for t in super::normalize::extract_literal_tokens(c) {
            set.insert(t);
        }
    }
    set.into_iter().collect()
}

/// Returns `true` when `command` contains none of the keywords and may be
/// fast-rejected (skipped). Callers must first guarantee the conservative
/// guard (no quotes / backslashes / command substitution) so a masked or
/// escaped danger is never shortcut.
pub fn quick_reject(command: &str, keywords: &[String]) -> bool {
    if keywords.is_empty() {
        return true;
    }
    let lower = command.to_lowercase();
    !keywords.iter().any(|k| lower.contains(k))
}

/// Mask shell comments only (`# ...`), keeping single-quoted content visible.
/// Used for inline-script bodies where quote-hiding would be a bypass.
pub fn mask_comments_only(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '#' && (i == 0 || chars[i - 1].is_whitespace()) {
            while i < chars.len() {
                out.push(' ');
                i += 1;
            }
            break;
        }
        out.push(ch);
        i += 1;
    }
    out
}