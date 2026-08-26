//! Session backends.
//!
//! A session runs on one of two subprocess backends: a plain pipe backend
//! (std child with piped stdin/stdout/stderr) and a real terminal via
//! `portable-pty` (always compiled in; whether it is used at runtime is
//! governed by the store's `pty_enabled` config). The kill helpers here
//! terminate a child and its whole process group (SIGTERM then SIGKILL),
//! shared by the stateless runner ([`crate::spawn`]) and the session engine.

use std::io::Write;
use std::process::{Child, ChildStdin};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{ShellError, ShellResult};

/// Terminal mode of a background session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMode {
    Pipe,
    Pty,
}

impl SessionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionMode::Pipe => "pipe",
            SessionMode::Pty => "pty",
        }
    }
}

/// Pipe-backed session: a std child process whose stdin is piped so that
/// [`crate::session::ShellSession::write_input`] can feed it. stdout/stderr are
/// captured by two background reader threads; bytes are preserved verbatim (no
/// CRLF normalization).
pub(crate) struct PipeBackend {
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
}

impl PipeBackend {
    pub(crate) fn new(child: Child, stdin: ChildStdin) -> Self {
        Self {
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(Some(stdin)),
        }
    }

    fn status(&self) -> (String, Option<i32>) {
        let mut child = wf_common::lock::lock_ok(self.child.lock());
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code();
                    *child = None;
                    *wf_common::lock::lock_ok(self.stdin.lock()) = None;
                    ("exited".into(), code)
                }
                Ok(None) => ("running".into(), None),
                Err(_) => ("unknown".into(), None),
            }
        } else {
            ("exited".into(), None)
        }
    }

    fn pid(&self) -> Option<u32> {
        wf_common::lock::lock_ok(self.child.lock())
            .as_ref()
            .map(|c| c.id())
    }

    fn write_input(&self, data: &str, enter: bool) -> ShellResult<()> {
        let mut stdin = wf_common::lock::lock_ok(self.stdin.lock());
        let handle = stdin
            .as_mut()
            .ok_or_else(|| ShellError::ExecutionError("Session stdin is not available".into()))?;
        let mut bytes = data.as_bytes().to_vec();
        if enter {
            bytes.push(b'\n');
        }
        handle
            .write_all(&bytes)
            .map_err(|e| ShellError::ExecutionError(format!("Failed to write stdin: {}", e)))
    }

    fn kill(&self, graceful: bool, timeout_ms: u64) -> ShellResult<()> {
        let mut child = wf_common::lock::lock_ok(self.child.lock());
        if let Some(c) = child.as_mut() {
            if graceful {
                graceful_kill_child(c, timeout_ms)?;
            } else {
                let _ = c.kill();
                let _ = c.wait();
            }
            *child = None;
            *wf_common::lock::lock_ok(self.stdin.lock()) = None;
        }
        Ok(())
    }
}

/// PTY-backed session: a real terminal via `portable-pty`. Output is a single
/// merged stream (stdout + stderr) normalized to `\n`; input goes to the
/// master writer; resize updates the terminal window size.
pub(crate) struct PtyBackend {
    child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
    master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
}

impl PtyBackend {
    pub(crate) fn new(
        child: Box<dyn portable_pty::Child + Send + Sync>,
        master: Box<dyn portable_pty::MasterPty + Send>,
        writer: Box<dyn Write + Send>,
    ) -> Self {
        Self {
            child: Mutex::new(Some(child)),
            master: Mutex::new(Some(master)),
            writer: Mutex::new(Some(writer)),
        }
    }

    fn status(&self) -> (String, Option<i32>) {
        let mut child = wf_common::lock::lock_ok(self.child.lock());
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(status)) => {
                    let code = Some(status.exit_code() as i32);
                    *child = None;
                    ("exited".into(), code)
                }
                Ok(None) => ("running".into(), None),
                Err(_) => ("unknown".into(), None),
            }
        } else {
            ("exited".into(), None)
        }
    }

    fn pid(&self) -> Option<u32> {
        self.child
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.process_id())
    }

    fn write_input(&self, data: &str, enter: bool) -> ShellResult<()> {
        let mut writer = wf_common::lock::lock_ok(self.writer.lock());
        let handle = writer
            .as_mut()
            .ok_or_else(|| ShellError::ExecutionError("PTY writer is not available".into()))?;
        let mut bytes = data.as_bytes().to_vec();
        if enter {
            bytes.push(b'\n');
        }
        handle
            .write_all(&bytes)
            .map_err(|e| ShellError::ExecutionError(format!("Failed to write to PTY: {}", e)))?;
        handle
            .flush()
            .map_err(|e| ShellError::ExecutionError(format!("Failed to flush PTY: {}", e)))
    }

    fn resize(&self, rows: u16, cols: u16) -> ShellResult<()> {
        let master = wf_common::lock::lock_ok(self.master.lock());
        let master = master
            .as_ref()
            .ok_or_else(|| ShellError::ExecutionError("PTY master is not available".into()))?;
        master
            .resize(portable_pty::PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| ShellError::ExecutionError(format!("Failed to resize PTY: {}", e)))?;
        Ok(())
    }

    fn kill(&self, graceful: bool, timeout_ms: u64) -> ShellResult<()> {
        let mut child = wf_common::lock::lock_ok(self.child.lock());
        if let Some(c) = child.as_mut() {
            if graceful {
                graceful_kill_pty(&mut **c, timeout_ms)?;
            } else {
                let _ = c.kill();
                let _ = c.wait();
            }
            *child = None;
            *wf_common::lock::lock_ok(self.writer.lock()) = None;
            *wf_common::lock::lock_ok(self.master.lock()) = None;
        }
        Ok(())
    }
}

/// Backend used by a session: pipe (default) or PTY.
pub(crate) enum Backend {
    Pipe(PipeBackend),
    Pty(PtyBackend),
}

impl Backend {
    pub(crate) fn status(&self) -> (String, Option<i32>) {
        match self {
            Backend::Pipe(b) => b.status(),
            Backend::Pty(b) => b.status(),
        }
    }

    pub(crate) fn pid(&self) -> Option<u32> {
        match self {
            Backend::Pipe(b) => b.pid(),
            Backend::Pty(b) => b.pid(),
        }
    }

    pub(crate) fn write_input(&self, data: &str, enter: bool) -> ShellResult<()> {
        match self {
            Backend::Pipe(b) => b.write_input(data, enter),
            Backend::Pty(b) => b.write_input(data, enter),
        }
    }

    pub(crate) fn resize(&self, rows: u16, cols: u16) -> ShellResult<()> {
        match self {
            Backend::Pipe(_) => {
                let _ = (rows, cols);
                Err(ShellError::ValidationFailed(
                    "Session does not use a PTY; resize is only supported for interactive (PTY) sessions"
                        .into(),
                ))
            }
            Backend::Pty(b) => b.resize(rows, cols),
        }
    }

    pub(crate) fn mode(&self) -> SessionMode {
        match self {
            Backend::Pipe(_) => SessionMode::Pipe,
            Backend::Pty(_) => SessionMode::Pty,
        }
    }

    pub(crate) fn kill(&self, graceful: bool, timeout_ms: u64) -> ShellResult<()> {
        match self {
            Backend::Pipe(b) => b.kill(graceful, timeout_ms),
            Backend::Pty(b) => b.kill(graceful, timeout_ms),
        }
    }
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

/// Gracefully terminate a PTY child (see [`graceful_kill_child`]).
fn graceful_kill_pty(
    child: &mut (dyn portable_pty::Child + Send + Sync),
    timeout_ms: u64,
) -> ShellResult<()> {
    #[cfg(unix)]
    {
        if let Some(pid) = child.process_id() {
            let pid = pid as i32;
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
        } else {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}
