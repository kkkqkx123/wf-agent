//! Shared shell process spawning.
//!
//! Single place that turns `(command, cwd, env, shell_type)` into a spawned
//! shell process with process-group setup, reused by both the stateless
//! runner (`crate::runner`) and the session engine (`crate::engine`) so the
//! two entries never drift on env, cwd or kill semantics.
//!
//! **Env inheritance rule** (aligned with the TS terminal service
//! `{...process.env, ...env}`): every spawned shell inherits the parent
//! process environment and then overlays the session/command `env`. A
//! variable set in the overlay therefore replaces the inherited value
//! (including special vars such as `PATH`).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::error::{ShellError, ShellResult};
use crate::shell_detector::{default_shell_detector, resolve_shell_command, ShellType};

/// How long a graceful kill waits after SIGTERM before forcing SIGKILL for
/// one-shot commands.
const DEFAULT_GRACEFUL_KILL_TIMEOUT_MS: u64 = 5000;

/// Build a configured shell invocation without spawning: resolved shell
/// program, command arguments, working directory and process-group setup.
/// Every entry (runner, session engine) builds its `Command` from here.
pub fn build_shell_command(
    shell_type: Option<ShellType>,
    command: &str,
    cwd: Option<&Path>,
) -> Command {
    let (shell, shell_args) = resolve_shell_command(default_shell_detector(), shell_type, command);
    let mut cmd = Command::new(shell);
    cmd.args(shell_args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Make the child the leader of its own process group so a graceful
        // kill can terminate the shell and its descendants together.
        cmd.process_group(0);
    }
    cmd
}

/// Run a single command to completion in the current (blocking) thread,
/// capturing stdout/stderr and enforcing a timeout. An optional one-shot
/// `input` line is written to stdin right after start, then stdin is closed.
/// The process inherits the parent environment with the optional `env`
/// overlay merged on top (see the crate-level env inheritance note).
pub fn run_shell_blocking(
    shell_type: Option<ShellType>,
    command: &str,
    cwd: Option<&Path>,
    env: Option<&HashMap<String, String>>,
    input: Option<&str>,
    timeout: Duration,
) -> ShellResult<Output> {
    let mut cmd = build_shell_command(shell_type, command, cwd);
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    if let Some(env) = env {
        cmd.envs(env);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| ShellError::ExecutionError(format!("Failed to spawn command: {}", e)))?;

    if let Some(text) = input {
        if let Some(mut stdin) = child.stdin.take() {
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(b'\n');
            let _ = stdin.write_all(&bytes);
        }
    }

    // Drain stdout/stderr concurrently so a full pipe never deadlocks the
    // blocking wait below. On a timeout the child is killed first, closing
    // the pipes, so the detached reader threads terminate on their own.
    let stdout = child.stdout.take().map(read_pipe_thread);
    let stderr = child.stderr.take().map(read_pipe_thread);

    let status = wait_with_timeout(&mut child, timeout)?;
    let stdout = stdout.map(join_pipe_thread).unwrap_or_default();
    let stderr = stderr.map(join_pipe_thread).unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Block until the child exits or `timeout` elapses; on timeout the child is
/// terminated gracefully and an error is returned.
fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
) -> ShellResult<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = graceful_kill_child(child, DEFAULT_GRACEFUL_KILL_TIMEOUT_MS);
                    return Err(ShellError::ExecutionError(format!(
                        "Command timed out after {} seconds",
                        timeout.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                return Err(ShellError::ExecutionError(format!(
                    "Failed to run command: {}",
                    e
                )));
            }
        }
    }
}

/// Read a pipe to EOF into a byte vector on a background thread.
fn read_pipe_thread(pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut reader = pipe;
        let mut buffer = [0u8; 4096];
        let mut out = Vec::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buffer[..n]),
                Err(_) => break,
            }
        }
        out
    })
}

/// Join a reader thread, defaulting to an empty buffer on panic.
fn join_pipe_thread(handle: std::thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    handle.join().unwrap_or_default()
}

/// Poll `check` until it reports success or the timeout elapses.
pub(crate) fn wait_for_exit<F>(mut check: F, timeout_ms: u64) -> bool
where
    F: FnMut() -> Result<bool, ()>,
{
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match check() {
            Ok(true) => return true,
            Ok(false) => {
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(()) => return false,
        }
    }
}

/// Gracefully terminate a std child: SIGTERM the process group, wait up to
/// `timeout_ms`, then SIGKILL. Degenerates to a force kill on non-Unix.
pub(crate) fn graceful_kill_child(child: &mut Child, timeout_ms: u64) -> ShellResult<()> {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        let exited = {
            let mut check = || match child.try_wait() {
                Ok(Some(_)) => Ok(true),
                Ok(None) => Ok(false),
                Err(_) => Err(()),
            };
            wait_for_exit(&mut check, timeout_ms)
        };
        if !exited {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
        let _ = child.wait();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_shell_blocking_echo() {
        let output = run_shell_blocking(
            None,
            "echo hello-blocking",
            None,
            None,
            None,
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello-blocking"));
    }

    #[test]
    fn test_run_shell_blocking_input() {
        let output = run_shell_blocking(
            None,
            "cat",
            None,
            None,
            Some("hello-input"),
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello-input"));
    }

    #[test]
    fn test_run_shell_blocking_env_overlay() {
        let mut env = HashMap::new();
        env.insert("WF_TEST_OVERLAY".to_string(), "overlay-value".to_string());
        let output = run_shell_blocking(
            None,
            "echo $WF_TEST_OVERLAY",
            None,
            Some(&env),
            None,
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains("overlay-value"));
    }

    #[test]
    fn test_run_shell_blocking_timeout() {
        let result = run_shell_blocking(
            None,
            "sleep 10",
            None,
            None,
            None,
            Duration::from_millis(200),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[test]
    fn test_run_shell_blocking_nonzero_exit() {
        let output =
            run_shell_blocking(None, "exit 3", None, None, None, Duration::from_secs(10)).unwrap();
        assert_eq!(output.status.code(), Some(3));
    }
}
