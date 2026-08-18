use std::sync::Arc;

use crate::resolver::VfsProvider;
use crate::security::SecurityValidator;

/// Split a command line into sub-commands on `;`, `&&`, `||` and `|` while
/// respecting single/double quotes and backslash escapes (so `2>&1` and
/// `echo "a;b"` stay intact). Hand-written because shlex alone does not
/// understand the separator operators.
pub fn parse_command_chain(command: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;
    let chars: Vec<char> = command.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        if escaped {
            current.push(ch);
            escaped = false;
            i += 1;
            continue;
        }
        match ch {
            '\\' if !in_single_quote => {
                current.push('\\');
                escaped = true;
            }
            '\'' if !in_double_quote => {
                current.push('\'');
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                current.push('"');
                in_double_quote = !in_double_quote;
            }
            '&' | '|'
                if !in_single_quote && !in_double_quote && i + 1 < len && chars[i + 1] == ch =>
            {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
                i += 1;
            }
            '|' | ';' if !in_single_quote && !in_double_quote => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        commands.push(trimmed);
    }

    commands
}

pub fn tokenize_command(command: &str) -> Vec<String> {
    shlex::split(command).unwrap_or_default()
}

enum RedirectKind {
    Read,
    Write,
}

/// Detect a redirect token (`>file`, `>>file`, `2>file`, `&>file`, `<file`,
/// `2<file`, ...). `>&2`-style fd duplication, heredocs (`<<`) and herestrings
/// (`<<<`) are not file accesses and yield `None`. An empty target means the
/// next token carries the path.
fn parse_redirect_token(tok: &str) -> Option<(RedirectKind, String)> {
    let t = tok.trim();
    if t.is_empty() {
        return None;
    }

    let digit_count = t.chars().take_while(|c| c.is_ascii_digit()).count();
    let mut rest = &t[digit_count.min(t.len())..];

    // `&>file` (stdout+stderr) — treat as a write redirect.
    if rest.starts_with('&') {
        if let Some(after) = rest.strip_prefix("&>") {
            let target = after.trim_start().to_string();
            if target.is_empty() {
                return Some((RedirectKind::Write, String::new()));
            }
            if target.starts_with('&') {
                return None; // >&2 style fd duplication
            }
            return Some((RedirectKind::Write, target));
        }
        return None;
    }

    if rest.is_empty() {
        return None;
    }

    let op = rest.chars().next().unwrap();
    rest = &rest[op.len_utf8()..];
    match op {
        '>' => {
            if let Some(stripped) = rest.strip_prefix('>') {
                rest = stripped; // `>>` append
            }
            let target = rest.trim_start().to_string();
            if target.is_empty() {
                return Some((RedirectKind::Write, String::new()));
            }
            if target.starts_with('&') {
                return None; // fd duplication
            }
            Some((RedirectKind::Write, target))
        }
        '<' => {
            // `<<` heredoc and `<<<` herestring markers are not paths.
            if rest.starts_with('<') || rest.starts_with('>') {
                return None;
            }
            let target = rest.trim_start().to_string();
            if target.is_empty() {
                return Some((RedirectKind::Read, String::new()));
            }
            if target.starts_with('&') {
                return None;
            }
            Some((RedirectKind::Read, target))
        }
        _ => None,
    }
}

fn looks_like_path(t: &str) -> bool {
    t.starts_with('/')
        || t.starts_with("./")
        || t.starts_with("../")
        || t.starts_with("~/")
        || t.contains('/')
        || t.starts_with('.')
}

/// Extract read and write paths from a tokenized sub-command.
/// Positional arguments are reads; `>`-style redirect targets are writes;
/// `<`-style redirect targets are reads.
pub fn extract_file_paths(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    let mut reads = Vec::new();
    let mut writes = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i].trim().to_string();
        if t.is_empty() {
            i += 1;
            continue;
        }

        if let Some((kind, target)) = parse_redirect_token(&t) {
            if !target.is_empty() {
                match kind {
                    RedirectKind::Read => {
                        if !reads.contains(&target) {
                            reads.push(target);
                        }
                    }
                    RedirectKind::Write => {
                        if !writes.contains(&target) {
                            writes.push(target);
                        }
                    }
                }
                i += 1;
                continue;
            }
            // Redirect with an empty target: the path is the next token.
            if let Some(next) = tokens.get(i + 1) {
                let next_t = next.trim().to_string();
                if !next_t.is_empty() && parse_redirect_token(&next_t).is_none() {
                    match kind {
                        RedirectKind::Read => {
                            if !reads.contains(&next_t) {
                                reads.push(next_t);
                            }
                        }
                        RedirectKind::Write => {
                            if !writes.contains(&next_t) {
                                writes.push(next_t);
                            }
                        }
                    }
                }
            }
            i += 2;
            continue;
        }

        if looks_like_path(&t) && !reads.contains(&t) {
            reads.push(t);
        }
        i += 1;
    }

    (reads, writes)
}

/// Shared VFS path check used by the shell analysis gates
/// (`static-analyzer` and `vfs-gate`): validate every extracted path with
/// `SecurityValidator` first, then `check_read`/`check_write` against the VFS
/// policy. Returns the first violation message, or `None` when all paths pass.
pub async fn check_vfs_paths(tokens: &[String], vfs: &Arc<dyn VfsProvider>) -> Option<String> {
    let (reads, writes) = extract_file_paths(tokens);
    if reads.is_empty() && writes.is_empty() {
        return None;
    }

    for path in reads.iter().chain(writes.iter()) {
        let violations = SecurityValidator::validate_path(path);
        if !violations.is_empty() {
            return Some(format!(
                "Path '{}' security violation: {}",
                path, violations[0].reason
            ));
        }
    }

    for path in &reads {
        if let Err(e) = vfs.check_read(path).await {
            return Some(format!("VFS denied read access to '{path}': {e}"));
        }
    }

    for path in &writes {
        if let Err(e) = vfs.check_write(path).await {
            return Some(format!("VFS denied write access to '{path}': {e}"));
        }
    }

    None
}
