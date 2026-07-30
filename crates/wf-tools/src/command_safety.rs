use regex::Regex;

use std::sync::OnceLock;

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

fn fd_redirect_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d*>&\d*").unwrap())
}

pub fn parse_command_chain(command: &str) -> Vec<String> {
    let operators = &["&&", "||", ";", "|", "&"];
    let mut result = vec![command.to_string()];

    for op in operators {
        result = result
            .iter()
            .flat_map(|cmd| cmd.split(op).map(|c| c.trim().to_string()))
            .collect();
    }

    result.retain(|c| !c.is_empty());
    result
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
            && command[pos..].trim_end().ends_with(')') {
                return true;
            }
    }

    false
}

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

#[derive(Debug, Clone, PartialEq)]
pub enum CommandDecision {
    AutoApprove,
    AutoDeny,
    AskUser,
}

pub fn get_single_command_decision(
    command: &str,
    allowed_commands: &[String],
    denied_commands: Option<&[String]>,
) -> CommandDecision {
    if command.is_empty() {
        return CommandDecision::AutoApprove;
    }

    if allowed_commands.is_empty() {
        return CommandDecision::AskUser;
    }

    let has_wildcard = allowed_commands.iter().any(|c| c.to_lowercase() == "*");

    match denied_commands {
        None => {
            let trimmed = command.trim().to_lowercase();
            let has_match = allowed_commands.iter().any(|prefix| {
                let lower = prefix.to_lowercase();
                lower == "*" || trimmed.starts_with(&lower)
            });
            if has_match {
                CommandDecision::AutoApprove
            } else {
                CommandDecision::AskUser
            }
        }
        Some(denied) => {
            let longest_denied = find_longest_prefix_match(command, denied);
            let longest_allowed = find_longest_prefix_match(command, allowed_commands);

            if has_wildcard && longest_denied.is_none() {
                return CommandDecision::AutoApprove;
            }

            match (&longest_allowed, &longest_denied) {
                (None, Some(_)) => CommandDecision::AutoDeny,
                (None, None) => CommandDecision::AskUser,
                (Some(_), None) => CommandDecision::AutoApprove,
                (Some(allow), Some(deny)) => {
                    if allow.len() > deny.len() {
                        CommandDecision::AutoApprove
                    } else {
                        CommandDecision::AutoDeny
                    }
                }
            }
        }
    }
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

    let sub_commands = parse_command_chain(command);

    let decisions: Vec<CommandDecision> = sub_commands
        .iter()
        .map(|cmd| {
            let cleaned = fd_redirect_re()
                .replace(cmd.trim(), "")
                .to_string();
            let cleaned = cleaned.trim().to_string();
            get_single_command_decision(&cleaned, allowed_commands, denied_commands)
        })
        .collect();

    if decisions.contains(&CommandDecision::AutoDeny) {
        return CommandDecision::AutoDeny;
    }

    if decisions.iter().all(|d| *d == CommandDecision::AutoApprove) {
        return CommandDecision::AutoApprove;
    }

    CommandDecision::AskUser
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

    #[test]
    fn test_chain_decision() {
        let allowed = vec!["git".to_string()];
        let denied: Vec<String> = vec![];
        let result = get_command_decision(
            "git add . && git commit -m 'fix'",
            &allowed,
            Some(&denied),
        );
        assert_eq!(result, CommandDecision::AutoApprove);
    }

    #[test]
    fn test_chain_blocks_denied() {
        let allowed = vec!["git".to_string(), "rm".to_string()];
        let denied = vec!["rm -rf".to_string()];
        let result = get_command_decision(
            "git checkout main && rm -rf /",
            &allowed,
            Some(&denied),
        );
        assert_eq!(result, CommandDecision::AutoDeny);
    }
}
