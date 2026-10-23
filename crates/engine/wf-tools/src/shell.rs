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

use crate::error::{ToolError, ToolResult};
use crate::executor::stateless::StatelessAsyncHandler;
use crate::executor::trait_def::ToolExecutionContext;

/// Resolve the observer scope dir from the final cwd string: absolute
/// paths are used as-is, relative paths resolve against the tool default
/// workspace dir (or the process cwd when no default exists). Returns
/// `None` when no cwd information exists at all.
pub(crate) fn resolve_scope_dir(
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
    tool: &str,
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
    Err(ToolError::scope_violation(tool, message))
}

fn scope_guidance(scope: &wf_types::config::file_scope::ResolvedFileScope, kind: &str) -> String {
    match scope.workdir.as_deref() {
        Some(workdir) => format!(
            " Work inside '{workdir}' and keep paths under the allowed {kind} prefixes of this execution."
        ),
        None => format!(
            " Keep paths under the allowed {kind} prefixes of this execution."
        ),
    }
}

fn ensure_shell_path_in_scope(
    tool: &str,
    path: &std::path::Path,
    scope: &wf_types::config::file_scope::ResolvedFileScope,
    want_write: bool,
    execution: &str,
    mode: Option<&wf_types::script::sandbox::SandboxMode>,
) -> Result<(), ToolError> {
    use crate::scope_gate::{check_path_bounds, PathVerdict};
    // Writes stay fail-closed in every mode: a lenient write would execute
    // while sampling drops it as out of scope, leaving an unrestorable
    // side effect behind a success message.
    let effective_mode = if want_write { None } else { mode };
    let kind = if want_write { "write" } else { "read" };
    if !path.is_absolute() {
        tracing::warn!(
            execution = %execution,
            path = %path.display(),
            "shell scope violation: relative command path without base"
        );
        return scope_violation(
            tool,
            format!(
                "command path '{}' outside allowed {} prefixes: relative command path without base.{}",
                path.display(),
                kind,
                scope_guidance(scope, kind)
            ),
            effective_mode,
            execution,
        );
    }
    let bounds = if want_write {
        &scope.write_prefixes
    } else {
        &scope.read_prefixes
    };
    match check_path_bounds(path, bounds) {
        PathVerdict::Inside(_) => Ok(()),
        PathVerdict::Outside(_) => {
            tracing::warn!(
                execution = %execution,
                path = %path.display(),
                want_write = want_write,
                "shell scope violation: command path outside allowed prefixes"
            );
            scope_violation(
                tool,
                format!(
                    "command path '{}' outside allowed {} prefixes.{}",
                    path.display(),
                    kind,
                    scope_guidance(scope, kind)
                ),
                effective_mode,
                execution,
            )
        }
        PathVerdict::Escaped(physical) => {
            tracing::warn!(
                execution = %execution,
                path = %path.display(),
                physical = %physical.display(),
                "shell scope violation: command path escapes through symlink"
            );
            scope_violation(
                tool,
                format!(
                    "command path '{}' escapes allowed {} prefixes through symlink.{}",
                    path.display(),
                    kind,
                    scope_guidance(scope, kind)
                ),
                effective_mode,
                execution,
            )
        }
    }
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

pub(crate) fn check_shell_scope(
    tool: &str,
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
        let chains = wf_sandbox::parse_command_chain(command);
        for chain in &chains {
            let tokens = wf_sandbox::strategy::shell::vfs_paths::tokenize_command(chain);
            let (_, writes) = wf_sandbox::extract_file_paths(&tokens);
            if !writes.is_empty() {
                tracing::warn!(
                    execution = %execution,
                    "shell scope violation: write with closed file channel"
                );
                return scope_violation(
                    tool,
                    "command cannot run: the file channel is closed for this execution and writes are denied.".to_string(),
                    None,
                    execution,
                );
            }
        }
        return Ok(());
    }
    let Some(dir) = scope_dir else {
        tracing::warn!(
            execution = %execution,
            "shell scope violation: missing startup directory"
        );
        return scope_violation(
            tool,
            "startup directory outside allowed prefixes: no working directory. Set an explicit 'cwd' inside the execution workdir.".to_string(),
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
            tool,
            format!(
                "startup directory '{}' outside allowed prefixes.{}",
                dir.display(),
                scope_guidance(scope, "read")
            ),
            mode,
            execution,
        );
    }
    ensure_shell_path_in_scope(tool, dir, scope, false, execution, mode)
        .or_else(|_| ensure_shell_path_in_scope(tool, dir, scope, true, execution, mode))?;
    let chains = wf_sandbox::parse_command_chain(command);
    for chain in &chains {
        let tokens = wf_sandbox::strategy::shell::vfs_paths::tokenize_command(chain);
        let (reads, writes) = wf_sandbox::extract_file_paths(&tokens);
        for raw in &writes {
            let resolved = resolve_command_path(raw, Some(dir), default_dir);
            ensure_shell_path_in_scope(tool, &resolved, scope, true, execution, mode)?;
        }
        for raw in &reads {
            let resolved = resolve_command_path(raw, Some(dir), default_dir);
            ensure_shell_path_in_scope(tool, &resolved, scope, false, execution, mode)?;
        }
    }
    Ok(())
}

/// Scope gate shared by the stateful session tools, mirroring the
/// `execute_command` baseline: the startup directory must sit inside the
/// read or write prefixes and every parsed command path is re-checked.
/// Session tools carry no sandbox policy of their own, so the gate always
/// runs fail-closed. Returns the resolved startup directory for checkpoint
/// sampling, or `None` when no execution scope applies.
pub(crate) fn gate_session_shell(
    tool: &'static str,
    params_cwd: Option<&str>,
    command: Option<&str>,
    ctx: &ToolExecutionContext,
) -> ToolResult<Option<std::path::PathBuf>> {
    let Some(scope) = ctx.file_scope.as_ref() else {
        return Ok(None);
    };
    if !scope.enabled {
        let default_dir = scope.workdir.as_ref().map(std::path::PathBuf::from);
        let scope_dir = resolve_scope_dir(params_cwd, default_dir.as_ref());
        check_shell_scope(
            tool,
            scope_dir.as_ref(),
            command.unwrap_or(""),
            Some(scope),
            default_dir.as_ref(),
            &ctx.execution_id.to_string(),
            None,
        )?;
        return Ok(None);
    }
    let default_dir = scope.workdir.as_ref().map(std::path::PathBuf::from);
    let scope_dir = resolve_scope_dir(params_cwd, default_dir.as_ref());
    check_shell_scope(
        tool,
        scope_dir.as_ref(),
        command.unwrap_or(""),
        Some(scope),
        default_dir.as_ref(),
        &ctx.execution_id.to_string(),
        None,
    )?;
    Ok(scope_dir)
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
                "execute_command",
                scope_dir.as_ref(),
                &command,
                ctx.file_scope.as_ref(),
                effective_default.as_ref(),
                &ctx.execution_id.to_string(),
                scope_mode,
            )?;

            let input = parameters.get("input").and_then(|v| v.as_str());

            let execution_id = ctx.execution_id.to_string();
            // Exclusive sampling window: held across before-capture,
            // command execution and after-commit so concurrent shells
            // cannot interleave their scans and misattribute writes.
            // `Disabled` samples in parallel (explicit opt-out);
            // `TimedOut` skips sampling for this window: the command still
            // runs, but no diff is submitted and the result is marked
            // degraded instead of attributing possibly foreign writes.
            let window = if scope_dir.is_some() {
                match ctx.checkpoint_session.as_ref() {
                    Some(session) => session.acquire_shell_window(&execution_id).await,
                    None => wf_checkpoint::ShellWindowOutcome::Disabled,
                }
            } else {
                wf_checkpoint::ShellWindowOutcome::Disabled
            };
            let degraded = matches!(window, wf_checkpoint::ShellWindowOutcome::TimedOut);
            let _shell_window = match window {
                wf_checkpoint::ShellWindowOutcome::Acquired(guard) => Some(guard),
                wf_checkpoint::ShellWindowOutcome::Disabled
                | wf_checkpoint::ShellWindowOutcome::TimedOut => None,
            };
            if degraded {
                tracing::warn!(
                    execution = %execution_id,
                    scope = ?scope_dir,
                    "shell window timed out; executing without sampling"
                );
            } else if let Some(scope) = scope_dir.as_ref() {
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
                    // so already-written files are still captured. A degraded
                    // window captured nothing, so it ends nothing.
                    if !degraded {
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
            // commands: already-written files are still captured. A degraded
            // window captured nothing, so it ends nothing; the result is
            // marked instead.
            if degraded {
                result["sampling_degraded"] = Value::Bool(true);
            } else if let Some(scope) = scope_dir.as_ref() {
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

    fn scoped_shell_ctx(root: &std::path::Path, sub: &str) -> ToolExecutionContext {
        let root_str = root.to_string_lossy().to_string();
        let sub_str = root.join(sub).to_string_lossy().to_string();
        ToolExecutionContext::new("exec-session".into()).with_file_scope(Some(
            wf_types::config::file_scope::ResolvedFileScope {
                enabled: true,
                read_prefixes: vec![root_str.clone()],
                write_prefixes: vec![sub_str.clone()],
                checkpoint_prefixes: vec![sub_str],
                workdir: Some(root_str),
            },
        ))
    }

    #[test]
    fn session_gate_rejects_outside_cwd() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("work")).unwrap();
        let ctx = scoped_shell_ctx(root.path(), "work");
        let outside = other.path().to_string_lossy().to_string();
        let err = gate_session_shell("backend_shell", Some(&outside), Some("echo hi"), &ctx)
            .expect_err("outside session cwd must fail");
        assert!(err.is_scope_violation());
        let inside = root.path().join("work").to_string_lossy().to_string();
        let dir = gate_session_shell("backend_shell", Some(&inside), Some("echo hi"), &ctx)
            .expect("inside session cwd must pass");
        assert!(dir.is_some());
    }

    #[test]
    fn session_gate_rejects_outside_command_path() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("work")).unwrap();
        let ctx = scoped_shell_ctx(root.path(), "work");
        let inside = root.path().join("work").to_string_lossy().to_string();
        let err = gate_session_shell(
            "execute_in_session",
            Some(&inside),
            Some("cat /etc/passwd"),
            &ctx,
        )
        .expect_err("outside command path must fail");
        assert!(err.is_scope_violation());
    }

    #[test]
    fn quoted_redirect_outside_scope_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("work")).unwrap();
        let ctx = scoped_shell_ctx(root.path(), "work");
        let inside = root.path().join("work").to_string_lossy().to_string();
        let err = gate_session_shell(
            "execute_in_session",
            Some(&inside),
            Some("echo hi > \"/etc/passwd\""),
            &ctx,
        )
        .expect_err("quoted outside redirect must fail");
        assert!(err.is_scope_violation());
    }

    #[test]
    fn closed_channel_denies_shell_writes_but_allows_reads() {
        let root = tempfile::tempdir().unwrap();
        let root_str = root.path().to_string_lossy().to_string();
        let ctx = ToolExecutionContext::new("exec-closed".into()).with_file_scope(Some(
            wf_types::config::file_scope::ResolvedFileScope {
                enabled: false,
                read_prefixes: vec![root_str],
                write_prefixes: vec![],
                checkpoint_prefixes: vec![],
                workdir: None,
            },
        ));
        gate_session_shell(
            "execute_in_session",
            Some(root.path().to_str().unwrap()),
            Some("echo hi"),
            &ctx,
        )
        .expect("read-only shell must pass with closed channel");
        let err = gate_session_shell(
            "execute_in_session",
            Some(root.path().to_str().unwrap()),
            Some("echo hi > out.txt"),
            &ctx,
        )
        .expect_err("shell write must fail with closed channel");
        assert!(err.is_scope_violation());
    }
}
