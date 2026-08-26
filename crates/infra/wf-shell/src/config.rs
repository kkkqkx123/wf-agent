//! Shared shell configuration.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::event_sink::ShellEventSink;
use crate::shell_detector::ShellType;

pub const DEFAULT_MAX_TIMEOUT_MS: u64 = 600_000;
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;
pub const DEFAULT_GRACEFUL_KILL_TIMEOUT_MS: u64 = 5000;
pub const DEFAULT_PTY_SIZE: (u16, u16) = (24, 80);

/// Default allowlist of common development/shell commands. Commands not on
/// this list are still executed but flagged (approval is an app-layer concern);
/// commands explicitly denied by a configured policy are rejected.
pub const DEFAULT_ALLOWED_COMMANDS: &[&str] = &[
    "git", "ls", "cat", "echo", "pwd", "mkdir", "touch", "cp", "mv", "rm", "grep", "find", "head",
    "tail", "wc", "sort", "uniq", "diff", "sed", "awk", "rg", "make", "cargo", "rustc", "node",
    "npm", "npx", "pnpm", "yarn", "python", "python3", "pip", "curl", "wget", "sh", "bash", "zsh",
];

#[derive(Clone)]
pub struct ShellToolConfig {
    pub workspace_dir: Option<PathBuf>,
    pub max_timeout_ms: u64,
    pub allowed_commands: Vec<String>,
    pub denied_commands: Option<Vec<String>>,
    /// Explicit shell override. When `None`, the platform default is detected
    /// via `$SHELL` / `which`.
    pub shell_type: Option<ShellType>,
    /// Whether interactive sessions may use the PTY backend. When `false`,
    /// sessions run on the pipe backend regardless of the session request.
    pub pty_enabled: bool,
    /// Default terminal size (rows, cols) for PTY sessions.
    pub default_pty_size: (u16, u16),
    /// How long a graceful kill waits after SIGTERM before forcing SIGKILL.
    pub graceful_kill_timeout_ms: u64,
    /// Base environment merged into every command (session/command env wins).
    pub default_env: HashMap<String, String>,
    /// Whether idle sessions are reused across calls (get_or_create_shell).
    pub session_reuse_enabled: bool,
    /// Optional cap on sessions bound to a single task id.
    pub max_sessions_per_task: Option<usize>,
    /// Optional idle session auto-release (lazy sweep on session creation).
    pub session_idle_timeout_ms: Option<u64>,
    /// Whether per-line output events are dispatched to the event sink.
    pub output_event_enabled: bool,
    /// Event sink receiving shell session lifecycle/output events; the
    /// default is `None` (no events are emitted).
    pub event_sink: Option<Arc<dyn ShellEventSink>>,
    /// Optional sandbox policy applied to every spawned command. When set,
    /// commands run through the shared `wf-sandbox` execution gateway:
    /// seccomp-bpf (AUDIT_ARCH validated) + rlimits + env clearing. The
    /// PTY backend cannot attach `pre_exec` hooks and therefore skips the
    /// kernel-level enforcement (it still inherits the command-safety gate).
    pub sandbox_policy: Option<wf_types::script::sandbox::SandboxPolicy>,
}

impl Default for ShellToolConfig {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            max_timeout_ms: DEFAULT_MAX_TIMEOUT_MS,
            allowed_commands: DEFAULT_ALLOWED_COMMANDS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            denied_commands: None,
            shell_type: None,
            pty_enabled: true,
            default_pty_size: DEFAULT_PTY_SIZE,
            graceful_kill_timeout_ms: DEFAULT_GRACEFUL_KILL_TIMEOUT_MS,
            default_env: HashMap::new(),
            session_reuse_enabled: true,
            max_sessions_per_task: None,
            session_idle_timeout_ms: None,
            output_event_enabled: false,
            event_sink: None,
            sandbox_policy: None,
        }
    }
}

impl std::fmt::Debug for ShellToolConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellToolConfig")
            .field("workspace_dir", &self.workspace_dir)
            .field("max_timeout_ms", &self.max_timeout_ms)
            .field("allowed_commands", &self.allowed_commands)
            .field("denied_commands", &self.denied_commands)
            .field("shell_type", &self.shell_type)
            .field("pty_enabled", &self.pty_enabled)
            .field("default_pty_size", &self.default_pty_size)
            .field("graceful_kill_timeout_ms", &self.graceful_kill_timeout_ms)
            .field("default_env", &self.default_env)
            .field("session_reuse_enabled", &self.session_reuse_enabled)
            .field("max_sessions_per_task", &self.max_sessions_per_task)
            .field("session_idle_timeout_ms", &self.session_idle_timeout_ms)
            .field("output_event_enabled", &self.output_event_enabled)
            .field(
                "event_sink",
                &self.event_sink.as_ref().map(|_| "<shell event sink>"),
            )
            .finish()
    }
}
