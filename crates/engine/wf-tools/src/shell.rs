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

/// Resolve the observer scope dir from the final cwd string: absolute
/// paths are used as-is, relative paths resolve against the tool default
/// workspace dir (or the process cwd when no default exists). Returns
/// `None` when no cwd information exists at all.
fn resolve_scope_dir(
    cwd: Option<&str>,
    default_dir: Option<&std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let cwd = cwd.filter(|s| !s.trim().is_empty())?;
    let candidate = std::path::Path::new(cwd);
    let absolute = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else if let Some(base) = default_dir {
        base.join(candidate)
    } else {
        std::env::current_dir().ok()?.join(candidate)
    };
    Some(wf_checkpoint::normalize_effect_path(&absolute))
}

/// Create the async handler for the execute_command tool.
///
/// The final working directory follows the `parameters.cwd` first,
/// `ShellToolConfig.workspace_dir` default rule; it is passed to the
/// side-effect observer so the upper layer can diff the exact scope. The
/// scope end is emitted after the process has terminated, including for
/// failed commands (already-written files are still captured). Shell
/// startup failures end the scope as terminated with success=false; a
/// timeout that cannot confirm termination must end as incomplete.
pub fn execute_command_handler(config: ShellToolConfig) -> StatelessAsyncHandler {
    // Single policy instance shared across calls so the stateless path uses
    // the same decision logic as the engine-level spawn baseline.
    let policy = CommandPolicy::from_config(&config);
    Arc::new(move |parameters: Value, ctx: ToolExecutionContext| {
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

            // Final cwd: explicit `parameters.cwd` wins, otherwise the tool
            // default workspace dir. Preserved for the observer input.
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
            let scope_dir = resolve_scope_dir(cwd.as_deref(), config.workspace_dir.as_ref());

            let input = parameters.get("input").and_then(|v| v.as_str());

            let execution_id = ctx.execution_id.to_string();
            if let Some(scope) = scope_dir.as_ref() {
                if let Some(session) = ctx.checkpoint_session.as_ref() {
                    // Blocking scan offloaded to the blocking pool.
                    session
                        .begin_scope_async(execution_id.clone(), scope.clone())
                        .await;
                }
            }

            let start = Instant::now();
            let output = match run_command(
                &command,
                cwd.as_deref(),
                timeout_ms,
                config.shell_type,
                input,
                config.sandbox_policy.as_ref(),
            )
            .await
            {
                Ok(output) => output,
                Err(err) => {
                    // `run_command` enforces the timeout with process
                    // termination, so the sampling boundary is the confirmed
                    // process end; startup failures also terminate (nothing
                    // was started). Both end as terminated with success=false
                    // so already-written files are still captured.
                    if let Some(scope) = scope_dir.as_ref() {
                        if let Some(_sess) = ctx.checkpoint_session.as_ref() {
                            _sess
                                .end_scope_async(
                                    scope.clone(),
                                    wf_checkpoint::ScopeOutcome {
                                        execution_id: execution_id.clone(),
                                        success: false,
                                        terminated: true,
                                        detail: Some(err.to_string()),
                                    },
                                )
                                .await;
                        }
                    }
                    return Err(err.into());
                }
            };

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
            let success = output.status.success();
            if !success {
                result["success"] = Value::Bool(false);
                result["error"] = Value::String(format!(
                    "Command failed with exit code {:?}",
                    output.status.code()
                ));
            } else {
                result["success"] = Value::Bool(true);
            }

            // Sampling ends after the process terminated, even for failed
            // commands: already-written files are still captured.
            if let Some(scope) = scope_dir.as_ref() {
                if let Some(_sess) = ctx.checkpoint_session.as_ref() {
                    _sess
                        .end_scope_async(
                            scope.clone(),
                            wf_checkpoint::ScopeOutcome {
                                execution_id: execution_id.clone(),
                                success,
                                terminated: true,
                                detail: Some(format!("exit {:?}", output.status.code())),
                            },
                        )
                        .await;
                }
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

    #[test]
    fn scope_dir_resolution_rules() {
        use std::path::PathBuf;
        // Explicit absolute cwd wins.
        assert_eq!(
            resolve_scope_dir(Some("/ws/sub"), Some(&PathBuf::from("/ws"))),
            Some(PathBuf::from("/ws/sub"))
        );
        // Relative cwd resolves against the tool default.
        assert_eq!(
            resolve_scope_dir(Some("sub"), Some(&PathBuf::from("/ws"))),
            Some(PathBuf::from("/ws/sub"))
        );
        // No cwd at all means no scope.
        assert_eq!(resolve_scope_dir(None, Some(&PathBuf::from("/ws"))), None);
    }

    #[tokio::test]
    async fn scoped_shell_runs_without_session() {
        let dir = tempfile::tempdir().unwrap();
        let config = ShellToolConfig {
            workspace_dir: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        let handler = execute_command_handler(config);
        let ctx = ToolExecutionContext::new("exec-shell".into());
        let result = handler(
            serde_json::json!({ "command": "echo hi", "cwd": dir.path().to_str().unwrap() }),
            ctx,
        )
        .await
        .unwrap();
        assert_eq!(result["details"]["success"], serde_json::Value::Bool(true));
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
