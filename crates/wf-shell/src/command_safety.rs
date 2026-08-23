use regex::Regex;

use std::sync::OnceLock;

use wf_sandbox::command_policy::{self, CommandRules};

/// Re-exported so existing consumers (`wf-tools`) keep importing
/// `wf_shell::command_safety::CommandDecision`.
pub use wf_sandbox::command_policy::CommandDecision;

fn dangerous_param_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{[^}]*@[PQEAa][^}]*\}").unwrap())
}

fn assignment_octal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{[^}]*[=+\-?][^}]*\\[0-7]{3}[^}]*\}").unwrap())
}

fn assignment_hex_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{[^}]*[=+\-?][^}]*\\x[0-9a-fA-F]{2}[^}]*\}").unwrap())
}

fn assignment_unicode_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{[^}]*[=+\-?][^}]*\\u[0-9a-fA-F]{4}[^}]*\}").unwrap())
}

fn indirect_expansion_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{![^}]+\}").unwrap())
}

fn here_string_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<<<\s*(\$\(|`)").unwrap())
}

fn zsh_glob_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[*?+@!]\(e:[^:]+:\)").unwrap())
}

/// Split a command line into sub-commands on the shell separators.
///
/// Single source of truth: delegates to the quote-aware chain parser shared
/// with the `wf-sandbox` analysis gates, so the approval layer and the
/// sandbox static analysis always agree on what a "sub-command" is.
pub fn parse_command_chain(command: &str) -> Vec<String> {
    wf_sandbox::parse_command_chain(command)
}

pub fn contains_dangerous_substitution(command: &str) -> bool {
    if dangerous_param_re().is_match(command) {
        return true;
    }
    if assignment_octal_re().is_match(command) {
        return true;
    }
    if assignment_hex_re().is_match(command) {
        return true;
    }
    if assignment_unicode_re().is_match(command) {
        return true;
    }
    if indirect_expansion_re().is_match(command) {
        return true;
    }
    if here_string_re().is_match(command) {
        return true;
    }
    if zsh_glob_re().is_match(command) {
        return true;
    }

    // zsh process substitution =(...) — check starts with = or preceded by space/semicolon/etc
    if let Some(pos) = command.find("=(") {
        if (pos == 0
            || command[..pos].ends_with(' ')
            || command[..pos].ends_with(';')
            || command[..pos].ends_with('|')
            || command[..pos].ends_with('&')
            || command[..pos].ends_with('<')
            || command[..pos].ends_with('('))
            && command[pos..].trim_end().ends_with(')')
        {
            return true;
        }
    }

    false
}

/// Longest-prefix match helper, kept for API compatibility with the legacy
/// prefix-based decision path (the unified pipeline in `wf-sandbox` uses its
/// own whitespace-aware sub-command prefix matching).
pub fn find_longest_prefix_match(command: &str, prefixes: &[String]) -> Option<String> {
    if command.is_empty() || prefixes.is_empty() {
        return None;
    }

    let trimmed_cmd = command.trim().to_lowercase();
    let mut longest: Option<String> = None;

    for prefix in prefixes {
        let lower = prefix.to_lowercase();
        if lower == "*" || trimmed_cmd.starts_with(&lower) {
            match &longest {
                Some(existing) if lower.len() > existing.to_lowercase().len() => {
                    longest = Some(prefix.clone());
                }
                None => {
                    longest = Some(prefix.clone());
                }
                _ => {}
            }
        }
    }

    longest
}

/// Immutable allow/deny command policy evaluated at the unified spawn entry.
///
/// The policy is the single source of truth for the engine-level command
/// baseline: `AutoDeny` is hard-rejected at every spawn path, while
/// `AskUser`/`AutoApprove` proceed (interactive approval is an upper-layer
/// concern). Upper crates build a policy from the same
/// [`crate::config::ShellToolConfig`] so the engine baseline and the
/// approval layer stay consistent.
///
/// This type is now a thin wrapper over the unified decision pipeline
/// (`wf_sandbox::command_policy::evaluate_command`), which also carries the
/// sandbox shell rules when they are configured, so the engine baseline and
/// the sandbox static-analysis gate always agree.
#[derive(Debug, Clone)]
pub struct CommandPolicy {
    allowed_commands: Vec<String>,
    denied_commands: Option<Vec<String>>,
    shell_policy: Option<wf_types::script::sandbox::ShellPolicy>,
    shell_type: Option<wf_sandbox::strategy::shell::base::ShellType>,
}

impl CommandPolicy {
    pub fn new(allowed_commands: Vec<String>, denied_commands: Option<Vec<String>>) -> Self {
        Self {
            allowed_commands,
            denied_commands,
            shell_policy: None,
            shell_type: None,
        }
    }

    /// Default policy: the common development command allowlist and no deny
    /// list (aligned with the default `ShellToolConfig`).
    pub fn default_allowed() -> Self {
        Self::new(
            crate::config::DEFAULT_ALLOWED_COMMANDS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            None,
        )
    }

    /// Build a policy from the shell tool configuration. The sandbox shell
    /// rules (when a sandbox policy is attached) are carried into the unified
    /// pipeline so deny is a union and allow is the stricter side.
    pub fn from_config(config: &crate::config::ShellToolConfig) -> Self {
        Self {
            allowed_commands: config.allowed_commands.clone(),
            denied_commands: config.denied_commands.clone(),
            shell_policy: config.sandbox_policy.as_ref().and_then(|p| p.shell.clone()),
            shell_type: config
                .shell_type
                .as_ref()
                .and_then(|s| wf_sandbox::strategy::shell::base::ShellType::parse(s.as_str())),
        }
    }

    pub fn allowed_commands(&self) -> &[String] {
        &self.allowed_commands
    }

    pub fn denied_commands(&self) -> Option<&[String]> {
        self.denied_commands.as_deref()
    }

    /// Evaluate the command against the policy via the unified pipeline.
    pub fn decision(&self, command: &str) -> CommandDecision {
        let rules = CommandRules {
            allowed_commands: self.allowed_commands.clone(),
            denied_commands: self.denied_commands.clone().unwrap_or_default(),
        };
        let shell_type = self
            .shell_type
            .unwrap_or_else(wf_sandbox::strategy::shell::base::ShellType::default_for_platform);
        command_policy::evaluate_command(command, shell_type, &rules, self.shell_policy.as_ref())
    }

    /// Whether the command is hard-rejected by the policy.
    pub fn is_denied(&self, command: &str) -> bool {
        self.decision(command) == CommandDecision::AutoDeny
    }
}

pub fn get_single_command_decision(
    command: &str,
    allowed_commands: &[String],
    denied_commands: Option<&[String]>,
) -> CommandDecision {
    let rules = CommandRules {
        allowed_commands: allowed_commands.to_vec(),
        denied_commands: denied_commands.unwrap_or(&[]).to_vec(),
    };
    let shell_type = wf_sandbox::strategy::shell::base::ShellType::default_for_platform();
    command_policy::evaluate_command(command, shell_type, &rules, None)
}

pub fn get_command_decision(
    command: &str,
    allowed_commands: &[String],
    denied_commands: Option<&[String]>,
) -> CommandDecision {
    let command = command.trim();
    if command.is_empty() {
        return CommandDecision::AutoApprove;
    }

    if contains_dangerous_substitution(command) {
        return CommandDecision::AskUser;
    }

    get_single_command_decision(command, allowed_commands, denied_commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let result = parse_command_chain("git status");
        assert_eq!(result, vec!["git status"]);
    }

    #[test]
    fn test_parse_chain_and() {
        let result = parse_command_chain("git add . && git commit -m 'fix'");
        assert_eq!(result.len(), 2);
        assert!(result[0].contains("git add"));
        assert!(result[1].contains("git commit"));
    }

    #[test]
    fn test_parse_chain_pipe() {
        let result = parse_command_chain("ls | grep foo");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_dangerous_substitution() {
        assert!(contains_dangerous_substitution("echo ${USER@Q}"));
        assert!(contains_dangerous_substitution("cat ${!VAR}"));
        assert!(contains_dangerous_substitution("echo <<<$(whoami)"));
        assert!(!contains_dangerous_substitution("echo hello"));
        assert!(!contains_dangerous_substitution("ls -la"));
    }

    #[test]
    fn test_longest_prefix_match() {
        let prefixes = vec!["git".to_string(), "git commit".to_string()];
        let result = find_longest_prefix_match("git commit -m 'msg'", &prefixes);
        assert_eq!(result, Some("git commit".to_string()));
    }

    #[test]
    fn test_single_decision_allowlist() {
        let allowed = vec!["git".to_string()];
        assert_eq!(
            get_single_command_decision("git status", &allowed, None),
            CommandDecision::AutoApprove
        );
        assert_eq!(
            get_single_command_decision("rm -rf /", &allowed, None),
            CommandDecision::AskUser
        );
    }

    #[test]
    fn test_denylist_overrides() {
        let allowed = vec!["*".to_string()];
        let denied = vec!["rm".to_string()];
        assert_eq!(
            get_single_command_decision("rm -rf /", &allowed, Some(&denied)),
            CommandDecision::AutoDeny
        );
        assert_eq!(
            get_single_command_decision("ls", &allowed, Some(&denied)),
            CommandDecision::AutoApprove
        );
    }

    // Acceptance: `sudo rm -rf /` must be denied by a bare `rm` deny
    // rule (the legacy prefix path returned AutoApprove).
    #[test]
    fn test_sudo_wrapper_cannot_bypass_denylist() {
        let allowed = vec!["*".to_string()];
        let denied = vec!["rm".to_string()];
        assert_eq!(
            get_command_decision("sudo rm -rf /", &allowed, Some(&denied)),
            CommandDecision::AutoDeny
        );
    }

    // Acceptance: a bare command word allowlist must not match longer
    // command names via prefix (`gitx` was AutoApprove before the merge).
    #[test]
    fn test_allowlist_does_not_prefix_leak() {
        let allowed = vec!["git".to_string()];
        assert_eq!(
            get_command_decision("gitx", &allowed, None),
            CommandDecision::AskUser
        );
    }

    #[test]
    fn test_chain_decision() {
        let allowed = vec!["git".to_string()];
        let denied: Vec<String> = vec![];
        let result =
            get_command_decision("git add . && git commit -m 'fix'", &allowed, Some(&denied));
        assert_eq!(result, CommandDecision::AutoApprove);
    }

    #[test]
    fn test_chain_blocks_denied() {
        let allowed = vec!["git".to_string(), "rm".to_string()];
        let denied = vec!["rm -rf".to_string()];
        let result = get_command_decision("git checkout main && rm -rf /", &allowed, Some(&denied));
        assert_eq!(result, CommandDecision::AutoDeny);
    }

    // Acceptance: absolute paths resolve against the allowlist.
    #[test]
    fn test_absolute_path_allowed() {
        let allowed = vec!["git".to_string()];
        assert_eq!(
            get_command_decision("/usr/bin/git status", &allowed, None),
            CommandDecision::AutoApprove
        );
    }
}
