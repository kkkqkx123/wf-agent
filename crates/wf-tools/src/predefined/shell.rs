//! Predefined shell tools: definitions + background shell engine.
//!
//! Tools: execute_command (stateless), backend_shell / shell_output /
//! shell_kill (stateful background shell sessions).

use serde_json::Value;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use wf_types::tool::ToolType;

use super::schema::{ToolDefinition, ToolParameter};
use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::registry::ToolRegistry;
use crate::shell::execute_command_handler;
use crate::shell::ShellToolConfig;

pub static EXECUTE_COMMAND: ToolDefinition = ToolDefinition {
    id: "execute_command",
    tool_type: ToolType::Stateless,
    category: "shell",
    tags: &["shell", "command"],
    description: "Execute a shell command and capture its output. Supports configurable timeout and working directory.",
    parameters: &[
        ToolParameter { name: "command", r#type: "string", required: true, description: "The shell command to execute", default_json: None },
        ToolParameter { name: "timeout", r#type: "number", required: false, description: "Timeout in milliseconds", default_json: Some("120000") },
        ToolParameter { name: "cwd", r#type: "string", required: false, description: "Working directory for the command", default_json: None },
    ],
    tips: Some(&["Use absolute paths for safety", "Avoid interactive commands"]),
    examples: Some(&["execute_command(\"cargo build\")"]),
};

pub static BACKEND_SHELL: ToolDefinition = ToolDefinition {
    id: "backend_shell",
    tool_type: ToolType::Stateful,
    category: "shell",
    tags: &["backend", "shell"],
    description: "Start a long-running shell command in the background. Returns a session_id for subsequent operations with shell_output and shell_kill.",
    parameters: &[
        ToolParameter { name: "command", r#type: "string", required: true, description: "The command to start in the background", default_json: None },
        ToolParameter { name: "cwd", r#type: "string", required: false, description: "Working directory", default_json: None },
    ],
    tips: Some(&["Use for long-running processes like dev servers"]),
    examples: Some(&["backend_shell(\"npm run dev\")"]),
};

pub static SHELL_OUTPUT: ToolDefinition = ToolDefinition {
    id: "shell_output",
    tool_type: ToolType::Stateful,
    category: "shell",
    tags: &["output"],
    description: "Retrieve output from a running background shell session by session_id.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID returned by backend_shell", default_json: None },
    ],
    tips: None,
    examples: Some(&["shell_output(\"abc123\")"]),
};

pub static SHELL_KILL: ToolDefinition = ToolDefinition {
    id: "shell_kill",
    tool_type: ToolType::Stateful,
    category: "shell",
    tags: &["kill"],
    description: "Kill a running background shell session by session_id.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID to kill", default_json: None },
    ],
    tips: None,
    examples: Some(&["shell_kill(\"abc123\")"]),
};

/// All shell tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[
    &EXECUTE_COMMAND,
    &BACKEND_SHELL,
    &SHELL_OUTPUT,
    &SHELL_KILL,
];

// ── Background shell engine ────────────────────────────────

const MAX_OUTPUT_BYTES: usize = 256_000;
const MAX_SESSIONS: usize = 64;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A single background shell session.
pub struct ShellSession {
    command: String,
    start_time: Instant,
    child: Mutex<Option<Child>>,
    output: Arc<Mutex<String>>,
    killed: AtomicBool,
}

impl ShellSession {
    fn new(command: String, child: Child, output: Arc<Mutex<String>>) -> Self {
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
    fn append_output(buf: &Arc<Mutex<String>>, chunk: String) {
        let mut out = buf.lock().unwrap();
        if out.len() + chunk.len() > MAX_OUTPUT_BYTES {
            let keep = MAX_OUTPUT_BYTES.saturating_sub(chunk.len() + 64);
            if out.len() > keep {
                let cut = out.len() - keep;
                *out = format!("(output truncated, {} bytes omitted)\n{}", cut, &out[cut..]);
            }
        }
        out.push_str(&chunk);
    }

    /// Read the child's stdout/stderr on a background thread until EOF.
    fn spawn_output_reader<R>(pipe: R, output: Arc<Mutex<String>>)
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

    fn status(&self) -> (String, Option<i32>) {
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
        let output = self.output.lock().unwrap().clone();
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
}

/// Shared store of background shell sessions across the three tools.
pub struct BackgroundShellStore {
    sessions: dashmap::DashMap<String, Arc<ShellSession>>,
    default_cwd: Option<PathBuf>,
}

impl BackgroundShellStore {
    pub fn new(default_cwd: Option<PathBuf>) -> Self {
        Self {
            sessions: dashmap::DashMap::new(),
            default_cwd,
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

        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        if let Some(dir) = &cwd {
            cmd.current_dir(dir);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| ToolError::ExecutionError(format!("Failed to spawn command: {}", e)))?;

        let output = Arc::new(Mutex::new(String::new()));
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

// ── Stateful instances ─────────────────────────────────────

/// Stateful instance for the backend_shell tool: spawns a session on the
/// first call and returns its session_id. Destroys the session on cleanup.
struct BackendShellInstance {
    store: Arc<BackgroundShellStore>,
    execution_id: String,
    session_id: Arc<Mutex<Option<String>>>,
}

impl StatefulInstance for BackendShellInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'command' parameter".into())
            })?;
        let cwd = params.get("cwd").and_then(|v| v.as_str());

        let session_id = self.store.spawn(command, cwd)?;
        *self.session_id.lock().unwrap() = Some(session_id.clone());
        Ok(serde_json::json!({
            "session_id": session_id,
            "status": "started",
            "execution_id": self.execution_id,
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        if let Some(session_id) = self.session_id.lock().unwrap().clone() {
            let _ = self.store.kill(&session_id);
        }
        Ok(())
    }
}

/// Stateful instance for the shell_output tool.
struct ShellOutputInstance {
    store: Arc<BackgroundShellStore>,
}

impl StatefulInstance for ShellOutputInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let session = self.store.get(session_id).ok_or_else(|| {
            ToolError::NotFound(format!("No background shell session '{}'", session_id))
        })?;
        let mut value = session.snapshot();
        value["session_id"] = Value::String(session_id.into());
        Ok(value)
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Stateful instance for the shell_kill tool.
struct ShellKillInstance {
    store: Arc<BackgroundShellStore>,
}

impl StatefulInstance for ShellKillInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let killed = self.store.kill(session_id)?;
        Ok(serde_json::json!({ "session_id": session_id, "killed": killed }))
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register shell handlers: execute_command (stateless) plus the background
/// shell stateful factories.
pub fn register(registry: &ToolRegistry, config: &ShellToolConfig) -> ToolResult<()> {
    let shell_handler = execute_command_handler(config.clone());
    registry.register_stateless_async_handler("execute_command", shell_handler);

    let store = Arc::new(BackgroundShellStore::new(config.workspace_dir.clone()));

    let backend_store = store.clone();
    registry.register_stateful_factory("backend_shell", Arc::new(move |execution_id| {
        Box::new(BackendShellInstance {
            store: backend_store.clone(),
            execution_id: execution_id.to_string(),
            session_id: Arc::new(Mutex::new(None)),
        })
    }));

    let output_store = store.clone();
    registry.register_stateful_factory("shell_output", Arc::new(move |_execution_id| {
        Box::new(ShellOutputInstance {
            store: output_store.clone(),
        })
    }));

    let kill_store = store;
    registry.register_stateful_factory("shell_kill", Arc::new(move |_execution_id| {
        Box::new(ShellKillInstance {
            store: kill_store.clone(),
        })
    }));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::trait_def::ToolExecutionContext;

    #[tokio::test]
    async fn test_backend_shell_lifecycle() {
        let registry = ToolRegistry::new();
        register(&registry, &ShellToolConfig::default()).unwrap();

        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let tool = BACKEND_SHELL.tool_def();
        registry.register_tool(tool.clone());
        registry.register_tool(SHELL_OUTPUT.tool_def());
        registry.register_tool(SHELL_KILL.tool_def());

        let result = registry
            .execute_tool(
                "backend_shell",
                &serde_json::json!({ "command": "echo hello-backend" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success);
        let session_id = result
            .result
            .and_then(|v| v.get("session_id").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .unwrap();

        // Wait briefly for the command to finish writing output.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let output = registry
            .execute_tool(
                "shell_output",
                &serde_json::json!({ "session_id": session_id }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(output.success);
        let text = output
            .result
            .and_then(|v| v.get("output").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        assert!(text.contains("hello-backend"), "output was: {}", text);

        let killed = registry
            .execute_tool(
                "shell_kill",
                &serde_json::json!({ "session_id": session_id }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(killed.success);
        assert_eq!(
            killed.result.unwrap()["killed"],
            serde_json::json!(true)
        );
    }

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
}
