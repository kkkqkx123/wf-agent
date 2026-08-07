use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub name: String,
    pub binary_name: String,
    pub custom_path: Option<String>,
    pub additional_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ExecutorInfo {
    pub name: String,
    pub binary_path: String,
    pub status: ExecutorStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutorStatus {
    Available,
    Unavailable,
    Error,
}

#[derive(Debug, Clone)]
pub struct CliExecutionOptions {
    pub args: Vec<String>,
    pub max_lines: Option<usize>,
    pub max_output_bytes: Option<u64>,
    pub output_dir: Option<PathBuf>,
    pub call_id: Option<String>,
    pub timeout_ms: Option<u64>,
    pub cwd: Option<String>,
    pub env: Option<Vec<(String, String)>>,
}

#[derive(Debug, Clone)]
pub struct CliExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub success: bool,
    pub stdout_path: Option<PathBuf>,
    pub stderr_path: Option<PathBuf>,
    pub truncated: bool,
    pub truncated_reason: Option<String>,
    pub total_lines: Option<usize>,
    pub total_bytes: Option<u64>,
}

struct PipeOutput {
    text: String,
    path: Option<PathBuf>,
    truncated: bool,
    truncated_reason: Option<String>,
    total_lines: usize,
    total_bytes: u64,
}

#[allow(async_fn_in_trait)]
pub trait CliExecutor: Send + Sync {
    fn config(&self) -> &ExecutorConfig;
    fn binary_path(&self) -> &Option<String>;

    fn set_binary_path(&mut self, path: Option<String>);

    async fn find_binary(&self) -> Option<String> {
        let config = self.config();

        if let Some(ref custom) = config.custom_path {
            if tokio::fs::metadata(custom).await.is_ok() {
                return Some(custom.clone());
            }
        }

        let path_result = find_in_path(&config.binary_name).await;
        if path_result.is_some() {
            return path_result;
        }

        if let Some(ref extra) = config.additional_paths {
            for p in extra {
                if tokio::fs::metadata(p).await.is_ok() {
                    return Some(p.clone());
                }
            }
        }

        None
    }

    async fn ensure_initialized(&self) -> ToolResult<String> {
        if let Some(ref path) = self.binary_path() {
            return Ok(path.clone());
        }

        let path = self.find_binary().await.ok_or_else(|| {
            ToolError::Internal(format!("Could not find {} binary", self.config().name))
        })?;

        Ok(path)
    }

    async fn execute(&self, options: &CliExecutionOptions) -> ToolResult<CliExecutionResult> {
        let bin = self.ensure_initialized().await?;
        let max_lines = options.max_lines.unwrap_or(1000);
        let max_bytes = options.max_output_bytes.unwrap_or(20 * 1024 * 1024);

        let mut cmd = Command::new(&bin);
        cmd.args(&options.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(ref cwd) = options.cwd {
            cmd.current_dir(cwd);
        }

        if let Some(ref env) = options.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }

        let mut child = cmd.spawn()?;

        let stdout_path = options
            .output_dir
            .as_ref()
            .zip(options.call_id.as_ref())
            .map(|(dir, id)| dir.join(format!("{}-stdout.txt", id)));
        let stderr_path = options
            .output_dir
            .as_ref()
            .zip(options.call_id.as_ref())
            .map(|(dir, id)| dir.join(format!("{}-stderr.txt", id)));

        if let Some(ref path) = stdout_path {
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
        }

        let (stdout_res, stderr_res, status) = if let Some(ms) = options.timeout_ms {
            tokio::time::timeout(
                Duration::from_millis(ms),
                collect_child_output(&mut child, max_lines, max_bytes, &stdout_path, &stderr_path),
            )
            .await
            .map_err(|_| ToolError::Timeout {
                tool_id: self.config().name.clone(),
                timeout_ms: ms,
            })??
        } else {
            collect_child_output(&mut child, max_lines, max_bytes, &stdout_path, &stderr_path)
                .await?
        };

        Ok(CliExecutionResult {
            stdout: stdout_res.text,
            stderr: stderr_res.text,
            exit_code: status.code().unwrap_or(-1),
            success: status.success(),
            stdout_path: stdout_res.path,
            stderr_path: stderr_res.path,
            truncated: stdout_res.truncated || stderr_res.truncated,
            truncated_reason: stdout_res
                .truncated_reason
                .clone()
                .or(stderr_res.truncated_reason),
            total_lines: Some(stdout_res.total_lines),
            total_bytes: Some(stdout_res.total_bytes),
        })
    }
}

async fn collect_child_output(
    child: &mut tokio::process::Child,
    max_lines: usize,
    max_bytes: u64,
    stdout_path: &Option<PathBuf>,
    stderr_path: &Option<PathBuf>,
) -> ToolResult<(PipeOutput, PipeOutput, std::process::ExitStatus)> {
    let stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| ToolError::Internal("stdout pipe not configured".to_string()))?;
    let stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| ToolError::Internal("stderr pipe not configured".to_string()))?;

    let (stdout_res, stderr_res) = tokio::join!(
        read_pipe(stdout_pipe, max_lines, max_bytes, stdout_path),
        read_pipe(stderr_pipe, max_lines, max_bytes, stderr_path),
    );

    let status = child.wait().await?;
    Ok((stdout_res, stderr_res, status))
}

async fn read_pipe<R: AsyncRead + Unpin>(
    mut reader: R,
    max_lines: usize,
    max_bytes: u64,
    file_path: &Option<PathBuf>,
) -> PipeOutput {
    let mut file = match file_path {
        Some(path) => tokio::fs::File::create(path).await.ok(),
        None => None,
    };

    let mut buf: Vec<u8> = Vec::new();
    let mut total_lines: usize = 0;
    let mut total_bytes: u64 = 0;
    let mut hit_byte_limit = false;
    let mut chunk = [0u8; 65536];

    loop {
        let n = match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };

        let remaining = max_bytes.saturating_sub(total_bytes) as usize;
        if remaining == 0 {
            hit_byte_limit = true;
            break;
        }

        let take = n.min(remaining);

        for &b in &chunk[..take] {
            if b == b'\n' {
                total_lines += 1;
            }
        }

        total_bytes += take as u64;
        buf.extend_from_slice(&chunk[..take]);

        if let Some(ref mut f) = file {
            let _ = f.write_all(&chunk[..take]).await;
        }

        if take < n {
            hit_byte_limit = true;
            break;
        }
    }

    drop(file);

    let raw = String::from_utf8_lossy(&buf).to_string();
    let (text, truncated, reason) =
        build_truncated_output(&raw, total_bytes, max_lines, hit_byte_limit, file_path);

    PipeOutput {
        text,
        path: file_path.clone(),
        truncated,
        truncated_reason: reason,
        total_lines,
        total_bytes,
    }
}

fn build_truncated_output(
    raw: &str,
    total_bytes: u64,
    max_lines: usize,
    hit_byte_limit: bool,
    file_path: &Option<PathBuf>,
) -> (String, bool, Option<String>) {
    let path_hint = file_path
        .as_ref()
        .map(|p| {
            format!(
                "\nFull output: {}\nYou can use `grep`, `head`, `tail`, `sed` on that file to search or paginate.",
                p.display()
            )
        });

    let line_count = raw.lines().count();
    let hit_line_limit = line_count > max_lines;

    match (hit_byte_limit, hit_line_limit) {
        (true, true) => {
            let lines: Vec<&str> = raw.lines().take(max_lines).collect();
            let body = format!(
                "{}\n... (output limited to {} bytes, truncated {} lines)",
                lines.join("\n"),
                total_bytes,
                line_count - max_lines,
            );
            let text = match path_hint {
                Some(ref h) => format!("{}{}", body, h),
                None => body,
            };
            (text, true, Some("size+lines".to_string()))
        }
        (true, false) => {
            let body = format!("... (output limited to {} bytes)", total_bytes);
            let text = match path_hint {
                Some(ref h) => format!("{}{}", body, h),
                None => body,
            };
            (text, true, Some("size".to_string()))
        }
        (false, true) => {
            let lines: Vec<&str> = raw.lines().take(max_lines).collect();
            let body = format!(
                "{}\n... (truncated {} lines)",
                lines.join("\n"),
                line_count - max_lines,
            );
            let text = match path_hint {
                Some(ref h) => format!("{}{}", body, h),
                None => body,
            };
            (text, true, Some("lines".to_string()))
        }
        (false, false) => (raw.to_string(), false, None),
    }
}

async fn run_command(
    cmd: &mut tokio::process::Command,
    max_lines: usize,
    max_bytes: u64,
    timeout_ms: Option<u64>,
) -> ToolResult<(PipeOutput, PipeOutput, std::process::ExitStatus)> {
    let exec = async {
        let mut child = cmd.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::Internal("stdout pipe not configured".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ToolError::Internal("stderr pipe not configured".to_string()))?;

        let (stdout_res, stderr_res) = tokio::join!(
            read_pipe(stdout, max_lines, max_bytes, &None),
            read_pipe(stderr, max_lines, max_bytes, &None),
        );
        let status = child.wait().await?;

        Ok::<_, ToolError>((stdout_res, stderr_res, status))
    };

    match timeout_ms {
        Some(ms) => match tokio::time::timeout(Duration::from_millis(ms), exec).await {
            Ok(result) => result,
            Err(_) => Err(ToolError::Timeout {
                tool_id: "cli".into(),
                timeout_ms: ms,
            }),
        },
        None => exec.await,
    }
}

async fn find_in_path(binary_name: &str) -> Option<String> {
    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    let output = Command::new(which_cmd)
        .arg(binary_name)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()?
            .trim()
            .to_string();
        if !path.is_empty() && tokio::fs::metadata(&path).await.is_ok() {
            return Some(path);
        }
    }
    None
}

pub struct CliToolExecutor {
    binary: String,
    args_template: Vec<String>,
    timeout_ms: Option<u64>,
    max_lines: Option<usize>,
    #[allow(dead_code)]
    env: Option<Vec<(String, String)>>,
}

impl CliToolExecutor {
    pub fn new(binary: impl Into<String>, args_template: Vec<String>) -> Self {
        Self {
            binary: binary.into(),
            args_template,
            timeout_ms: None,
            max_lines: None,
            env: None,
        }
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    pub fn with_max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = Some(max_lines);
        self
    }

    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = Some(env);
        self
    }
}

#[async_trait]
impl ToolExecutor for CliToolExecutor {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        _options: &ToolExecutionOptions,
        _context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();
        BaseExecutor::validate_parameters(tool, parameters)?;

        let args: Vec<String> = self
            .args_template
            .iter()
            .map(|arg| {
                if arg.starts_with('{') && arg.ends_with('}') {
                    let key = &arg[1..arg.len() - 1];
                    parameters
                        .get(key)
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| arg.clone())
                } else {
                    arg.clone()
                }
            })
            .collect();

        let max_lines = self.max_lines.unwrap_or(1000);
        let max_bytes = 20 * 1024 * 1024;

        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let (stdout, stderr, status) = run_command(&mut cmd, max_lines, max_bytes, self.timeout_ms)
            .await
            .map_err(|e| {
                if matches!(&e, ToolError::Timeout { .. }) {
                    e
                } else {
                    ToolError::ExecutionFailed {
                        tool_id: tool.id.clone(),
                        reason: e.to_string(),
                    }
                }
            })?;
        let execution_time = start.elapsed().as_millis() as i64;

        Ok(BaseExecutor::build_result(
            status.success(),
            Some(serde_json::json!({
                "stdout": stdout.text,
                "stderr": stderr.text,
                "exit_code": status.code().unwrap_or(-1),
            })),
            if status.success() {
                None
            } else {
                Some(stderr.text)
            },
            execution_time,
            0,
        ))
    }

    fn executor_type(&self) -> &str {
        "cli"
    }
}

pub struct RipgrepExecutor {
    config: ExecutorConfig,
    binary_path: Option<String>,
}

impl RipgrepExecutor {
    pub fn new() -> Self {
        Self {
            config: ExecutorConfig {
                name: "ripgrep".to_string(),
                binary_name: "rg".to_string(),
                custom_path: None,
                additional_paths: None,
            },
            binary_path: None,
        }
    }

    pub async fn search(
        &self,
        pattern: &str,
        path: &str,
        max_lines: Option<usize>,
    ) -> ToolResult<CliExecutionResult> {
        let mut args = vec![
            "--line-number".to_string(),
            "--color".to_string(),
            "never".to_string(),
            pattern.to_string(),
            path.to_string(),
        ];

        if let Some(max) = max_lines {
            args.push(format!("--max-count={}", max));
        }

        self.execute(&CliExecutionOptions {
            args,
            max_lines: Some(500),
            max_output_bytes: None,
            output_dir: None,
            call_id: None,
            timeout_ms: Some(30000),
            cwd: None,
            env: None,
        })
        .await
    }

    pub async fn list_files(
        &self,
        path: &str,
        glob: Option<&str>,
    ) -> ToolResult<CliExecutionResult> {
        let mut args = vec!["--files".to_string(), path.to_string()];
        if let Some(g) = glob {
            args.push("--glob".to_string());
            args.push(g.to_string());
        }

        self.execute(&CliExecutionOptions {
            args,
            max_lines: Some(1000),
            max_output_bytes: None,
            output_dir: None,
            call_id: None,
            timeout_ms: Some(30000),
            cwd: None,
            env: None,
        })
        .await
    }
}

impl Default for RipgrepExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl CliExecutor for RipgrepExecutor {
    fn config(&self) -> &ExecutorConfig {
        &self.config
    }

    fn binary_path(&self) -> &Option<String> {
        &self.binary_path
    }

    fn set_binary_path(&mut self, path: Option<String>) {
        self.binary_path = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_truncated_output_no_truncation() {
        let (text, truncated, reason) =
            build_truncated_output("hello\nworld", 12, 100, false, &None);
        assert_eq!(text, "hello\nworld");
        assert!(!truncated);
        assert!(reason.is_none());
    }

    #[test]
    fn test_build_truncated_output_line_limit() {
        let input = "line1\nline2\nline3\nline4\nline5";
        let (text, truncated, reason) = build_truncated_output(input, 30, 3, false, &None);
        assert!(truncated);
        assert_eq!(reason.as_deref(), Some("lines"));
        assert!(text.contains("line1"));
        assert!(text.contains("line3"));
        assert!(text.contains("truncated 2 lines"));
        assert!(!text.contains("line4"));
    }

    #[test]
    fn test_build_truncated_output_byte_limit() {
        let (text, truncated, reason) =
            build_truncated_output("hello\nworld", 12, 100, true, &None);
        assert!(truncated);
        assert_eq!(reason.as_deref(), Some("size"));
        assert!(text.contains("limited to 12 bytes"));
    }

    #[test]
    fn test_build_truncated_output_both_limits() {
        let input = "line1\nline2\nline3\nline4\nline5";
        let (text, truncated, reason) = build_truncated_output(input, 20, 2, true, &None);
        assert!(truncated);
        assert_eq!(reason.as_deref(), Some("size+lines"));
        assert!(text.contains("line1"));
        assert!(text.contains("line2"));
        assert!(text.contains("limited to 20 bytes"));
        assert!(text.contains("truncated 3 lines"));
    }

    #[test]
    fn test_build_truncated_output_with_path_hint() {
        let path = Some(PathBuf::from("/tmp/output.txt"));
        let input = "line1\nline2\nline3";
        let (text, truncated, _) = build_truncated_output(input, 30, 1, false, &path);
        assert!(truncated);
        assert!(text.contains("/tmp/output.txt"));
        assert!(text.contains("grep"));
    }
}
