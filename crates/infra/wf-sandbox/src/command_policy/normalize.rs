use super::*;

use regex::Regex;
use std::sync::OnceLock;

/// Upper bound on recursive wrapper stripping (prevents DoS).
pub const MAX_NORMALIZE_DEPTH: usize = 32;

fn fd_redirect_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\d*>&\d*")
            .expect("invariant: regex literal is a fixed pattern and must compile")
    })
}

/// Strip fd redirect tokens (`2>&1`, `3>&2`) — they carry no command identity
/// and previously tripped up prefix matching.
pub(crate) fn strip_fd_redirect(s: &str) -> String {
    fd_redirect_re().replace(s, "").to_string()
}

fn join_tokens(tokens: &[String]) -> String {
    tokens.join(" ")
}

/// Lightweight whitespace split that preserves backslashes (Windows paths).
pub(crate) fn split_raw(s: &str) -> Vec<String> {
    s.split_whitespace().map(|t| t.to_string()).collect()
}

fn is_var_assignment(t: &str) -> bool {
    if t.is_empty() || t.starts_with('-') || t.starts_with('=') {
        return false;
    }
    t.find('=').map(|eq| eq > 0).unwrap_or(false)
}

/// Strip wrapper prefixes (`sudo`, `env`, `command`, `time`, `nice`, `nohup`,
/// backslash, `start`, `@`, `&`, `$x =`) and absolute path prefixes
/// (`/usr/bin/git` → `git`, `C:\...\notepad.exe` → `notepad`) so pattern and
/// allow/deny matching sees the real command. Recursive with a fixed depth cap
/// to avoid DoS. Non-destructive: returns a string used for *matching* only;
/// the original command is never replaced for execution.
pub fn normalize_command(command: &str, shell_type: ShellType) -> String {
    let mut cur = command.trim().to_string();
    for _ in 0..MAX_NORMALIZE_DEPTH {
        let next = normalize_once(&cur, shell_type);
        if next == cur {
            break;
        }
        cur = next;
    }
    cur
}

fn normalize_once(cmd: &str, shell_type: ShellType) -> String {
    // Windows dialects keep backslashes in paths, which `shlex` would eat as
    // escapes — split them on raw whitespace instead.
    let tokens = match shell_type {
        ShellType::Cmd | ShellType::PowerShell => split_raw(cmd),
        _ => shlex::split(cmd).unwrap_or_else(|| split_raw(cmd)),
    };
    if tokens.is_empty() {
        return String::new();
    }

    let first = tokens[0].as_str();
    let mut idx = 0usize;

    match shell_type {
        ShellType::Bash => {
            // Absolute path prefix: /usr/bin/git -> git
            if first.starts_with('/') && first.len() > 1 {
                let base = first.rsplit('/').next().unwrap_or(first);
                if !base.is_empty() {
                    let mut out = vec![base.to_string()];
                    out.extend(tokens[1..].iter().cloned());
                    return join_tokens(&out);
                }
            }
            match first {
                "sudo" => idx = skip_sudo_args(&tokens, 1),
                "env" => idx = skip_env_args(&tokens, 1),
                "command" => {
                    // `command -v` / `command -V` are queries — never strip.
                    if tokens
                        .get(1)
                        .map(|t| t == "-v" || t == "-V")
                        .unwrap_or(false)
                    {
                        return cmd.trim().to_string();
                    }
                    idx = 1;
                    if tokens
                        .get(idx)
                        .map(|t| t == "-p" || t == "--")
                        .unwrap_or(false)
                    {
                        idx += 1;
                    }
                }
                "time" => idx = 1,
                "nice" => {
                    idx = 1;
                    if tokens.get(idx).map(|t| t == "-n").unwrap_or(false) {
                        idx += 2;
                    }
                }
                "nohup" => idx = 1,
                "\\" => idx = 1,
                _ => {}
            }
        }
        ShellType::Cmd => {
            if first.eq_ignore_ascii_case("start") {
                idx = 1;
                while idx < tokens.len() && tokens[idx].starts_with('/') {
                    idx += 1;
                }
            } else if first.starts_with('@') && first.len() > 1 {
                let mut out = vec![first[1..].to_string()];
                out.extend(tokens[1..].iter().cloned());
                return join_tokens(&out);
            }
        }
        ShellType::PowerShell => {
            if first == "&" {
                idx = 1;
            } else if first.starts_with('$') && tokens.get(1).map(|t| t == "=").unwrap_or(false) {
                idx = 2;
            }
        }
    }

    if idx >= tokens.len() {
        return String::new();
    }
    join_tokens(&tokens[idx..])
}

pub(crate) fn skip_sudo_args(tokens: &[String], mut idx: usize) -> usize {
    // Flags that consume a value; everything else starting with `-` is
    // skipped as a bare flag.
    const VALUE_FLAGS: &[&str] = &[
        "-u", "-g", "-C", "-D", "-R", "-r", "-t", "-T", "-p", "-P", "-U", "-A",
    ];
    while idx < tokens.len() {
        let t = tokens[idx].as_str();
        if t == "--" {
            idx += 1;
            break;
        }
        if VALUE_FLAGS.contains(&t) {
            idx += 2;
        } else if t.starts_with('-') && t.len() > 1 {
            idx += 1;
        } else {
            break;
        }
    }
    idx
}

pub(crate) fn skip_env_args(tokens: &[String], mut idx: usize) -> usize {
    while idx < tokens.len() {
        let t = tokens[idx].as_str();
        if t == "-i" || t == "--" {
            idx += 1;
        } else if t == "-u" && idx + 1 < tokens.len() {
            idx += 2;
        } else if (t.starts_with('-') && t.len() > 1) || is_var_assignment(t) {
            idx += 1;
        } else {
            break;
        }
    }
    idx
}

/// Mask single-quoted data and shell comments so dangerous-pattern matching
/// never fires on data. Conservative: double quotes and ambiguous spans are
/// kept as Executed (false positives are acceptable, false negatives are not).
pub fn mask_data_spans(command: &str, shell_type: ShellType) -> String {
    let mut out = String::with_capacity(command.len());
    let mut in_single = false;
    let mut escaped = false;
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if escaped {
            out.push(ch);
            escaped = false;
            i += 1;
            continue;
        }
        if ch == '\\' && !in_single {
            out.push(ch);
            escaped = true;
            i += 1;
            continue;
        }
        if ch == '\'' && matches!(shell_type, ShellType::Bash | ShellType::PowerShell) {
            out.push(' ');
            in_single = !in_single;
            i += 1;
            continue;
        }
        if in_single {
            out.push(' ');
            i += 1;
            continue;
        }
        // Comment: `#` at line start or after whitespace (bash/powershell).
        if ch == '#'
            && matches!(shell_type, ShellType::Bash | ShellType::PowerShell)
            && (i == 0 || chars[i - 1].is_whitespace())
        {
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

/// Extract alphanumeric-ish tokens (len >= 2) from a rule pattern or command
/// word, lowercased. Used to build the quick-reject keyword set.
pub(crate) fn extract_literal_tokens(s: &str) -> Vec<String> {
    static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    // No `.` in the character class: `curl.*` must yield `curl`, not `curl.`.
    let re = TOKEN_RE.get_or_init(|| {
        Regex::new(r"[A-Za-z_][A-Za-z0-9_\-]*")
            .expect("invariant: regex literal is a fixed pattern and must compile")
    });
    re.find_iter(s)
        .map(|m| m.as_str().to_lowercase())
        .filter(|t| t.len() >= 2)
        .collect()
}