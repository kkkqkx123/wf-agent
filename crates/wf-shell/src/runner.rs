//! Stateless single-command runner.

use tokio::process::Command;

use crate::error::{ShellError, ShellResult};
use crate::shell_detector::{default_shell_detector, resolve_shell_command, ShellType};

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
    let (shell, shell_args) = resolve_shell_command(default_shell_detector(), shell_type, command);
    let mut cmd = Command::new(&shell);
    cmd.args(&shell_args)
        .current_dir(cwd.unwrap_or("."))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if input.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    } else {
        cmd.stdin(std::process::Stdio::null());
    }
    let mut child = cmd.spawn().map_err(|e| {
        ShellError::ExecutionError(format!(
            "Failed to spawn command with shell '{}': {}",
            shell, e
        ))
    })?;

    if let Some(text) = input {
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(b'\n');
            let _ = stdin.write_all(&bytes).await;
            drop(stdin);
        }
    }

    match tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(ShellError::ExecutionError(format!(
            "Failed to run command: {}",
            e
        ))),
        Err(_) => Err(ShellError::ExecutionError(format!(
            "Command timed out after {} seconds",
            timeout_ms / 1000
        ))),
    }
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
}
