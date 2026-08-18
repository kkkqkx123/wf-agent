use regex::Regex;
use std::collections::HashMap;
use wf_types::script::sandbox::ShellPolicy;

use crate::command_policy::{CommandRule, Severity};

use super::base::{ShellAnalysisContext, ShellAnalysisResult, ShellAnalyzer, ShellType};

const SHELL_TYPE: ShellType = ShellType::PowerShell;

/// Default PowerShell blacklist, also used by the unified command policy.
pub(crate) const DENIED_COMMANDS: &[&str] = &[
    "Start-Process",
    "Stop-Process",
    "Get-WmiObject",
    "Get-WinEvent",
    "Invoke-WmiMethod",
    "Register-WmiEvent",
    "Remove-WmiObject",
    "Set-WmiInstance",
    "Invoke-Expression",
    "Invoke-Command",
    "Invoke-CimMethod",
    "Invoke-WebRequest",
    "Invoke-RestMethod",
    "Enter-PSSession",
    "Exit-PSSession",
    "New-PSSession",
    "Remove-PSSession",
    "Set-ExecutionPolicy",
    "Set-MpPreference",
    "Unblock-File",
    "New-Service",
    "Set-Service",
    "Restart-Service",
    "Stop-Service",
    "New-ItemProperty",
    "Set-ItemProperty",
    "Remove-ItemProperty",
];

/// Default PowerShell dangerous rules (addressable ids + severity grades).
pub const DANGEROUS_PATTERNS: &[CommandRule] = &[
    CommandRule { id: "core.powershell:iex-download", pack: "core.scripting", pattern: r"IEX\s*\(?\s*(New-Object|Invoke-WebRequest|Invoke-RestMethod)", severity: Severity::Critical },
    CommandRule { id: "core.powershell:iex-download", pack: "core.scripting", pattern: r"Invoke-Expression\s*\(?\s*(New-Object|Invoke-WebRequest)", severity: Severity::Critical },
    CommandRule { id: "core.powershell:encoded-command", pack: "core.scripting", pattern: r"-EncodedCommand\s+", severity: Severity::Critical },
    CommandRule { id: "core.powershell:short-encoded", pack: "core.scripting", pattern: r"-e\s+[A-Za-z0-9+/=]{20,}", severity: Severity::High },
    CommandRule { id: "core.powershell:from-base64", pack: "core.scripting", pattern: r"\[System\.Convert\]::FromBase64String", severity: Severity::High },
    CommandRule { id: "core.powershell:webclient", pack: "core.network", pattern: r"New-Object\s+Net\.WebClient", severity: Severity::High },
    CommandRule { id: "core.powershell:webclient", pack: "core.network", pattern: r"New-Object\s+System\.Net\.WebClient", severity: Severity::High },
    CommandRule { id: "core.powershell:download-string", pack: "core.network", pattern: r#"\.DownloadString\(\s*(['"]?)https?://"#, severity: Severity::High },
    CommandRule { id: "core.powershell:download-file", pack: "core.network", pattern: r#"\.DownloadFile\(\s*(['"]?)https?://"#, severity: Severity::High },
    CommandRule { id: "core.powershell:amsi-utils", pack: "core.av-evasion", pattern: r"AmsiUtils", severity: Severity::Critical },
    CommandRule { id: "core.powershell:amsi-init", pack: "core.av-evasion", pattern: r"amsiInitFailed", severity: Severity::Critical },
    CommandRule { id: "core.powershell:amsi-utils", pack: "core.av-evasion", pattern: r"System\.Management\.Automation\.AmsiUtils", severity: Severity::Critical },
    CommandRule { id: "core.powershell:reflection-load", pack: "core.av-evasion", pattern: r"\[Ref\].*Assembly.*Load.*System\.Management\.Automation", severity: Severity::High },
    CommandRule { id: "core.powershell:amsi-getfield", pack: "core.av-evasion", pattern: r#"GetField\s*\(\s*['"]amsi"#, severity: Severity::Critical },
    CommandRule { id: "core.powershell:amsi-setvalue", pack: "core.av-evasion", pattern: r"SetValue\s*\(\s*null", severity: Severity::Critical },
    CommandRule { id: "core.powershell:com-object", pack: "core.scripting", pattern: r"New-Object\s+-ComObject\s+", severity: Severity::Medium },
    CommandRule { id: "core.powershell:wscript-shell", pack: "core.scripting", pattern: r#"CreateObject\s*\(\s*['"]WScript\.Shell"#, severity: Severity::High },
    CommandRule { id: "core.powershell:open-process-token", pack: "core.privilege", pattern: r"Advapi32\..*OpenProcessToken", severity: Severity::High },
    CommandRule { id: "core.powershell:duplicate-token", pack: "core.privilege", pattern: r"Advapi32\..*DuplicateToken", severity: Severity::High },
    CommandRule { id: "core.powershell:virtual-alloc", pack: "core.memory", pattern: r"Kernel32\..*VirtualAlloc", severity: Severity::High },
    CommandRule { id: "core.powershell:create-thread", pack: "core.memory", pattern: r"Kernel32\..*CreateThread", severity: Severity::High },
    CommandRule { id: "core.powershell:create-process", pack: "core.process", pattern: r"Kernel32\..*CreateProcess", severity: Severity::High },
];

/// Alias resolution table, shared with the unified command policy
/// (`crate::command_policy::primary_command`).
pub(crate) fn build_alias_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("iex", "Invoke-Expression"),
        ("iwr", "Invoke-WebRequest"),
        ("irm", "Invoke-RestMethod"),
        ("icm", "Invoke-Command"),
        ("saps", "Start-Process"),
        ("gcm", "Get-Command"),
        ("gm", "Get-Member"),
        ("gi", "Get-Item"),
        ("gci", "Get-ChildItem"),
        ("gl", "Get-Location"),
        ("gp", "Get-ItemProperty"),
        ("gsv", "Get-Service"),
        ("gwmi", "Get-WmiObject"),
        ("ni", "New-Item"),
        ("nv", "New-Variable"),
        ("ogv", "Out-GridView"),
        ("oh", "Out-Host"),
        ("r", "Invoke-History"),
        ("rc", "Set-PSReadLineOption"),
        ("rm", "Remove-Item"),
        ("rmdir", "Remove-Item"),
        ("sasv", "Start-Service"),
        ("shcm", "Show-Command"),
        ("sls", "Select-String"),
        ("sp", "Set-ItemProperty"),
        ("spsv", "Stop-Service"),
        ("sv", "Set-Variable"),
        ("tee", "Tee-Object"),
        ("type", "Get-Content"),
        ("wi", "Write-Output"),
        ("write", "Write-Output"),
    ])
}

pub struct PowerShellAnalyzer;

struct ResolvedShellPolicy {
    /// (regex, severity) pairs resolved from user patterns or the built-in
    /// rule table.
    dangerous_patterns: Vec<(String, Severity)>,
    allow_pipe: bool,
    allow_redirect: bool,
}

impl PowerShellAnalyzer {
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

impl ShellAnalyzer for PowerShellAnalyzer {
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

    fn analyzer() -> PowerShellAnalyzer {
        PowerShellAnalyzer
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
    fn test_powershell_analyzer_shell_type() {
        assert_eq!(analyzer().shell_type(), ShellType::PowerShell);
    }

    #[test]
    fn test_allows_safe() {
        let result = analyze("Get-ChildItem", &empty_policy());
        assert!(result.allowed);
    }

    #[test]
    fn test_denies_encoded_command() {
        let result = analyze("powershell -EncodedCommand SQBFAFgAIAAoAE4AZQB3AC0ATwBiAGoAZQBjAHQAIABOAGUAdAAuAFcAZQBiAEMAbABpAGUAbgB0ACkA", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_amsi_bypass() {
        let result = analyze(
            "[Ref].Assembly.GetType('System.Management.Automation.AmsiUtils')",
            &empty_policy(),
        );
        assert!(!result.allowed);
    }

    #[test]
    fn test_variable_assignment_stripped() {
        let result = analyze("$x = Get-ChildItem", &empty_policy());
        assert!(result.allowed);
    }

    #[test]
    fn test_empty_command() {
        let result = analyze("", &empty_policy());
        assert!(!result.allowed);
    }
}
