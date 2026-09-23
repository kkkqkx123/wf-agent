use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use wf_script::{
    ExecutorMode, ScriptDefinition, ScriptEngine, ScriptEngineOptions, ScriptExecutionOptions,
    ScriptExecutionResult,
};
use wf_types::script::sandbox::{SandboxConfig, SandboxMode};

static SHARED_SHELL_STORE: OnceLock<Arc<wf_shell::engine::BackgroundShellStore>> = OnceLock::new();

fn shared_shell_store() -> Arc<wf_shell::engine::BackgroundShellStore> {
    SHARED_SHELL_STORE
        .get_or_init(|| Arc::new(wf_shell::engine::BackgroundShellStore::new(None)))
        .clone()
}

fn default_sandbox_config() -> SandboxConfig {
    SandboxConfig {
        mode: Some(SandboxMode::Strict),
        policy: None,
        shell_strategy: None,
        python_strategy: None,
        javascript_strategy: None,
        lua_strategy: None,
        vfs: None,
        workdir: None,
        env: None,
        resource_limits: None,
        skip_gate_check: None,
    }
}

/// Output of a routed script execution: the engine result plus the
/// transport metadata the workflow layer records on the node.
pub struct RoutedScriptResult {
    pub result: ScriptExecutionResult,
    pub executor_mode: ExecutorMode,
    pub strategy_id: Option<String>,
    pub sandbox_mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct SandboxExtras {
    strategy_id: Option<String>,
    sandbox_mode: Option<String>,
}

/// Routes a rendered script command to a transport.
///
/// The template, policy and retry layers stay inside `ScriptEngine`; this
/// router only performs the single-shot transport selected by the resolved
/// executor mode: one-shot runner, shared background session, PTY session or
/// sandbox runtime.
pub struct ScriptRouter {
    sandbox: Arc<wf_sandbox::SandboxRuntime>,
    shell: Arc<wf_shell::engine::BackgroundShellStore>,
}

impl Default for ScriptRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything one transport call needs: the session stores plus the
/// resolved script identity, command and executor mode.
struct TransportRequest<'a> {
    shell: &'a Arc<wf_shell::engine::BackgroundShellStore>,
    sandbox: &'a Arc<wf_sandbox::SandboxRuntime>,
    script_name: &'a str,
    language: &'a str,
    mode: &'a ExecutorMode,
    command: String,
    options: Option<ScriptExecutionOptions>,
    sandbox_config: &'a SandboxConfig,
}

impl ScriptRouter {
    pub fn new() -> Self {
        Self {
            sandbox: Arc::new(wf_sandbox::SandboxRuntime::new()),
            shell: shared_shell_store(),
        }
    }

    pub fn with_sandbox(sandbox: Arc<wf_sandbox::SandboxRuntime>) -> Self {
        Self {
            sandbox,
            shell: shared_shell_store(),
        }
    }

    pub fn with_stores(
        sandbox: Arc<wf_sandbox::SandboxRuntime>,
        shell: Arc<wf_shell::engine::BackgroundShellStore>,
    ) -> Self {
        Self { sandbox, shell }
    }

    pub fn shell_store(&self) -> &Arc<wf_shell::engine::BackgroundShellStore> {
        &self.shell
    }

    pub async fn run(
        &self,
        script: &ScriptDefinition,
        options: Option<&ScriptExecutionOptions>,
        engine_options: &ScriptEngineOptions,
        sandbox_config: Option<SandboxConfig>,
    ) -> RoutedScriptResult {
        let sandbox = self.sandbox.clone();
        let shell = self.shell.clone();
        let sandbox_config = sandbox_config.unwrap_or_else(default_sandbox_config);
        let script_name = script.name.clone();
        let language = script
            .language
            .clone()
            .unwrap_or_else(|| "shell".to_string());
        let mode = ScriptEngine::resolve_executor_mode(script, options);
        let extras: Arc<std::sync::Mutex<SandboxExtras>> =
            Arc::new(std::sync::Mutex::new(SandboxExtras::default()));
        let extras_sink = extras.clone();
        let result = ScriptEngine
            .execute(script, options, engine_options, move |command, opts| {
                let sandbox = sandbox.clone();
                let shell = shell.clone();
                let sandbox_config = sandbox_config.clone();
                let script_name = script_name.clone();
                let language = language.clone();
                let mode = mode.clone();
                let extras_sink = extras_sink.clone();
                async move {
                    let (result, sandbox_extras) = Self::transport(TransportRequest {
                        shell: &shell,
                        sandbox: &sandbox,
                        script_name: &script_name,
                        language: &language,
                        mode: &mode,
                        command,
                        options: opts,
                        sandbox_config: &sandbox_config,
                    })
                    .await;
                    if let Some(found) = sandbox_extras {
                        if let Ok(mut slot) = extras_sink.lock() {
                            *slot = found;
                        }
                    }
                    result
                }
            })
            .await;
        let found = extras.lock().map(|slot| slot.clone()).unwrap_or_default();
        RoutedScriptResult {
            result,
            executor_mode: ScriptEngine::resolve_executor_mode(script, options),
            strategy_id: found.strategy_id,
            sandbox_mode: found.sandbox_mode,
        }
    }

    async fn transport(
        req: TransportRequest<'_>,
    ) -> (ScriptExecutionResult, Option<SandboxExtras>) {
        let TransportRequest {
            shell,
            sandbox,
            script_name,
            language,
            mode,
            command,
            options,
            sandbox_config,
        } = req;
        match mode {
            ExecutorMode::Direct => (Self::run_direct(script_name, &command, options).await, None),
            ExecutorMode::Shared => (
                Self::run_session(shell, script_name, &command, options, false).await,
                None,
            ),
            ExecutorMode::Pty => (
                Self::run_session(shell, script_name, &command, options, true).await,
                None,
            ),
            ExecutorMode::SandboxShell
            | ExecutorMode::SandboxPython
            | ExecutorMode::SandboxJavaScript => {
                let (result, found) = Self::run_sandbox(
                    sandbox,
                    script_name,
                    language,
                    &command,
                    options.clone(),
                    sandbox_config,
                )
                .await;
                (result, Some(found))
            }
        }
    }

    async fn run_direct(
        script_name: &str,
        command: &str,
        options: Option<ScriptExecutionOptions>,
    ) -> ScriptExecutionResult {
        let cwd = options.as_ref().and_then(|o| o.working_directory.clone());
        let timeout_ms = options
            .as_ref()
            .and_then(|o| o.timeout_ms)
            .unwrap_or(120_000);
        let stdin = options.as_ref().and_then(|o| o.stdin.clone());
        let env: HashMap<String, String> = options
            .as_ref()
            .and_then(|o| o.environment.clone())
            .unwrap_or_default();
        let command = command.to_string();
        let outcome = tokio::task::spawn_blocking(move || {
            let cwd_path = cwd.map(std::path::PathBuf::from);
            let env_overlay = if env.is_empty() { None } else { Some(&env) };
            wf_shell::spawn::run_shell_blocking(
                None,
                &command,
                cwd_path.as_deref(),
                env_overlay,
                stdin.as_deref(),
                std::time::Duration::from_millis(timeout_ms),
                None,
            )
        })
        .await;
        let output = match outcome {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return ScriptExecutionResult {
                    success: false,
                    script_name: String::new(),
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                    execution_time_ms: 0,
                    error: Some(e.to_string()),
                    requires_review: false,
                    truncated: false,
                    output_bytes: None,
                    stdout_path: None,
                    stderr_path: None,
                };
            }
            Err(e) => {
                return ScriptExecutionResult {
                    success: false,
                    script_name: String::new(),
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                    execution_time_ms: 0,
                    error: Some(format!("direct execution join failed: {e}")),
                    requires_review: false,
                    truncated: false,
                    output_bytes: None,
                    stdout_path: None,
                    stderr_path: None,
                };
            }
        };
        {
            let success = output.status.success();
            let capped_out = wf_script::cap_stream(
                Some(String::from_utf8_lossy(&output.stdout).to_string()),
                options.as_ref().and_then(|o| o.max_output_bytes),
                options.as_ref().and_then(|o| o.output_spill_dir.as_deref()),
                &format!("{script_name}-stdout"),
            );
            let capped_err = wf_script::cap_stream(
                Some(String::from_utf8_lossy(&output.stderr).to_string()),
                options.as_ref().and_then(|o| o.max_output_bytes),
                options.as_ref().and_then(|o| o.output_spill_dir.as_deref()),
                &format!("{script_name}-stderr"),
            );
            let truncated = capped_out.truncated || capped_err.truncated;
            let mut error = if success {
                None
            } else {
                Some(format!(
                    "command exited with {}",
                    output
                        .status
                        .code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown status".to_string())
                ))
            };
            for note in [&capped_out.spill_error, &capped_err.spill_error]
                .into_iter()
                .flatten()
            {
                error = Some(match error {
                    Some(prev) => format!("{prev}; {note}"),
                    None => note.clone(),
                });
            }
            ScriptExecutionResult {
                success,
                script_name: script_name.to_string(),
                stdout: capped_out.text,
                stderr: capped_err.text,
                exit_code: output.status.code(),
                execution_time_ms: 0,
                error,
                requires_review: false,
                truncated,
                output_bytes: Some(
                    capped_out
                        .total_bytes
                        .saturating_add(capped_err.total_bytes),
                ),
                stdout_path: capped_out.spilled_path,
                stderr_path: capped_err.spilled_path,
            }
        }
    }

    async fn run_session(
        shell: &Arc<wf_shell::engine::BackgroundShellStore>,
        script_name: &str,
        command: &str,
        options: Option<ScriptExecutionOptions>,
        interactive: bool,
    ) -> ScriptExecutionResult {
        let script_name = script_name.to_string();
        if options.as_ref().and_then(|o| o.stdin.clone()).is_some() {
            return ScriptExecutionResult {
                success: false,
                script_name,
                stdout: None,
                stderr: None,
                exit_code: None,
                execution_time_ms: 0,
                error: Some(
                    "standard input is only supported in Direct mode; pass session input through the command or an input file instead".to_string(),
                ),
                requires_review: false,
                truncated: false,
                output_bytes: None,
                stdout_path: None,
                stderr_path: None,
            };
        }
        let command = command.to_string();
        let cwd = options.as_ref().and_then(|o| o.working_directory.clone());
        let env: HashMap<String, String> = options
            .as_ref()
            .and_then(|o| o.environment.clone())
            .unwrap_or_default();
        let timeout_ms = options.as_ref().and_then(|o| o.timeout_ms);
        let shell = shell.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let created = shell.get_or_create(
                &wf_shell::store::SessionCreateOptions {
                    cwd,
                    env,
                    interactive,
                    force_pty: false,
                    pty_size: (24, 80),
                },
                None,
            );
            let created = match created {
                Ok(created) => created,
                Err(e) => return Err(e.to_string()),
            };
            shell
                .execute_in_session(&created.session_id, &command, timeout_ms)
                .map_err(|e| e.to_string())
        })
        .await;
        let value = match outcome {
            Ok(Ok(value)) => value,
            Ok(Err(e)) => {
                return ScriptExecutionResult {
                    success: false,
                    script_name,
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                    execution_time_ms: 0,
                    error: Some(e),
                    requires_review: false,
                    truncated: false,
                    output_bytes: None,
                    stdout_path: None,
                    stderr_path: None,
                };
            }
            Err(e) => {
                return ScriptExecutionResult {
                    success: false,
                    script_name,
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                    execution_time_ms: 0,
                    error: Some(format!("session execution join failed: {e}")),
                    requires_review: false,
                    truncated: false,
                    output_bytes: None,
                    stdout_path: None,
                    stderr_path: None,
                };
            }
        };
        let success = value
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let exit_code = value
            .get("exit_code")
            .and_then(|v| v.as_i64())
            .map(|c| c as i32);
        let output = value
            .get("output")
            .and_then(|v| v.as_str())
            .map(String::from);
        let capped = wf_script::cap_stream(
            output,
            options.as_ref().and_then(|o| o.max_output_bytes),
            options.as_ref().and_then(|o| o.output_spill_dir.as_deref()),
            &format!("{script_name}-stdout"),
        );
        let mut error = if success {
            None
        } else {
            Some(format!(
                "session command failed{}",
                exit_code
                    .map(|c| format!(" with exit {c}"))
                    .unwrap_or_default()
            ))
        };
        if let Some(note) = capped.spill_error.as_ref() {
            error = Some(match error {
                Some(prev) => format!("{prev}; {note}"),
                None => note.clone(),
            });
        }
        ScriptExecutionResult {
            success,
            script_name,
            stdout: capped.text,
            stderr: None,
            exit_code,
            execution_time_ms: 0,
            error,
            requires_review: false,
            truncated: capped.truncated,
            output_bytes: Some(capped.total_bytes),
            stdout_path: capped.spilled_path,
            stderr_path: None,
        }
    }

    async fn run_sandbox(
        sandbox: &Arc<wf_sandbox::SandboxRuntime>,
        script_name: &str,
        language: &str,
        command: &str,
        options: Option<ScriptExecutionOptions>,
        sandbox_config: &SandboxConfig,
    ) -> (ScriptExecutionResult, SandboxExtras) {
        if options.as_ref().and_then(|o| o.stdin.clone()).is_some() {
            let denied = ScriptExecutionResult {
                success: false,
                script_name: script_name.to_string(),
                stdout: None,
                stderr: None,
                exit_code: None,
                execution_time_ms: 0,
                error: Some(
                    "standard input is only supported in Direct mode; pass sandbox input through the command or an input file instead".to_string(),
                ),
                requires_review: false,
                truncated: false,
                output_bytes: None,
                stdout_path: None,
                stderr_path: None,
            };
            return (
                denied,
                SandboxExtras {
                    strategy_id: None,
                    sandbox_mode: None,
                },
            );
        }
        let config = sandbox_config.clone();
        let result = sandbox
            .execute_named(language, script_name, command, &config)
            .await;
        let extras = SandboxExtras {
            strategy_id: result.strategy_id.clone(),
            sandbox_mode: result.sandbox_mode.clone(),
        };
        let capped_out = wf_script::cap_stream(
            result.stdout,
            options.as_ref().and_then(|o| o.max_output_bytes),
            options.as_ref().and_then(|o| o.output_spill_dir.as_deref()),
            &format!("{script_name}-stdout"),
        );
        let capped_err = wf_script::cap_stream(
            result.stderr,
            options.as_ref().and_then(|o| o.max_output_bytes),
            options.as_ref().and_then(|o| o.output_spill_dir.as_deref()),
            &format!("{script_name}-stderr"),
        );
        (
            ScriptExecutionResult {
                success: result.success,
                script_name: result.script_name,
                stdout: capped_out.text,
                stderr: capped_err.text,
                exit_code: result.exit_code,
                execution_time_ms: result.execution_time,
                error: {
                    let joined = [result.error, capped_out.spill_error, capped_err.spill_error]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join("; ");
                    if joined.is_empty() {
                        None
                    } else {
                        Some(joined)
                    }
                },
                requires_review: false,
                truncated: capped_out.truncated || capped_err.truncated,
                output_bytes: Some(
                    capped_out
                        .total_bytes
                        .saturating_add(capped_err.total_bytes),
                ),
                stdout_path: capped_out.spilled_path,
                stderr_path: capped_err.spilled_path,
            },
            extras,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_router_direct_echo() {
        let router = ScriptRouter::new();
        let script = ScriptDefinition {
            name: "router-direct".to_string(),
            content: Some("echo router-hi".to_string()),
            template: None,
            arguments: None,
            language: Some("shell".to_string()),
            executor_mode: Some(ExecutorMode::Direct),
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let routed = router
            .run(&script, None, &ScriptEngineOptions::default(), None)
            .await;
        assert!(routed.result.success, "error: {:?}", routed.result.error);
        assert!(routed
            .result
            .stdout
            .unwrap_or_default()
            .contains("router-hi"));
    }

    #[tokio::test]
    async fn test_router_shared_reuses_session() {
        let router = ScriptRouter::new();
        let options = ScriptExecutionOptions {
            executor_mode: Some(ExecutorMode::Shared),
            working_directory: Some("/tmp/router-shared".to_string()),
            ..Default::default()
        };
        std::fs::create_dir_all("/tmp/router-shared").expect("test dir is creatable");
        for command in ["echo first-router", "echo second-router"] {
            let script = ScriptDefinition {
                name: "router-shared".to_string(),
                content: Some(command.to_string()),
                template: None,
                arguments: None,
                language: Some("shell".to_string()),
                executor_mode: None,
                interactive: None,
                security_policy: None,
                description: None,
                enabled: None,
            };
            let routed = router
                .run(
                    &script,
                    Some(&options),
                    &ScriptEngineOptions::default(),
                    None,
                )
                .await;
            assert!(routed.result.success, "error: {:?}", routed.result.error);
        }
    }

    #[tokio::test]
    async fn test_router_direct_output_cap_truncates_tail() {
        let router = ScriptRouter::new();
        let script = ScriptDefinition {
            name: "router-cap".to_string(),
            content: Some("printf '0123456789'".to_string()),
            template: None,
            arguments: None,
            language: Some("shell".to_string()),
            executor_mode: Some(ExecutorMode::Direct),
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            max_output_bytes: Some(4),
            ..Default::default()
        };
        let routed = router
            .run(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                None,
            )
            .await;
        assert!(routed.result.success, "error: {:?}", routed.result.error);
        assert!(routed.result.truncated);
        assert_eq!(routed.result.stdout.as_deref(), Some("6789"));
    }

    #[tokio::test]
    async fn test_router_session_rejects_stdin() {
        let router = ScriptRouter::new();
        let script = ScriptDefinition {
            name: "router-stdin".to_string(),
            content: Some("cat".to_string()),
            template: None,
            arguments: None,
            language: Some("shell".to_string()),
            executor_mode: Some(ExecutorMode::Shared),
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            stdin: Some("hello".to_string()),
            ..Default::default()
        };
        let routed = router
            .run(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                None,
            )
            .await;
        assert!(!routed.result.success);
        assert!(routed
            .result
            .error
            .unwrap_or_default()
            .contains("Direct mode"));
    }

    #[tokio::test]
    async fn test_router_direct_honors_env_without_session() {
        let router = ScriptRouter::new();
        let script = ScriptDefinition {
            name: "router-env".to_string(),
            content: Some("echo $WF_ROUTER_PROBE".to_string()),
            template: None,
            arguments: None,
            language: Some("shell".to_string()),
            executor_mode: Some(ExecutorMode::Direct),
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            environment: Some(HashMap::from([(
                "WF_ROUTER_PROBE".to_string(),
                "env-direct-ok".to_string(),
            )])),
            ..Default::default()
        };
        let routed = router
            .run(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                None,
            )
            .await;
        assert!(routed.result.success, "error: {:?}", routed.result.error);
        assert!(routed
            .result
            .stdout
            .unwrap_or_default()
            .contains("env-direct-ok"));
    }
}
