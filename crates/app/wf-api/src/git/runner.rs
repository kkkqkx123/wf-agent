use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::types::{GitError, GitResult};

pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
}

pub fn run_git(root: &Path, args: &[&str], timeout_ms: u64) -> GitResult<GitOutput> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| GitError::Command(format!("failed to spawn git: {e}")))?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(GitError::Timeout(timeout_ms));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(GitError::Command(format!("git wait failed: {e}"))),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| GitError::Command(format!("git output failed: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(GitError::Command(detail));
    }
    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn run_shell(root: &Path, command: &str, timeout_ms: u64) -> GitResult<GitOutput> {
    run_shell_impl(root, command, timeout_ms)
}

fn run_shell_impl(root: &Path, command: &str, timeout_ms: u64) -> GitResult<GitOutput> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(root)
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| GitError::Command(format!("failed to spawn shell: {e}")))?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(GitError::Timeout(timeout_ms));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(GitError::Command(format!("shell wait failed: {e}"))),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| GitError::Command(format!("shell output failed: {e}")))?;
    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
