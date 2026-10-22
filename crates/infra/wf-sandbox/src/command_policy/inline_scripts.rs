use super::*;

/// Interpreters whose `-c` / `-e` / `-r` / `--command` argument carries an
/// inline script that must be scanned for dangerous content.
pub const INLINE_INTERPRETERS: &[&str] = &[
    "python", "python2", "python3", "bash", "sh", "zsh", "node", "nodejs", "ruby", "perl", "php",
    "lua", "luajit",
];
pub const INLINE_CODE_FLAGS: &[&str] = &[
    "-c",
    "-e",
    "-r",
    "--command",
    "-Command",
    "-command",
    "-EncodedCommand",
];

/// Extract inline script bodies (`python3 -c '...'`, `bash -c '...'`,
/// `node -e '...'`, heredoc `<<EOF ... EOF`) for a second dangerous-pattern
/// pass. The extracted bodies are scanned verbatim (single quotes are NOT
/// masked) so `os.system('rm -rf /')` is caught.
pub fn extract_inline_scripts(command: &str, shell_type: ShellType) -> Vec<String> {
    let mut out = Vec::new();

    // 1. `<interpreter> <code-flag> <code>` (quotes already resolved by shlex).
    let tokens = shlex::split(command).unwrap_or_default();
    for i in 0..tokens.len() {
        if !INLINE_INTERPRETERS.contains(&tokens[i].to_lowercase().as_str()) {
            continue;
        }
        if let Some(flag) = tokens.get(i + 1) {
            if INLINE_CODE_FLAGS.contains(&flag.as_str()) {
                if let Some(code) = tokens.get(i + 2) {
                    // `-EncodedCommand` payloads are base64 — flag them
                    // without attempting to scan the blob.
                    if flag.eq_ignore_ascii_case("-encodedcommand") {
                        out.push("<base64-encoded-command>".to_string());
                    } else {
                        out.push(code.clone());
                    }
                }
            }
        }
    }

    // 2. Heredoc bodies: `<<[-]WORD` ... a line equal to `WORD`.
    if matches!(shell_type, ShellType::Bash | ShellType::PowerShell) {
        let mut cursor = 0usize;
        while let Some(rel) = command[cursor..].find("<<") {
            let start = cursor + rel;
            let after = command[start + 2..]
                .strip_prefix('-')
                .unwrap_or(&command[start + 2..]);
            let trimmed = after.trim_start();
            // Delimiter word is alphanumeric/underscore.
            let word_len = trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .count();
            if word_len == 0 {
                break;
            }
            let word: String = trimmed.chars().take(word_len).collect();
            let body_start = match command[start + 2..].find('\n') {
                Some(p) => start + 2 + p + 1,
                None => break,
            };
            let end_marker = format!("\n{word}");
            match command[body_start..].find(&end_marker) {
                Some(rel_end) => {
                    let body = &command[body_start..body_start + rel_end];
                    if !body.is_empty() {
                        out.push(body.to_string());
                    }
                    cursor = body_start + rel_end;
                }
                None => break,
            }
        }
    }

    out
}