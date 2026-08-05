pub mod shell_detector;

use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::command_safety::{get_command_decision, CommandDecision};
use crate::error::{ToolError, ToolResult};
use crate::executor::stateless::StatelessAsyncHandler;
use crate::executor::trait_def::ToolExecutionContext;
use crate::shell::shell_detector::{default_shell_detector, resolve_shell_command, ShellType};

const DEFAULT_MAX_TIMEOUT_MS: u64 = 600_000;
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Default allowlist of common development/shell commands. Commands not on
/// this list are still executed but flagged (approval is an app-layer concern);
/// commands explicitly denied by a configured policy are rejected.
pub const DEFAULT_ALLOWED_COMMANDS: &[&str] = &[
    "git", "ls", "cat", "echo", "pwd", "mkdir", "touch", "cp", "mv", "rm", "grep", "find", "head",
    "tail", "wc", "sort", "uniq", "diff", "sed", "awk", "rg", "make", "cargo", "rustc", "node",
    "npm", "npx", "pnpm", "yarn", "python", "python3", "pip", "curl", "wget", "sh", "bash", "zsh",
];

#[derive(Debug, Clone)]
pub struct ShellToolConfig {
    pub workspace_dir: Option<PathBuf>,
    pub max_timeout_ms: u64,
    pub allowed_commands: Vec<String>,
    pub denied_commands: Option<Vec<String>>,
    /// Explicit shell override. When `None`, the platform default is detected
    /// via `$SHELL` / `which`.
    pub shell_type: Option<ShellType>,
}

impl Default for ShellToolConfig {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            max_timeout_ms: DEFAULT_MAX_TIMEOUT_MS,
            allowed_commands: DEFAULT_ALLOWED_COMMANDS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            denied_commands: None,
            shell_type: None,
        }
    }
}

/// Create the async handler for the execute_command tool.
pub fn execute_command_handler(config: ShellToolConfig) -> StatelessAsyncHandler {
    Arc::new(move |parameters: Value, _ctx: ToolExecutionContext| {
        let config = config.clone();
        Box::pin(async move {
            let command = parameters
                .get("command")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ToolError::ValidationFailed(
                        "Missing or invalid 'command' parameter".to_string(),
                    )
                })?
                .to_string();

            // Shell policy check: deny is hard, ask/approve proceed at this layer.
            let decision = get_command_decision(
                &command,
                &config.allowed_commands,
                config.denied_commands.as_deref(),
            );
            if decision == CommandDecision::AutoDeny {
                return Err(ToolError::ExecutionError(format!(
                    "Command rejected by shell policy: {}",
                    command
                )));
            }

            let timeout_ms = parameters
                .get("timeout")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_TIMEOUT_MS)
                .clamp(1000, config.max_timeout_ms);

            let cwd = parameters
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    config
                        .workspace_dir
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                });

            let start = Instant::now();
            let output =
                run_command(&command, cwd.as_deref(), timeout_ms, config.shell_type).await?;

            let mut content = String::from_utf8_lossy(&output.stdout).to_string();
            if !output.stderr.is_empty() {
                content.push_str("\n[stderr]:\n");
                content.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            if content.trim().is_empty() {
                content = "(no output)".into();
            }

            let mut result = serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                "exit_code": output.status.code(),
                "duration_ms": start.elapsed().as_millis(),
            });
            if !output.status.success() {
                result["success"] = Value::Bool(false);
                result["error"] = Value::String(format!(
                    "Command failed with exit code {:?}",
                    output.status.code()
                ));
            } else {
                result["success"] = Value::Bool(true);
            }

            Ok(serde_json::json!({
                "content": content,
                "details": result,
            }))
        })
    })
}

async fn run_command(
    command: &str,
    cwd: Option<&str>,
    timeout_ms: u64,
    shell_type: Option<ShellType>,
) -> ToolResult<std::process::Output> {
    let (shell, shell_args) = resolve_shell_command(default_shell_detector(), shell_type, command);
    let mut cmd = tokio::process::Command::new(&shell);
    cmd.args(&shell_args)
        .current_dir(cwd.unwrap_or("."))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| {
        ToolError::ExecutionError(format!(
            "Failed to spawn command with shell '{}': {}",
            shell, e
        ))
    })?;

    match tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(ToolError::ExecutionError(format!(
            "Failed to run command: {}",
            e
        ))),
        Err(_) => Err(ToolError::ExecutionError(format!(
            "Command timed out after {} seconds",
            timeout_ms / 1000
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_command_echo() {
        let handler = execute_command_handler(ShellToolConfig::default());
        let ctx = ToolExecutionContext::new("exec-1".into());
        let result = handler(serde_json::json!({ "command": "echo hello" }), ctx)
            .await
            .unwrap();
        assert_eq!(result["details"]["success"], Value::Bool(true));
        assert!(result["content"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn test_execute_command_failure() {
        let handler = execute_command_handler(ShellToolConfig::default());
        let ctx = ToolExecutionContext::new("exec-1".into());
        let result = handler(serde_json::json!({ "command": "exit 3" }), ctx)
            .await
            .unwrap();
        assert_eq!(result["details"]["success"], Value::Bool(false));
        assert_eq!(result["details"]["exit_code"], Value::from(3));
    }

    #[tokio::test]
    async fn test_execute_command_missing_param() {
        let handler = execute_command_handler(ShellToolConfig::default());
        let ctx = ToolExecutionContext::new("exec-1".into());
        let result = handler(serde_json::json!({}), ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_command_denied() {
        let config = ShellToolConfig {
            denied_commands: Some(vec!["danger".into()]),
            ..Default::default()
        };
        let handler = execute_command_handler(config);
        let ctx = ToolExecutionContext::new("exec-1".into());
        let result = handler(serde_json::json!({ "command": "danger --all" }), ctx).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("rejected by shell policy"));
    }

    #[tokio::test]
    async fn test_execute_command_timeout() {
        let config = ShellToolConfig {
            max_timeout_ms: 2000,
            ..Default::default()
        };
        let handler = execute_command_handler(config);
        let ctx = ToolExecutionContext::new("exec-1".into());
        let result = handler(
            serde_json::json!({ "command": "sleep 10", "timeout": 200 }),
            ctx,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }
}
