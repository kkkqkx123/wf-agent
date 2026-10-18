use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use wf_common::now;

use crate::infra::context::ApiContext;
use crate::ApiError;

const MAX_BUFFER_BYTES: usize = 2 * 1024 * 1024;
const MAX_EXITED_SESSIONS: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PtySessionStatus {
    Running,
    Exited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtySession {
    pub id: String,
    pub cwd: String,
    pub status: PtySessionStatus,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("pty session not found: {0}")]
    NotFound(String),
    #[error("invalid pty request: {0}")]
    Invalid(String),
    #[error("pty io error: {0}")]
    Io(String),
}

impl From<PtyError> for ApiError {
    fn from(err: PtyError) -> Self {
        match err {
            PtyError::NotFound(msg) => ApiError::NotFound {
                entity_type: "PtySession".to_string(),
                id: msg,
            },
            PtyError::Invalid(msg) => ApiError::Validation(msg),
            PtyError::Io(msg) => ApiError::Validation(msg),
        }
    }
}

pub type PtyResult<T> = Result<T, PtyError>;

pub struct PtyState {
    id: String,
    cwd: String,
    status: Mutex<PtySessionStatus>,
    exit_code: Mutex<Option<i32>>,
    pid: u32,
    rows: Mutex<u16>,
    cols: Mutex<u16>,
    buffer: Mutex<String>,
    buffer_start: Mutex<usize>,
    cursor: Mutex<usize>,
    output_tx: broadcast::Sender<String>,
    child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
    master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    created_at: i64,
}

struct ManagerInner {
    sessions: HashMap<String, Arc<PtyState>>,
}

pub struct PtyManager {
    inner: Mutex<ManagerInner>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ManagerInner {
                sessions: HashMap::new(),
            }),
        }
    }

    pub fn create_with_context(
        &self,
        ctx: Option<&ApiContext>,
        cwd: Option<String>,
        command: Option<String>,
        rows: Option<u16>,
        cols: Option<u16>,
    ) -> PtyResult<PtySession> {
        create_with_context(ctx, cwd, command, rows, cols, &self.inner)
    }

    pub fn list(&self) -> Vec<PtySession> {
        list_sessions(&self.inner)
    }

    pub fn get(&self, id: &str) -> PtyResult<PtySession> {
        get_session(&self.inner, id)
    }

    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> PtyResult<PtySession> {
        resize_session(&self.inner, id, rows, cols)
    }

    pub fn remove(&self, id: &str) -> PtyResult<bool> {
        remove_session(&self.inner, id)
    }

    pub fn attach(
        &self,
        id: &str,
        cursor: Option<i64>,
    ) -> PtyResult<(String, usize, broadcast::Receiver<String>, Arc<PtyState>)> {
        attach_session(&self.inner, id, cursor)
    }

    pub fn write(&self, id: &str, data: &str) -> PtyResult<()> {
        write_session(&self.inner, id, data.as_bytes())
    }

    pub fn write_bytes(&self, id: &str, data: &[u8]) -> PtyResult<()> {
        write_session(&self.inner, id, data)
    }
}

static PTY_MANAGER: std::sync::OnceLock<PtyManager> = std::sync::OnceLock::new();

pub fn global_manager() -> &'static PtyManager {
    PTY_MANAGER.get_or_init(PtyManager::new)
}

fn lock_inner(inner: &Mutex<ManagerInner>) -> std::sync::MutexGuard<'_, ManagerInner> {
    match inner.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

fn create_with_context(
    ctx: Option<&ApiContext>,
    cwd: Option<String>,
    command: Option<String>,
    rows: Option<u16>,
    cols: Option<u16>,
    inner: &Mutex<ManagerInner>,
) -> PtyResult<PtySession> {
    let rows = rows.unwrap_or(24).max(1);
    let cols = cols.unwrap_or(80).max(1);

    let cwd_path = resolve_cwd(ctx, cwd)?;
    if !cwd_path.exists() {
        return Err(PtyError::Invalid(format!(
            "working directory does not exist: {}",
            cwd_path.display()
        )));
    }

    let shell = command
        .clone()
        .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()));

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| PtyError::Io(format!("open pty: {e}")))?;

    let mut cmd = CommandBuilder::new(shell.clone());
    if command.is_none() {
        cmd.arg("-l");
    }
    cmd.cwd(&cwd_path);
    cmd.env("TERM", "xterm-256color");
    cmd.env("LANG", "C.UTF-8");

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| PtyError::Io(format!("spawn pty: {e}")))?;
    let pid = child.process_id().unwrap_or(0);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| PtyError::Io(format!("clone reader: {e}")))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| PtyError::Io(format!("take writer: {e}")))?;
    let master = pair.master;

    let (tx, _) = broadcast::channel::<String>(256);
    let id = format!("pty-{}", wf_common::generate_id());
    let state = Arc::new(PtyState {
        id: id.clone(),
        cwd: cwd_path.to_string_lossy().to_string(),
        status: Mutex::new(PtySessionStatus::Running),
        exit_code: Mutex::new(None),
        pid,
        rows: Mutex::new(rows),
        cols: Mutex::new(cols),
        buffer: Mutex::new(String::new()),
        buffer_start: Mutex::new(0),
        cursor: Mutex::new(0),
        output_tx: tx.clone(),
        child: Mutex::new(Some(child)),
        master: Mutex::new(Some(master)),
        writer: Mutex::new(Some(writer)),
        created_at: now(),
    });

    // reader thread
    let state_r = state.clone();
    let tx_r = tx.clone();
    thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    append_output(&state_r, &text);
                    let _ = tx_r.send(text);
                }
                Err(_) => break,
            }
        }
        let mut st = lock(&state_r.status);
        if *st == PtySessionStatus::Running {
            *st = PtySessionStatus::Exited;
        }
    });

    // waiter thread
    let state_w = state.clone();
    thread::spawn(move || {
        loop {
            let mut child_guard = lock(&state_w.child);
            if let Some(child) = child_guard.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        *lock(&state_w.exit_code) = Some(status.exit_code() as i32);
                        *lock(&state_w.status) = PtySessionStatus::Exited;
                        *child_guard = None;
                        drop(child_guard);
                        *lock(&state_w.master) = None;
                        *lock(&state_w.writer) = None;
                        break;
                    }
                    Ok(None) => {
                        drop(child_guard);
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(_) => {
                        drop(child_guard);
                        break;
                    }
                }
            } else {
                if *lock(&state_w.status) != PtySessionStatus::Exited {
                    *lock(&state_w.status) = PtySessionStatus::Exited;
                }
                break;
            }
        }
    });

    // retention cleanup
    {
        let mut mgr = lock_inner(inner);
        let mut exited: Vec<(String, i64)> = mgr
            .sessions
            .iter()
            .filter(|(_, s)| *lock(&s.status) == PtySessionStatus::Exited)
            .map(|(id, s)| (id.clone(), s.created_at))
            .collect();
        if exited.len() >= MAX_EXITED_SESSIONS {
            exited.sort_by_key(|(_, t)| *t);
            if let Some((old, _)) = exited.first() {
                if let Some(st) = mgr.sessions.remove(old) {
                    let _ = kill_state(&st);
                }
            }
        }
        mgr.sessions.insert(id.clone(), state.clone());
    }

    Ok(session_view(&state))
}

fn resolve_cwd(ctx: Option<&ApiContext>, cwd: Option<String>) -> PtyResult<PathBuf> {
    if let Some(cwd) = cwd {
        let p = PathBuf::from(&cwd);
        if let Some(ctx) = ctx {
            if let Some(cfg) = ctx.user_git_config() {
                if let Some(root) = &cfg.repo_root {
                    let root = PathBuf::from(root);
                    if !p.starts_with(&root) {
                        return Err(PtyError::Invalid(format!(
                            "cwd escapes repository root: {cwd}"
                        )));
                    }
                }
            }
        }
        Ok(p)
    } else if let Some(ctx) = ctx {
        match crate::git::repo_root(ctx) {
            Ok(p) => Ok(p),
            Err(_) => std::env::current_dir().map_err(|e| PtyError::Io(e.to_string())),
        }
    } else {
        std::env::current_dir().map_err(|e| PtyError::Io(e.to_string()))
    }
}

fn append_output(state: &PtyState, text: &str) {
    let mut buf = lock(&state.buffer);
    let mut start = lock(&state.buffer_start);
    let mut cursor = lock(&state.cursor);
    buf.push_str(text);
    *cursor += text.len();
    if buf.len() > MAX_BUFFER_BYTES {
        let excess = buf.len() - MAX_BUFFER_BYTES;
        let mut trim = excess;
        while trim > 0 && !buf.is_char_boundary(trim) {
            trim -= 1;
        }
        if trim > 0 {
            buf.drain(..trim);
            *start += trim;
        }
    }
}

fn session_view(state: &PtyState) -> PtySession {
    PtySession {
        id: state.id.clone(),
        cwd: state.cwd.clone(),
        status: *lock(&state.status),
        exit_code: *lock(&state.exit_code),
        pid: Some(state.pid),
        rows: *lock(&state.rows),
        cols: *lock(&state.cols),
    }
}

fn list_sessions(inner: &Mutex<ManagerInner>) -> Vec<PtySession> {
    let mgr = lock_inner(inner);
    let mut out: Vec<PtySession> = mgr
        .sessions
        .values()
        .map(|s| session_view(s))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn get_session(inner: &Mutex<ManagerInner>, id: &str) -> PtyResult<PtySession> {
    let mgr = lock_inner(inner);
    let state = mgr.sessions.get(id).cloned().ok_or_else(|| PtyError::NotFound(id.to_string()))?;
    drop(mgr);
    Ok(session_view(&state))
}

fn resize_session(inner: &Mutex<ManagerInner>, id: &str, rows: u16, cols: u16) -> PtyResult<PtySession> {
    let mgr = lock_inner(inner);
    let state = mgr.sessions.get(id).cloned().ok_or_else(|| PtyError::NotFound(id.to_string()))?;
    drop(mgr);
    let rows = rows.max(1);
    let cols = cols.max(1);
    {
        let master = lock(&state.master);
        if let Some(m) = master.as_ref() {
            let _ = m.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }
    *lock(&state.rows) = rows;
    *lock(&state.cols) = cols;
    Ok(session_view(&state))
}

fn remove_session(inner: &Mutex<ManagerInner>, id: &str) -> PtyResult<bool> {
    let mut mgr = lock_inner(inner);
    if let Some(state) = mgr.sessions.remove(id) {
        drop(mgr);
        let _ = kill_state(&state);
        Ok(true)
    } else {
        Err(PtyError::NotFound(id.to_string()))
    }
}

fn attach_session(
    inner: &Mutex<ManagerInner>,
    id: &str,
    cursor: Option<i64>,
) -> PtyResult<(String, usize, broadcast::Receiver<String>, Arc<PtyState>)> {
    let mgr = lock_inner(inner);
    let state = mgr.sessions.get(id).cloned().ok_or_else(|| PtyError::NotFound(id.to_string()))?;
    drop(mgr);

    let replay = {
        let buf = lock(&state.buffer);
        let start = *lock(&state.buffer_start);
        let end = *lock(&state.cursor);
        if cursor == Some(-1) {
            String::new()
        } else {
            let req = cursor.unwrap_or(0);
            let req = if req < 0 { 0usize } else { req as usize };
            if req <= start {
                buf.clone()
            } else if req >= end {
                String::new()
            } else {
                buf[req - start..].to_string()
            }
        }
    };
    let cursor_out = *lock(&state.cursor);
    let rx = state.output_tx.subscribe();
    Ok((replay, cursor_out, rx, state))
}

fn write_session(inner: &Mutex<ManagerInner>, id: &str, data: &[u8]) -> PtyResult<()> {
    let mgr = lock_inner(inner);
    let state = mgr.sessions.get(id).cloned().ok_or_else(|| PtyError::NotFound(id.to_string()))?;
    drop(mgr);
    let mut writer = lock(&state.writer);
    if let Some(w) = writer.as_mut() {
        w.write_all(data).map_err(|e| PtyError::Io(e.to_string()))?;
        w.flush().map_err(|e| PtyError::Io(e.to_string()))?;
        Ok(())
    } else {
        Err(PtyError::Io("writer closed".to_string()))
    }
}

fn kill_state(state: &PtyState) -> PtyResult<()> {
    let mut child = lock(&state.child);
    if let Some(c) = child.as_mut() {
        let _ = c.kill();
        let _ = c.wait();
    }
    *child = None;
    drop(child);
    *lock(&state.master) = None;
    *lock(&state.writer) = None;
    *lock(&state.status) = PtySessionStatus::Exited;
    Ok(())
}

// Top-level convenience functions using global manager.

pub fn create_pty_session(
    ctx: &ApiContext,
    cwd: Option<String>,
    command: Option<String>,
    rows: Option<u16>,
    cols: Option<u16>,
) -> PtyResult<PtySession> {
    global_manager().create_with_context(Some(ctx), cwd, command, rows, cols)
}

pub fn list_pty_sessions() -> Vec<PtySession> {
    global_manager().list()
}

pub fn get_pty_session(id: &str) -> PtyResult<PtySession> {
    global_manager().get(id)
}

pub fn resize_pty_session(id: &str, rows: u16, cols: u16) -> PtyResult<PtySession> {
    global_manager().resize(id, rows, cols)
}

pub fn remove_pty_session(id: &str) -> PtyResult<bool> {
    global_manager().remove(id)
}

pub fn attach_pty_session(
    id: &str,
    cursor: Option<i64>,
) -> PtyResult<(String, usize, broadcast::Receiver<String>, Arc<PtyState>)> {
    global_manager().attach(id, cursor)
}

pub fn write_pty_session(id: &str, data: &str) -> PtyResult<()> {
    global_manager().write(id, data)
}

pub fn write_pty_bytes(id: &str, data: &[u8]) -> PtyResult<()> {
    global_manager().write_bytes(id, data)
}

