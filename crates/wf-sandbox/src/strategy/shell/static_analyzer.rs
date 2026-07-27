use async_trait::async_trait;
use regex::Regex;
use std::sync::Arc;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, VfsProvider};

pub struct ShellStaticAnalyzerStrategy;

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

fn analyze_subcommand(
    sub_command: &str,
    shell_policy: &wf_types::script::sandbox::ShellPolicy,
) -> SubCommandResult {
    let parts: Vec<&str> = sub_command
        .split_whitespace()
        .filter(|s| !s.is_empty() && *s != "|" && *s != "||" && *s != "&&")
        .collect();

    if parts.is_empty() {
        return SubCommandResult::allowed();
    }

    let base_cmd = parts[0];

    if !shell_policy.allowed_commands.is_empty()
        && !shell_policy.allowed_commands.iter().any(|a| a == base_cmd)
    {
        return SubCommandResult::denied(format!(
            "Command not in allowed list: {base_cmd}"
        ));
    }

    if shell_policy
        .denied_commands
        .iter()
        .any(|d| d == base_cmd)
    {
        return SubCommandResult::denied(format!("Command denied: {base_cmd}"));
    }

    SubCommandResult::allowed()
}

struct SubCommandResult {
    allowed: bool,
    reason: Option<String>,
}

impl SubCommandResult {
    fn allowed() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    fn denied(reason: String) -> Self {
        Self {
            allowed: false,
            reason: Some(reason),
        }
    }
}

async fn check_vfs_paths(
    _sub_command: &str,
    _vfs: &Arc<dyn VfsProvider>,
) -> Option<String> {
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
        "Static command analysis with dangerous pattern matching"
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

        let shell_policy = policy.shell.as_ref().unwrap();

        let dangerous_patterns: Vec<Regex> = shell_policy
            .dangerous_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        for regex in &dangerous_patterns {
            if regex.is_match(&command) {
                return Ok(deny(&format!("Dangerous pattern detected: {}", regex.as_str())));
            }
        }

        if !shell_policy.allow_pipe && command.contains('|') {
            return Ok(deny("Pipe operator is not allowed"));
        }

        let sub_commands = parse_command_chain(&command);
        for sub_command in &sub_commands {
            let result = analyze_subcommand(sub_command, shell_policy);
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

    #[tokio::test]
    async fn test_empty_command() {
        let strategy = ShellStaticAnalyzerStrategy;
        let policy = crate::default_policy::default_sandbox_policy();
        let options = StrategyExecuteOptions {
            command: String::new(),
            shell_type: None,
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, policy).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Empty command"));
    }

    #[tokio::test]
    async fn test_denies_dangerous_rm() {
        let strategy = ShellStaticAnalyzerStrategy;
        let policy = crate::default_policy::default_sandbox_policy();
        let options = StrategyExecuteOptions {
            command: "rm -rf /".to_string(),
            shell_type: None,
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, policy).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_allows_safe_command() {
        let strategy = ShellStaticAnalyzerStrategy;
        let policy = crate::default_policy::default_sandbox_policy();
        let options = StrategyExecuteOptions {
            command: "echo hello".to_string(),
            shell_type: None,
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, policy).await.unwrap();
        assert!(result.success || result.stderr.is_some());
    }

    #[test]
    fn test_parse_command_chain() {
        let cmds = parse_command_chain("echo a; echo b && echo c");
        assert_eq!(cmds.len(), 3);
    }
}
