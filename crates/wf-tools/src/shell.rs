//! Stateless execute_command tool handler.
//!
//! The shell configuration, detection, runner and session engine live in
//! `wf-shell`; this module only wires the handler used by the tool registry.

use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

use wf_shell::command_safety::{CommandDecision, CommandPolicy};
use wf_shell::config::{ShellToolConfig, DEFAULT_TIMEOUT_MS};
use wf_shell::runner::run_command;

use crate::error::ToolError;
use crate::executor::stateless::StatelessAsyncHandler;
use crate::executor::trait_def::ToolExecutionContext;

/// Create the async handler for the execute_command tool.
pub fn execute_command_handler(config: ShellToolConfig) -> StatelessAsyncHandler {
    // Single policy instance shared across calls so the stateless path uses
    // the same decision logic as the engine-level spawn baseline.
    let policy = CommandPolicy::from_config(&config);
    Arc::new(move |parameters: Value, _ctx: ToolExecutionContext| {
        let config = config.clone();
        let policy = policy.clone();
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
            if policy.decision(&command) == CommandDecision::AutoDeny {
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

            let input = parameters.get("input").and_then(|v| v.as_str());

            let start = Instant::now();
            let output = run_command(
                &command,
                cwd.as_deref(),
                timeout_ms,
                config.shell_type,
                input,
            )
            .await?;

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
    async fn test_execute_command_with_input() {
        let handler = execute_command_handler(ShellToolConfig::default());
        let ctx = ToolExecutionContext::new("exec-1".into());
        let result = handler(
            serde_json::json!({ "command": "cat", "input": "hello-cat" }),
            ctx,
        )
        .await
        .unwrap();
        assert_eq!(result["details"]["success"], Value::Bool(true));
        assert!(result["content"].as_str().unwrap().contains("hello-cat"));
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
