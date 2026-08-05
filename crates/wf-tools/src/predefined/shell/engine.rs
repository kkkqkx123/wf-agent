//! Background shell engine shared by the backend_shell / shell_output /
//! shell_kill tools.

use serde_json::Value;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::error::{ToolError, ToolResult};
use crate::shell::shell_detector::{default_shell_detector, resolve_shell_command, ShellType};

const MAX_OUTPUT_BYTES: usize = 256_000;
const MAX_SESSIONS: usize = 64;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A single background shell session.
pub struct ShellSession {
    command: String,
    start_time: Instant,
    child: Mutex<Option<Child>>,
    output: Arc<Mutex<OutputBuffer>>,
    killed: AtomicBool,
}

/// Ring buffer of session output with an incremental read cursor, aligned
/// with the TS terminal service `getOutput` behaviour.
#[derive(Default)]
struct OutputBuffer {
    text: String,
    /// Byte offset of the next unread output chunk.
    last_read_index: usize,
}

impl OutputBuffer {
    fn append(&mut self, chunk: &str) {
        if self.text.len() + chunk.len() > MAX_OUTPUT_BYTES {
            let keep = MAX_OUTPUT_BYTES.saturating_sub(chunk.len() + 64);
            if self.text.len() > keep {
                let cut = self.text.len() - keep;
                self.text = format!(
                    "(output truncated, {} bytes omitted)\n{}",
                    cut,
                    &self.text[cut..]
                );
                // Output was trimmed: reset the cursor to the start so readers
                // do not miss the truncated prefix.
                self.last_read_index = 0;
            }
        }
        self.text.push_str(chunk);
    }

    fn snapshot(&self) -> String {
        self.text.clone()
    }

    /// Return output since the last call and advance the cursor.
    fn read_new(&mut self) -> String {
        if self.last_read_index >= self.text.len() {
            return String::new();
        }
        let new = self.text[self.last_read_index..].to_string();
        self.last_read_index = self.text.len();
        new
    }

    /// Output since the cursor, without advancing it.
    fn peek_new(&self) -> String {
        if self.last_read_index >= self.text.len() {
            return String::new();
        }
        self.text[self.last_read_index..].to_string()
    }
}

impl ShellSession {
    fn new(command: String, child: Child, output: Arc<Mutex<OutputBuffer>>) -> Self {
        Self {
            command,
            start_time: Instant::now(),
            child: Mutex::new(Some(child)),
            output,
            killed: AtomicBool::new(false),
        }
    }

    /// Accumulate output into the shared buffer, keeping only the tail once
    /// the buffer exceeds the cap.
    fn append_output(buf: &Arc<Mutex<OutputBuffer>>, chunk: String) {
        buf.lock().unwrap().append(&chunk);
    }

    /// Read the child's stdout/stderr on a background thread until EOF.
    fn spawn_output_reader<R>(pipe: R, output: Arc<Mutex<OutputBuffer>>)
    where
        R: std::io::Read + Send + 'static,
    {
        std::thread::spawn(move || {
            let mut reader = pipe;
            let mut buffer = [0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
                        Self::append_output(&output, chunk);
                    }
                    Err(_) => break,
                }
            }
        });
    }

    pub fn status(&self) -> (String, Option<i32>) {
        let mut child = self.child.lock().unwrap();
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code();
                    *child = None;
                    return ("exited".into(), code);
                }
                Ok(None) => return ("running".into(), None),
                Err(_) => return ("unknown".into(), None),
            }
        }
        ("exited".into(), None)
    }

    /// Kill the session and wait for it to terminate.
    pub fn kill(&self) -> ToolResult<()> {
        self.killed.store(true, Ordering::SeqCst);
        let mut child = self.child.lock().unwrap();
        if let Some(c) = child.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        *child = None;
        Ok(())
    }

    /// Snapshot of the session state for the shell_output tool.
    pub fn snapshot(&self) -> Value {
        let (status, exit_code) = self.status();
        let elapsed = self.start_time.elapsed().as_secs();
        let output = self.output.lock().unwrap().snapshot();
        serde_json::json!({
            "command": self.command,
            "session_id": String::new(), // filled by the store lookup
            "status": status,
            "exit_code": exit_code,
            "running_seconds": elapsed,
            "killed": self.killed.load(Ordering::SeqCst),
            "output": output,
        })
    }

    /// Incremental read: output since the last call, advancing the cursor.
    pub fn read_new_output(&self) -> String {
        self.output.lock().unwrap().read_new()
    }

    /// Incremental peek: output since the last read, without advancing.
    pub fn peek_new_output(&self) -> String {
        self.output.lock().unwrap().peek_new()
    }
}

/// Shared store of background shell sessions across the three tools.
pub struct BackgroundShellStore {
    sessions: dashmap::DashMap<String, Arc<ShellSession>>,
    default_cwd: Option<PathBuf>,
    shell_type: Option<ShellType>,
}

impl BackgroundShellStore {
    pub fn new(default_cwd: Option<PathBuf>) -> Self {
        Self::with_shell(default_cwd, None)
    }

    /// Create a store with an explicit shell override (detected otherwise).
    pub fn with_shell(default_cwd: Option<PathBuf>, shell_type: Option<ShellType>) -> Self {
        Self {
            sessions: dashmap::DashMap::new(),
            default_cwd,
            shell_type,
        }
    }

    /// Spawn a background shell command and return its session id.
    pub fn spawn(&self, command: &str, cwd: Option<&str>) -> ToolResult<String> {
        if command.trim().is_empty() {
            return Err(ToolError::ValidationFailed(
                "Missing or invalid 'command' parameter".into(),
            ));
        }
        if self.sessions.len() >= MAX_SESSIONS {
            return Err(ToolError::ExecutionError(format!(
                "Too many background sessions (limit {})",
                MAX_SESSIONS
            )));
        }

        let cwd = cwd
            .filter(|c| !c.is_empty())
            .map(PathBuf::from)
            .or_else(|| self.default_cwd.clone());

        let (shell, shell_args) =
            resolve_shell_command(default_shell_detector(), self.shell_type, command);
        let mut cmd = Command::new(&shell);
        cmd.args(&shell_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        if let Some(dir) = &cwd {
            cmd.current_dir(dir);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| ToolError::ExecutionError(format!("Failed to spawn command: {}", e)))?;

        let output = Arc::new(Mutex::new(OutputBuffer::default()));
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::ExecutionError("Failed to capture stdout".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ToolError::ExecutionError("Failed to capture stderr".into()))?;
        ShellSession::spawn_output_reader(stdout, output.clone());
        ShellSession::spawn_output_reader(stderr, output.clone());

        let session_id = format!(
            "shell-{}-{}",
            wf_common::time::now(),
            SESSION_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let session = Arc::new(ShellSession::new(command.to_string(), child, output));
        self.sessions.insert(session_id.clone(), session);
        Ok(session_id)
    }

    /// Look up a session by id.
    pub fn get(&self, session_id: &str) -> Option<Arc<ShellSession>> {
        self.sessions.get(session_id).map(|e| e.clone())
    }

    /// Kill and remove a session by id.
    pub fn kill(&self, session_id: &str) -> ToolResult<bool> {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            session.kill()?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Kill all sessions (used on registry cleanup).
    pub fn clear(&self) {
        for entry in self.sessions.iter() {
            let _ = entry.value().kill();
        }
        self.sessions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_validation() {
        let store = BackgroundShellStore::new(None);
        assert!(store.spawn("", None).is_err());
    }

    #[test]
    fn test_kill_missing_session() {
        let store = BackgroundShellStore::new(None);
        assert!(!store.kill("nope").unwrap());
    }

    #[test]
    fn test_output_buffer_incremental() {
        let mut buf = OutputBuffer::default();
        buf.append("hello ");
        assert_eq!(buf.read_new(), "hello ");
        assert_eq!(buf.read_new(), "");
        buf.append("world");
        assert_eq!(buf.read_new(), "world");
        assert_eq!(buf.snapshot(), "hello world");
    }

    #[test]
    fn test_output_buffer_peek() {
        let mut buf = OutputBuffer::default();
        buf.append("line1\n");
        assert_eq!(buf.peek_new(), "line1\n");
        // Peek does not advance the cursor.
        assert_eq!(buf.peek_new(), "line1\n");
        assert_eq!(buf.read_new(), "line1\n");
        assert_eq!(buf.peek_new(), "");
    }

    #[test]
    fn test_output_buffer_truncation_resets_cursor() {
        let mut buf = OutputBuffer::default();
        let big = "x".repeat(MAX_OUTPUT_BYTES);
        buf.append(&big);
        buf.append("tail");
        let text = buf.snapshot();
        assert!(text.contains("truncated"));
        assert!(text.ends_with("tail"));
        // After truncation the cursor points at the start, so a reader sees
        // the (truncated) current content.
        assert_eq!(buf.read_new(), text);
    }
}
