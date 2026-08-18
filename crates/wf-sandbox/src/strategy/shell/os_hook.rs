use std::os::unix::process::ExitStatusExt;

use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::cmd::{self, ApplyOptions};
use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};
use crate::timeout::execute_with_timeout;

#[cfg(target_os = "linux")]
pub struct LinuxSeccompStrategy;

#[cfg(target_os = "linux")]
#[async_trait]
impl StrategyImplementation for LinuxSeccompStrategy {
    fn id(&self) -> &str {
        "os-hook"
    }

    fn name(&self) -> &str {
        "Linux Seccomp (OS Hook)"
    }

    fn description(&self) -> &str {
        "Linux seccomp-bpf system call filtering (AUDIT_ARCH validated, \
         policy-driven deny/allow list) plus rlimits, env clearing and \
         optional Landlock path enforcement"
    }

    fn kind(&self) -> StrategyKind {
        StrategyKind::Execution
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let cmd_text = options.command.clone();
        let workdir = options.workdir.clone();
        let env_vars = options.env_vars.clone();
        let path_policy = options.vfs.as_ref().and_then(|v| v.path_policy());
        let timeout_ms = options.timeout_ms;
        let policy = policy.clone();

        let fut = async move {
            tokio::task::spawn_blocking(move || -> std::io::Result<ScriptExecutionResult> {
                let started = std::time::Instant::now();
                let mut child = std::process::Command::new("sh");
                child.args(["-c", &cmd_text]);
                if let Some(dir) = &workdir {
                    child.current_dir(dir);
                }
                if let Some(envs) = &env_vars {
                    child.envs(envs);
                }

                // All kernel-level hardening lives in the shared execution
                // gateway so shell/CLI executors enforce the same policy.
                cmd::apply(
                    &mut child,
                    &policy,
                    &ApplyOptions {
                        clear_env: true,
                        path_policy,
                        cwd: None,
                    },
                )?;

                let output = child.output()?;
                let elapsed = started.elapsed().as_millis() as u64;

                let (exit_code, error_msg, violations) = if let Some(code) = output.status.code() {
                    if code == 0 {
                        (Some(0), None, None)
                    } else {
                        (
                            Some(code),
                            Some(format!("Command failed with exit code {code}")),
                            None,
                        )
                    }
                } else if let Some(sig) = output.status.signal() {
                    let reason = sandbox_denial_reason(sig, &output.stderr);
                    let (msg, violation) = match reason {
                        DenialReason::Seccomp => (
                            "Command denied by sandbox: system call blocked by \
                             seccomp policy (SIGSYS)"
                                .to_string(),
                            Some(vec![format!("seccomp denied a system call (signal {sig})")]),
                        ),
                        DenialReason::Landlock => (
                            "Command denied by sandbox: filesystem access blocked \
                             by Landlock policy"
                                .to_string(),
                            Some(vec!["landlock path policy violation".to_string()]),
                        ),
                        DenialReason::Killed => ("Process killed by signal".to_string(), None),
                    };
                    (Some(-sig), Some(msg), violation)
                } else {
                    (None, Some("Process exited abnormally".to_string()), None)
                };

                Ok(ScriptExecutionResult {
                    success: output.status.success(),
                    script_name: "sandbox-os-hook".to_string(),
                    stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
                    stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
                    exit_code,
                    execution_time: elapsed,
                    error: error_msg,
                    sandbox_mode: None,
                    strategy_id: Some("os-hook".to_string()),
                    violations,
                })
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                format!("Task join error: {e}").into()
            })
            .and_then(|res| {
                res.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })
            })
        };

        execute_with_timeout(fut, timeout_ms).await
    }
}

/// Why a sandboxed process was killed by a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialReason {
    /// Killed by SIGSYS: a syscall was blocked by seccomp (deterministic).
    Seccomp,
    /// Killed by SIGSYS where the blocked syscall is in the Landlock
    /// denied set: filesystem enforcement (best-effort attribution).
    Landlock,
    /// Killed for another reason (SIGKILL, SIGSEGV, ...).
    Killed,
}

/// Deterministic + heuristic denial attribution for a killed child.
///
/// SIGSYS (31) is the deterministic seccomp kill signal. For other signals
/// the stderr is inspected for the classic denial messages; this is only a
/// hint used to produce a better error message, never an authorization
/// decision.
pub fn sandbox_denial_reason(sig: i32, stderr: &[u8]) -> DenialReason {
    if sig == libc::SIGSYS {
        let text = String::from_utf8_lossy(stderr).to_lowercase();
        if text.contains("operation not permitted")
            || text.contains("permission denied")
            || text.contains("no such file")
        {
            DenialReason::Landlock
        } else {
            DenialReason::Seccomp
        }
    } else {
        DenialReason::Killed
    }
}

#[cfg(not(target_os = "linux"))]
pub struct LinuxSeccompStrategy;

#[cfg(not(target_os = "linux"))]
#[async_trait]
impl StrategyImplementation for LinuxSeccompStrategy {
    fn id(&self) -> &str {
        "os-hook"
    }

    fn name(&self) -> &str {
        "Linux Seccomp (OS Hook)"
    }

    fn description(&self) -> &str {
        "Linux seccomp-bpf system call filtering (unavailable on this platform)"
    }

    fn kind(&self) -> StrategyKind {
        StrategyKind::Execution
    }

    fn is_available(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        _options: StrategyExecuteOptions,
        _policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        Err("seccomp is not available on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic_policy() -> SandboxPolicy {
        SandboxPolicy {
            mode: None,
            shell: None,
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

    #[tokio::test]
    async fn test_seccomp_echo_works() {
        let strategy = LinuxSeccompStrategy;
        let result = strategy
            .execute(make_options("echo hello seccomp"), &basic_policy())
            .await
            .unwrap();
        if cfg!(target_os = "linux") {
            assert!(result.success, "echo should work: {:?}", result.stderr);
            assert!(result.stdout.unwrap_or_default().contains("hello seccomp"));
        }
    }

    #[tokio::test]
    async fn test_seccomp_ls_works() {
        let strategy = LinuxSeccompStrategy;
        let result = strategy
            .execute(make_options("ls /"), &basic_policy())
            .await
            .unwrap();
        if cfg!(target_os = "linux") {
            assert!(result.success, "ls should work: {:?}", result.stderr);
            let stdout = result.stdout.unwrap_or_default();
            assert!(
                stdout.contains("bin") || stdout.contains("usr") || stdout.contains("etc"),
                "ls output should contain standard dirs: {stdout}"
            );
        }
    }

    #[tokio::test]
    async fn test_seccomp_env_cleared_and_overlay_applied() {
        let strategy = LinuxSeccompStrategy;
        let mut env_vars = std::collections::HashMap::new();
        env_vars.insert("WF_SANDBOX_TEST".to_string(), "from-overlay".to_string());
        let mut options = make_options("echo ${WF_SANDBOX_TEST:-unset}");
        options.env_vars = Some(env_vars);
        let result = strategy.execute(options, &basic_policy()).await.unwrap();
        if cfg!(target_os = "linux") {
            assert!(
                result.success,
                "env overlay should work: {:?}",
                result.stderr
            );
            assert!(result.stdout.unwrap_or_default().contains("from-overlay"));
        }
    }

    #[test]
    fn test_sigsys_is_seccomp_denial() {
        assert_eq!(
            sandbox_denial_reason(libc::SIGSYS, b""),
            DenialReason::Seccomp
        );
        assert_eq!(
            sandbox_denial_reason(libc::SIGSYS, b"sh: read: Operation not permitted"),
            DenialReason::Landlock
        );
        assert_eq!(
            sandbox_denial_reason(libc::SIGKILL, b""),
            DenialReason::Killed
        );
        assert_eq!(
            sandbox_denial_reason(libc::SIGSEGV, b"Operation not permitted"),
            DenialReason::Killed
        );
    }
}
