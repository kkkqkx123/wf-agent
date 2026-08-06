//! Stateless single-command runner.
//!
//! The actual process launch is delegated to [`crate::spawn::run_shell_blocking`]
//! (the same spawn configuration the session engine uses), so this entry and
//! the stateful entries never drift on env/cwd/kill semantics. The blocking
//! execution runs on the tokio blocking pool so a tokio worker thread is
//! never occupied for the duration of the command.

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{ShellError, ShellResult};
use crate::shell_detector::ShellType;
use crate::spawn::run_shell_blocking;

/// Run a single command via the resolved shell, capturing stdout/stderr and
/// enforcing a timeout. An optional one-shot `input` line is written to stdin
/// right after start, then stdin is closed.
pub async fn run_command(
    command: &str,
    cwd: Option<&str>,
    timeout_ms: u64,
    shell_type: Option<ShellType>,
    input: Option<&str>,
) -> ShellResult<std::process::Output> {
    let command = command.to_string();
    let cwd = cwd.map(PathBuf::from);
    let input = input.map(String::from);
    let result = tokio::task::spawn_blocking(move || {
        run_shell_blocking(
            shell_type,
            &command,
            cwd.as_deref(),
            None,
            input.as_deref(),
            Duration::from_millis(timeout_ms),
        )
    })
    .await
    .map_err(|e| ShellError::ExecutionError(format!("Runner task failed: {}", e)))??;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_TIMEOUT_MS;

    #[tokio::test]
    async fn test_run_command_echo() {
        let output = run_command("echo hello", None, DEFAULT_TIMEOUT_MS, None, None)
            .await
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
    }

    #[tokio::test]
    async fn test_run_command_timeout() {
        let result = run_command("sleep 10", None, 200, None, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_run_command_with_input() {
        let output = run_command("cat", None, DEFAULT_TIMEOUT_MS, None, Some("runner-input"))
            .await
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("runner-input"));
    }
}
