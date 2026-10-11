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
    Some(wf_types::effect::normalize_effect_path(&absolute))
}

fn scope_allows_violation(mode: Option<&wf_types::script::sandbox::SandboxMode>) -> bool {
    use wf_types::script::sandbox::SandboxMode;
    matches!(
        mode,
        Some(SandboxMode::Lenient) | Some(SandboxMode::Disabled)
    )
}

fn scope_violation(
    message: String,
    mode: Option<&wf_types::script::sandbox::SandboxMode>,
    execution: &str,
) -> Result<(), ToolError> {
    if scope_allows_violation(mode) {
        tracing::warn!(
            execution = %execution,
            error = %message,
            "shell scope violation allowed by lenient mode; checkpoint will not endorse the write"
        );
        return Ok(());
    }
    Err(ToolError::ExecutionError(message))
}

fn ensure_shell_path_in_scope(
    path: &std::path::Path,
    scope: &wf_types::config::file_scope::ResolvedFileScope,
    want_write: bool,
    execution: &str,
    mode: Option<&wf_types::script::sandbox::SandboxMode>,
) -> Result<(), ToolError> {
    use wf_types::config::file_scope::{normalize_scope_path, scope_path_contains};
    if !path.is_absolute() {
        tracing::warn!(
            execution = %execution,
            path = %path.display(),
            "shell scope violation: relative command path without base"
        );
        return scope_violation(
            format!(
                "command path '{}' outside allowed {} prefixes",
                path.display(),
                if want_write { "write" } else { "read" }
            ),
            mode,
            execution,
        );
    }
    let lexical = normalize_scope_path(&path.to_string_lossy());
    let bounds = if want_write {
        &scope.write_prefixes
    } else {
        &scope.read_prefixes
    };
    if !bounds
        .iter()
        .any(|bound| scope_path_contains(bound, &lexical))
    {
        tracing::warn!(
            execution = %execution,
            path = %path.display(),
            want_write = want_write,
            "shell scope violation: command path outside allowed prefixes"
        );
        return scope_violation(
            format!(
                "command path '{}' outside allowed {} prefixes",
                path.display(),
                if want_write { "write" } else { "read" }
            ),
            mode,
            execution,
        );
    }
    let physical = path.canonicalize().ok().or_else(|| {
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .and_then(|parent| parent.canonicalize().ok())
            .and_then(|pp| path.file_name().map(|name| pp.join(name)))
    });
    if let Some(physical) = physical {
        let physical_str = physical.to_string_lossy().to_string();
        if !bounds
            .iter()
            .any(|bound| scope_path_contains(bound, &physical_str))
        {
            tracing::warn!(
                execution = %execution,
                path = %path.display(),
                physical = %physical.display(),
                "shell scope violation: command path escapes through symlink"
            );
            return scope_violation(
                format!(
                    "command path '{}' escapes allowed {} prefixes through symlink",
                    path.display(),
                    if want_write { "write" } else { "read" }
                ),
                mode,
                execution,
            );
        }
    }
    Ok(())
}

fn resolve_command_path(
    raw: &str,
    scope_dir: Option<&std::path::PathBuf>,
    default_dir: Option<&std::path::PathBuf>,
) -> std::path::PathBuf {
    let candidate = std::path::Path::new(raw);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else if let Some(base) = scope_dir.or(default_dir) {
        base.join(candidate)
    } else {
        std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(candidate))
            .unwrap_or_else(|| candidate.to_path_buf())
    }
}

fn check_shell_scope(
    scope_dir: Option<&std::path::PathBuf>,
    command: &str,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
    default_dir: Option<&std::path::PathBuf>,
    execution: &str,
    mode: Option<&wf_types::script::sandbox::SandboxMode>,
) -> Result<(), ToolError> {
    let Some(scope) = scope else {
        return Ok(());
    };
    if !scope.enabled {
        return Ok(());
    }
    let Some(dir) = scope_dir else {
        tracing::warn!(
            execution = %execution,
            "shell scope violation: missing startup directory"
        );
        return scope_violation(
            "startup directory outside allowed prefixes: no working directory".to_string(),
            mode,
            execution,
        );
    };
    let inside_read_or_write = scope
        .read_prefixes
        .iter()
        .chain(scope.write_prefixes.iter())
        .any(|bound| {
            wf_types::config::file_scope::scope_path_contains(
                bound,
                &wf_types::config::file_scope::normalize_scope_path(&dir.to_string_lossy()),
            )
        });
    if !inside_read_or_write {
        tracing::warn!(
            execution = %execution,
            path = %dir.display(),
            "shell scope violation: startup directory outside allowed prefixes"
        );
        return scope_violation(
            format!(
                "startup directory '{}' outside allowed prefixes",
                dir.display()
            ),
            mode,
            execution,
        );
    }
    ensure_shell_path_in_scope(dir, scope, false, execution, mode)
        .or_else(|_| ensure_shell_path_in_scope(dir, scope, true, execution, mode))?;
    let chains = wf_sandbox::parse_command_chain(command);
    for chain in &chains {
        let tokens = chain
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let (reads, writes) = wf_sandbox::extract_file_paths(&tokens);
        for raw in &writes {
            let resolved = resolve_command_path(raw, Some(dir), default_dir);
            ensure_shell_path_in_scope(&resolved, scope, true, execution, mode)?;
        }
        for raw in &reads {
            let resolved = resolve_command_path(raw, Some(dir), default_dir);
            ensure_shell_path_in_scope(&resolved, scope, false, execution, mode)?;
        }
    }
    Ok(())
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
            let effective_default = ctx
                .file_scope
                .as_ref()
                .and_then(|s| s.workdir.as_ref().map(std::path::PathBuf::from))
                .or_else(|| config.workspace_dir.clone());
            let cwd = parameters
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    effective_default
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                });
            let scope_dir = resolve_scope_dir(cwd.as_deref(), effective_default.as_ref());
            let scope_mode = config
                .sandbox_policy
                .as_ref()
                .and_then(|policy| policy.mode.as_ref());
            check_shell_scope(
                scope_dir.as_ref(),
                &command,
                ctx.file_scope.as_ref(),
                effective_default.as_ref(),
                &ctx.execution_id.to_string(),
                scope_mode,
            )?;

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

            let effective_sandbox = ctx.file_scope.as_ref().map(|scope| {
                let mut policy = config.sandbox_policy.clone().unwrap_or(
                    wf_types::script::sandbox::SandboxPolicy {
                        mode: Some(wf_types::script::sandbox::SandboxMode::Strict),
                        shell: None,
                        python: None,
                        javascript: None,
                        lua: None,
                        filesystem: None,
                        process: None,
                        network: None,
                        resource: None,
                    },
                );
                let fs = policy
                    .filesystem
                    .get_or_insert(wf_types::script::sandbox::FilesystemPolicy::default());
                fs.allowed_read_paths = Some(scope.read_prefixes.clone());
                fs.allowed_write_paths = Some(scope.write_prefixes.clone());
                fs.allowed_remove_paths = Some(scope.write_prefixes.clone());
                policy
            });
            let sandbox_ref = effective_sandbox
                .as_ref()
                .or(config.sandbox_policy.as_ref());
            let start = Instant::now();
            let output = match run_command(
                &command,
                cwd.as_deref(),
                timeout_ms,
                config.shell_type,
                input,
                sandbox_ref,
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
                                    wf_types::effect::ScopeOutcome {
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
                            wf_types::effect::ScopeOutcome {
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
