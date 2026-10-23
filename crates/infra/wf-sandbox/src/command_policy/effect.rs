use super::evaluate::primary_command;
use crate::strategy::shell::base::ShellType;
use crate::strategy::shell::vfs_paths::{
    extract_file_paths, parse_command_chain, tokenize_command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellEffect {
    Read,
    Write,
    Unknown,
}

impl ShellEffect {
    pub fn is_read(self) -> bool {
        matches!(self, ShellEffect::Read)
    }

    pub fn needs_exclusive_window(self) -> bool {
        !matches!(self, ShellEffect::Read)
    }
}

/// Built-in read/write command lists for shell checkpoint sampling.
///
/// Single built-in source: `classify_shell_effect` falls back to these when
/// the caller passes `None`. File profiles (`sandbox.toml`) and host tool
/// config (`tools.toml [shell]`) override by passing explicit lists; an
/// explicit empty list disables the fallback for that side.
pub const DEFAULT_READ_COMMANDS: &[&str] = &[
    "ls", "cat", "pwd", "echo", "grep", "rg", "find", "head", "tail", "wc", "sort", "uniq", "diff",
    "file", "stat", "du", "df", "uname", "whoami", "hostname", "date", "env", "printenv", "which",
];

pub const DEFAULT_WRITE_COMMANDS: &[&str] = &[
    "cp", "mv", "rm", "mkdir", "touch", "ln", "chmod", "chown", "cargo", "rustc", "make", "npm",
    "npx", "pnpm", "yarn", "node", "python", "python3", "pip", "curl", "wget", "sh", "bash", "zsh",
    "tar", "unzip", "tee",
];

const READ_GIT_SUBCOMMANDS: &[&str] = &[
    "status",
    "log",
    "diff",
    "show",
    "branch",
    "tag",
    "remote",
    "ls-files",
    "rev-parse",
    "--version",
];

fn contains_dynamic_construct(sub: &str) -> bool {
    sub.contains("$(") || sub.contains('`') || sub.contains("${")
}

fn primary_in_list(primary: &str, list: &[String]) -> bool {
    list.iter().any(|item| item == primary)
}

fn classify_single(
    sub: &str,
    shell_type: ShellType,
    read_commands: &[String],
    write_commands: &[String],
) -> ShellEffect {
    let tokens = tokenize_command(sub);
    if tokens.is_empty() {
        return ShellEffect::Read;
    }
    let (_, writes) = extract_file_paths(&tokens);
    if !writes.is_empty() {
        return ShellEffect::Write;
    }
    if contains_dynamic_construct(sub) {
        return ShellEffect::Unknown;
    }
    let primary = primary_command(&tokens, shell_type)
        .unwrap_or_default()
        .to_lowercase();
    if primary.is_empty() {
        return ShellEffect::Unknown;
    }
    if primary_in_list(&primary, read_commands) {
        return ShellEffect::Read;
    }
    if primary == "git" {
        let second = tokens.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
        let second_trimmed = second.trim_start_matches('-').to_string();
        if READ_GIT_SUBCOMMANDS.contains(&second.as_str())
            || READ_GIT_SUBCOMMANDS.contains(&second_trimmed.as_str())
        {
            return ShellEffect::Read;
        }
        return ShellEffect::Unknown;
    }
    if primary_in_list(&primary, write_commands) {
        return ShellEffect::Write;
    }
    ShellEffect::Unknown
}

pub fn default_read_commands() -> Vec<String> {
    DEFAULT_READ_COMMANDS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

pub fn default_write_commands() -> Vec<String> {
    DEFAULT_WRITE_COMMANDS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

pub fn classify_shell_effect(
    command: &str,
    shell_type: ShellType,
    read_commands: Option<&[String]>,
    write_commands: Option<&[String]>,
) -> ShellEffect {
    if command.trim().is_empty() {
        return ShellEffect::Read;
    }
    let owned_read;
    let owned_write;
    let read_list: &[String] = match read_commands {
        Some(list) => list,
        None => {
            owned_read = default_read_commands();
            &owned_read
        }
    };
    let write_list: &[String] = match write_commands {
        Some(list) => list,
        None => {
            owned_write = default_write_commands();
            &owned_write
        }
    };
    let read_list = if read_list.is_empty() { &[] } else { read_list };
    let mut has_unknown = false;
    for sub in parse_command_chain(command) {
        match classify_single(&sub, shell_type, read_list, write_list) {
            ShellEffect::Write => return ShellEffect::Write,
            ShellEffect::Unknown => has_unknown = true,
            ShellEffect::Read => {}
        }
    }
    if has_unknown {
        ShellEffect::Unknown
    } else {
        ShellEffect::Read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bash() -> ShellType {
        ShellType::Bash
    }

    #[test]
    fn read_commands_classify_as_read() {
        assert_eq!(
            classify_shell_effect("ls -la", bash(), None, None),
            ShellEffect::Read
        );
        assert_eq!(
            classify_shell_effect("cat file.txt", bash(), None, None),
            ShellEffect::Read
        );
        assert_eq!(
            classify_shell_effect("git status", bash(), None, None),
            ShellEffect::Read
        );
        assert_eq!(
            classify_shell_effect("ls | grep foo", bash(), None, None),
            ShellEffect::Read
        );
    }

    #[test]
    fn redirect_promotes_to_write() {
        assert_eq!(
            classify_shell_effect("echo hi > out.txt", bash(), None, None),
            ShellEffect::Write
        );
        assert_eq!(
            classify_shell_effect("ls > out.txt", bash(), None, None),
            ShellEffect::Write
        );
    }

    #[test]
    fn unknown_commands_are_conservative() {
        assert_eq!(
            classify_shell_effect("cargo test", bash(), None, None),
            ShellEffect::Write
        );
        assert_eq!(
            classify_shell_effect("python script.py", bash(), None, None),
            ShellEffect::Write
        );
        assert_eq!(
            classify_shell_effect("my-binary --fast", bash(), None, None),
            ShellEffect::Unknown
        );
        assert_eq!(
            classify_shell_effect("echo $(whoami)", bash(), None, None),
            ShellEffect::Unknown
        );
        assert_eq!(
            classify_shell_effect("git commit -m ok", bash(), None, None),
            ShellEffect::Unknown
        );
    }

    #[test]
    fn chain_takes_most_severe() {
        assert_eq!(
            classify_shell_effect("ls && cargo test", bash(), None, None),
            ShellEffect::Write
        );
        assert_eq!(
            classify_shell_effect("ls && custom-tool", bash(), None, None),
            ShellEffect::Unknown
        );
    }
}
