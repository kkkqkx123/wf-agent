use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use wf_common::now;
use wf_core::EventBus;

use crate::infra::context::ApiContext;
use crate::{ApiError, BaseEvent, EventType};

const MAX_BUFFER_BYTES: usize = 2 * 1024 * 1024;
const MAX_EXITED_SESSIONS: usize = 25;
/// Per-subscriber backlog ceiling. Past this the WS consumer is considered
/// too slow and is evicted with an `Overflow` frame; the client reconnects
/// with its cursor and continues from the retained replay buffer.
const SUBSCRIBER_MAX_QUEUED_BYTES: usize = 4 * 1024 * 1024;
/// No-subscriber idle window after which a session is reclaimed.
const IDLE_TIMEOUT_MS: i64 = 600_000;
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

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
    pub title: String,
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

/// One item delivered to a single WS connection.
#[derive(Debug, Clone)]
pub enum PtyFrame {
    /// Raw terminal output.
    Data(String),
    /// Session stopped producing output; carries the exit code when known.
    Exited {
        exit_code: Option<i32>,
        reason: &'static str,
    },
    /// Backlog ceiling exceeded; the subscriber was evicted.
    Overflow,
}

struct Subscriber {
    tx: mpsc::UnboundedSender<PtyFrame>,
    /// Bytes queued and not yet consumed by the WS writer.
    queued: Arc<AtomicUsize>,
}

/// Output state guarded by one lock: buffer, absolute cursor window and the
/// subscriber table. Replay computation and subscription happen in the same
/// critical section so replay and live frames never overlap or drop a chunk.
struct Output {
    buffer: String,
    /// Absolute cursor pointing at the first byte still retained in `buffer`.
    start: usize,
    /// Absolute cursor pointing just past the last produced byte.
    cursor: usize,
    subscribers: HashMap<u64, Subscriber>,
    next_token: u64,
    /// Set once when the session stops: late attaches replay and receive the
    /// same `Exited` frame live subscribers get.
    ended: Option<(Option<i32>, &'static str)>,
}

pub struct PtyState {
    id: String,
    cwd: String,
    title: Mutex<String>,
    pid: u32,
    status: Mutex<PtySessionStatus>,
    exit_code: Mutex<Option<i32>>,
    rows: Mutex<u16>,
    cols: Mutex<u16>,
    output: Mutex<Output>,
    master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    created_at: i64,
    last_activity: AtomicI64,
    /// External kill request (removal or idle reclaim); the exit thread owns
    /// the child handle and performs the kill.
    kill_request: AtomicBool,
    kill_reason: Mutex<Option<&'static str>>,
    /// One-shot guard so only the first exit observation announces.
    exit_announced: AtomicBool,
    event_bus: Option<Arc<EventBus>>,
}

impl PtyState {
    fn touch(&self) {
        self.last_activity.store(now(), Ordering::Relaxed);
    }
}

/// Removes the subscriber entry when the receiver drops, covering every WS
/// exit path (normal close, write error, task abort, panic unwind).
struct SubscriberGuard {
    id: String,
    token: u64,
}

impl Drop for SubscriberGuard {
    fn drop(&mut self) {
        detach_session(&global_manager().inner, &self.id, self.token);
    }
}

/// Receiver side of one attachment. Credit is returned on consumption so the
/// producing reader thread can police the per-subscriber backlog.
pub struct PtyReceiver {
    rx: mpsc::UnboundedReceiver<PtyFrame>,
    queued: Arc<AtomicUsize>,
    _guard: Option<SubscriberGuard>,
}

impl PtyReceiver {
    pub async fn recv(&mut self) -> Option<PtyFrame> {
        self.rx.recv().await
    }

    /// Acknowledge `bytes` of `Data` frames handed to the WS writer.
    pub fn credit(&self, bytes: usize) {
        self.queued.fetch_sub(bytes, Ordering::Relaxed);
    }
}

pub struct PtyAttachment {
    /// Retained output from the requested cursor to `cursor`.
    pub replay: String,
    /// Absolute cursor after the replay snapshot.
    pub cursor: usize,
    pub rx: PtyReceiver,
}

struct ManagerInner {
    sessions: HashMap<String, Arc<PtyState>>,
}

pub struct PtyManager {
    inner: Mutex<ManagerInner>,
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ManagerInner {
                sessions: HashMap::new(),
            }),
        }
    }

    /// Start the idle/retention sweeper once. Safe with `&'static self`
    /// because the manager lives in a `OnceLock` for the whole process.
    fn ensure_reaper(&'static self) {
        static REAPER: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        REAPER.get_or_init(|| {
            let inner = &self.inner;
            thread::spawn(move || loop {
                thread::sleep(SWEEP_INTERVAL);
                sweep(inner);
            });
        });
    }

    pub fn create_with_context(
        &self,
        ctx: Option<&ApiContext>,
        cwd: Option<String>,
        command: Option<String>,
        rows: Option<u16>,
        cols: Option<u16>,
        title: Option<String>,
    ) -> PtyResult<PtySession> {
        create_with_context(ctx, cwd, command, rows, cols, title, &self.inner)
    }

    pub fn list(&self) -> Vec<PtySession> {
        list_sessions(&self.inner)
    }

    pub fn get(&self, id: &str) -> PtyResult<PtySession> {
        get_session(&self.inner, id)
    }

    pub fn update(
        &self,
        id: &str,
        title: Option<String>,
        rows: Option<u16>,
        cols: Option<u16>,
    ) -> PtyResult<PtySession> {
        update_session(&self.inner, id, title, rows, cols)
    }

    pub fn remove(&self, id: &str) -> PtyResult<bool> {
        remove_session(&self.inner, id)
    }

    pub fn attach(&self, id: &str, cursor: Option<i64>) -> PtyResult<PtyAttachment> {
        attach_session(&self.inner, id, cursor)
    }

    pub fn write(&self, id: &str, data: &[u8]) -> PtyResult<()> {
        write_session(&self.inner, id, data)
    }
}

static PTY_MANAGER: std::sync::OnceLock<PtyManager> = std::sync::OnceLock::new();

pub fn global_manager() -> &'static PtyManager {
    let manager = PTY_MANAGER.get_or_init(PtyManager::new);
    manager.ensure_reaper();
    manager
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
    title: Option<String>,
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

    let id = format!("pty-{}", wf_common::generate_id());
    let state = Arc::new(PtyState {
        id: id.clone(),
        cwd: cwd_path.to_string_lossy().to_string(),
        title: Mutex::new(title.unwrap_or_default()),
        status: Mutex::new(PtySessionStatus::Running),
        exit_code: Mutex::new(None),
        pid,
        rows: Mutex::new(rows),
        cols: Mutex::new(cols),
        output: Mutex::new(Output {
            buffer: String::new(),
            start: 0,
            cursor: 0,
            subscribers: HashMap::new(),
            next_token: 1,
            ended: None,
        }),
        master: Mutex::new(Some(master)),
        writer: Mutex::new(Some(writer)),
        created_at: now(),
        last_activity: AtomicI64::new(now()),
        kill_request: AtomicBool::new(false),
        kill_reason: Mutex::new(None),
        exit_announced: AtomicBool::new(false),
        event_bus: ctx.map(|c| c.event_bus.clone()),
    });

    // Reader thread: terminal output into the replay buffer + live delivery.
    let state_r = state.clone();
    thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 8192];
        let mut carry: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    if !carry.is_empty() {
                        let text = String::from_utf8_lossy(&carry).to_string();
                        carry.clear();
                        if !text.is_empty() {
                            append_output(&state_r, text);
                        }
                    }
                    break;
                }
                Ok(n) => {
                    carry.extend_from_slice(&buf[..n]);
                    let text = decode_complete(&mut carry);
                    if text.is_empty() {
                        state_r.touch();
                    } else {
                        append_output(&state_r, text);
                    }
                }
                Err(_) => {
                    if !carry.is_empty() {
                        let text = String::from_utf8_lossy(&carry).to_string();
                        carry.clear();
                        if !text.is_empty() {
                            append_output(&state_r, text);
                        }
                    }
                    break;
                }
            }
        }
    });

    // Exit thread: owns the child handle. Kills on request, reaps once, and
    // funnels every termination through `finalize_exit`.
    let state_e = state.clone();
    thread::spawn(move || {
        let mut child = child;
        let mut killing = false;
        let code = loop {
            if !killing && state_e.kill_request.load(Ordering::Acquire) {
                killing = true;
                let _ = child.kill();
            }
            match child.try_wait() {
                Ok(Some(status)) => break Some(status.exit_code() as i32),
                Ok(None) => thread::sleep(EXIT_POLL_INTERVAL),
                Err(_) => break None,
            }
        };
        let reason = lock(&state_e.kill_reason).unwrap_or("process");
        finalize_exit(&state_e, code, reason);
        // Release the pty ends so a lingering reader sees EOF promptly.
        *lock(&state_e.master) = None;
        *lock(&state_e.writer) = None;
    });

    lock_inner(inner).sessions.insert(id.clone(), state.clone());

    Ok(session_view(&state))
}

/// Sole exit convergence point. Idempotent: the first caller wins, records the
/// status and exit code, announces to every live subscriber and publishes the
/// domain event. Later observations (e.g. a concurrent EOF after the reaper
/// already reaped the child) must not rewrite the recorded code.
fn finalize_exit(state: &Arc<PtyState>, exit_code: Option<i32>, reason: &'static str) {
    if state.exit_announced.swap(true, Ordering::SeqCst) {
        return;
    }
    *lock(&state.status) = PtySessionStatus::Exited;
    *lock(&state.exit_code) = exit_code;
    let senders: Vec<mpsc::UnboundedSender<PtyFrame>> = {
        let mut out = lock(&state.output);
        out.ended = Some((exit_code, reason));
        out.subscribers.drain().map(|(_, sub)| sub.tx).collect()
    };
    for tx in senders {
        let _ = tx.send(PtyFrame::Exited { exit_code, reason });
    }
    publish_exited(state, exit_code, reason);
}

fn publish_exited(state: &PtyState, exit_code: Option<i32>, reason: &'static str) {
    let Some(bus) = &state.event_bus else { return };
    let mut metadata = HashMap::new();
    metadata.insert("session_id".to_string(), serde_json::json!(state.id));
    metadata.insert("exit_code".to_string(), serde_json::json!(exit_code));
    metadata.insert("reason".to_string(), serde_json::json!(reason));
    let event = BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::PtyExited,
        timestamp: now(),
        workflow_id: None,
        execution_id: None,
        agent_loop_id: None,
        event_name: Some("pty.exited".to_string()),
        metadata: Some(metadata),
    };
    let _ = bus.publish(event);
}

/// Ask the exit thread to kill the child with the given reason.
fn request_kill(state: &PtyState, reason: &'static str) {
    *lock(&state.kill_reason) = Some(reason);
    state.kill_request.store(true, Ordering::Release);
}

/// Periodic reclaim: running sessions nobody watches after the idle window
/// are killed; exited leftovers beyond the retention cap are dropped.
fn sweep(inner: &Mutex<ManagerInner>) {
    let now_ms = now();
    let mut victims: Vec<(String, Arc<PtyState>)> = Vec::new();
    {
        let mgr = lock_inner(inner);
        let mut exited: Vec<(String, i64)> = Vec::new();
        for (id, st) in mgr.sessions.iter() {
            let idle = now_ms - st.last_activity.load(Ordering::Relaxed).max(st.created_at)
                > IDLE_TIMEOUT_MS;
            let unwatched = lock(&st.output).subscribers.is_empty();
            if idle && unwatched {
                victims.push((id.clone(), st.clone()));
            } else if *lock(&st.status) == PtySessionStatus::Exited {
                exited.push((id.clone(), st.created_at));
            }
        }
        // Retention cap among exited leftovers: drop the oldest until within
        // the limit. Exited sessions never hold registered subscribers (the
        // exit drain clears the table), so eviction cannot strand a viewer.
        if exited.len() > MAX_EXITED_SESSIONS {
            exited.sort_by_key(|(_, created)| *created);
            let excess = exited.len() - MAX_EXITED_SESSIONS;
            for (id, _) in exited.into_iter().take(excess) {
                if let Some(st) = mgr.sessions.get(&id) {
                    victims.push((id, st.clone()));
                }
            }
        }
    }
    for (id, st) in victims {
        lock_inner(inner).sessions.remove(&id);
        if *lock(&st.status) == PtySessionStatus::Running {
            request_kill(&st, "idle");
        }
    }
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

fn append_output(state: &PtyState, text: String) {
    state.touch();
    let mut dead: Vec<u64> = Vec::new();
    let mut out = lock(&state.output);
    out.buffer.push_str(&text);
    out.cursor += text.len();
    if out.buffer.len() > MAX_BUFFER_BYTES {
        let excess = out.buffer.len() - MAX_BUFFER_BYTES;
        let mut trim = excess;
        while trim > 0 && !out.buffer.is_char_boundary(trim) {
            trim -= 1;
        }
        if trim > 0 {
            out.buffer.drain(..trim);
            out.start += trim;
        }
    }
    for (token, sub) in out.subscribers.iter() {
        if sub.queued.load(Ordering::Relaxed) + text.len() > SUBSCRIBER_MAX_QUEUED_BYTES {
            let _ = sub.tx.send(PtyFrame::Overflow);
            dead.push(*token);
            continue;
        }
        if sub.tx.send(PtyFrame::Data(text.clone())).is_err() {
            dead.push(*token);
        } else {
            sub.queued.fetch_add(text.len(), Ordering::Relaxed);
        }
    }
    for token in dead {
        out.subscribers.remove(&token);
    }
}

fn session_view(state: &PtyState) -> PtySession {
    PtySession {
        id: state.id.clone(),
        cwd: state.cwd.clone(),
        title: lock(&state.title).clone(),
        status: *lock(&state.status),
        exit_code: *lock(&state.exit_code),
        pid: Some(state.pid),
        rows: *lock(&state.rows),
        cols: *lock(&state.cols),
    }
}

fn list_sessions(inner: &Mutex<ManagerInner>) -> Vec<PtySession> {
    let mgr = lock_inner(inner);
    let mut out: Vec<PtySession> = mgr.sessions.values().map(|s| session_view(s)).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn lookup(inner: &Mutex<ManagerInner>, id: &str) -> PtyResult<Arc<PtyState>> {
    let mgr = lock_inner(inner);
    mgr.sessions
        .get(id)
        .cloned()
        .ok_or_else(|| PtyError::NotFound(id.to_string()))
}

fn get_session(inner: &Mutex<ManagerInner>, id: &str) -> PtyResult<PtySession> {
    let state = lookup(inner, id)?;
    Ok(session_view(&state))
}

fn update_session(
    inner: &Mutex<ManagerInner>,
    id: &str,
    title: Option<String>,
    rows: Option<u16>,
    cols: Option<u16>,
) -> PtyResult<PtySession> {
    let state = lookup(inner, id)?;
    if let Some(title) = title {
        let trimmed = title.trim().to_string();
        if trimmed.is_empty() {
            return Err(PtyError::Invalid("pty title must not be empty".to_string()));
        }
        *lock(&state.title) = trimmed;
    }
    if rows.is_some() || cols.is_some() {
        let rows = rows.unwrap_or(*lock(&state.rows)).max(1);
        let cols = cols.unwrap_or(*lock(&state.cols)).max(1);
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
    }
    state.touch();
    Ok(session_view(&state))
}

fn remove_session(inner: &Mutex<ManagerInner>, id: &str) -> PtyResult<bool> {
    let state = {
        let mut mgr = lock_inner(inner);
        mgr.sessions
            .remove(id)
            .ok_or_else(|| PtyError::NotFound(id.to_string()))?
    };
    request_kill(&state, "removed");
    Ok(true)
}

/// Attach a WS stream to a session. Subscription and replay snapshot are
/// taken in one output-lock critical section: frames produced afterwards land
/// in the queue strictly after the replay, with no overlap and no gap.
fn attach_session(
    inner: &Mutex<ManagerInner>,
    id: &str,
    cursor: Option<i64>,
) -> PtyResult<PtyAttachment> {
    let state = lookup(inner, id)?;
    state.touch();

    let (tx, rx) = mpsc::unbounded_channel();
    let queued = Arc::new(AtomicUsize::new(0));
    let (replay, end_cursor, guard) = {
        let mut out = lock(&state.output);
        let replay = replay_slice(&out.buffer, out.start, out.cursor, cursor);
        let end = out.cursor;
        if let Some((code, reason)) = out.ended {
            // Exited session: replay only, then the same terminal frame.
            let _ = tx.send(PtyFrame::Exited {
                exit_code: code,
                reason,
            });
            (replay, end, None)
        } else {
            let token = out.next_token;
            out.next_token += 1;
            out.subscribers.insert(
                token,
                Subscriber {
                    tx: tx.clone(),
                    queued: queued.clone(),
                },
            );
            drop(tx);
            (
                replay,
                end,
                Some(SubscriberGuard {
                    id: id.to_string(),
                    token,
                }),
            )
        }
    };
    Ok(PtyAttachment {
        replay,
        cursor: end_cursor,
        rx: PtyReceiver {
            rx,
            queued,
            _guard: guard,
        },
    })
}

fn replay_slice(buffer: &str, start: usize, end: usize, cursor: Option<i64>) -> String {
    if cursor == Some(-1) {
        return String::new();
    }
    let req = cursor.unwrap_or(0);
    let req = if req < 0 { 0usize } else { req as usize };
    if req <= start {
        buffer.to_string()
    } else if req >= end {
        String::new()
    } else {
        let mut offset = (req - start).min(buffer.len());
        while offset < buffer.len() && !buffer.is_char_boundary(offset) {
            offset += 1;
        }
        buffer[offset..].to_string()
    }
}

fn incomplete_tail_len(bytes: &[u8]) -> usize {
    let window = bytes.len().min(4);
    for len in 1..=window {
        let start = bytes.len() - len;
        let tail = &bytes[start..];
        if std::str::from_utf8(tail).is_ok() {
            continue;
        }
        if std::str::from_utf8(tail).unwrap_err().error_len().is_none() {
            return len;
        }
    }
    0
}

fn decode_complete(prefix: &mut Vec<u8>) -> String {
    if prefix.is_empty() {
        return String::new();
    }
    let tail = incomplete_tail_len(prefix);
    if tail == 0 {
        let text = String::from_utf8_lossy(prefix).to_string();
        prefix.clear();
        text
    } else {
        let split = prefix.len() - tail;
        let text = String::from_utf8_lossy(&prefix[..split]).to_string();
        prefix.drain(..split);
        text
    }
}

fn detach_session(inner: &Mutex<ManagerInner>, id: &str, token: u64) {
    let Ok(state) = lookup(inner, id) else {
        return;
    };
    lock(&state.output).subscribers.remove(&token);
    state.touch();
}

fn write_session(inner: &Mutex<ManagerInner>, id: &str, data: &[u8]) -> PtyResult<()> {
    let state = lookup(inner, id)?;
    if *lock(&state.status) != PtySessionStatus::Running {
        return Err(PtyError::Invalid("pty session has exited".to_string()));
    }
    let mut writer = lock(&state.writer);
    if let Some(w) = writer.as_mut() {
        w.write_all(data).map_err(|e| PtyError::Io(e.to_string()))?;
        w.flush().map_err(|e| PtyError::Io(e.to_string()))?;
        state.touch();
        Ok(())
    } else {
        Err(PtyError::Io("writer closed".to_string()))
    }
}

// Top-level convenience functions using the global manager.

pub fn create_pty_session(
    ctx: &ApiContext,
    cwd: Option<String>,
    command: Option<String>,
    rows: Option<u16>,
    cols: Option<u16>,
    title: Option<String>,
) -> PtyResult<PtySession> {
    global_manager().create_with_context(Some(ctx), cwd, command, rows, cols, title)
}

pub fn list_pty_sessions() -> Vec<PtySession> {
    global_manager().list()
}

pub fn get_pty_session(id: &str) -> PtyResult<PtySession> {
    global_manager().get(id)
}

pub fn update_pty_session(
    id: &str,
    title: Option<String>,
    rows: Option<u16>,
    cols: Option<u16>,
) -> PtyResult<PtySession> {
    global_manager().update(id, title, rows, cols)
}

pub fn remove_pty_session(id: &str) -> PtyResult<bool> {
    global_manager().remove(id)
}

pub fn attach_pty_session(id: &str, cursor: Option<i64>) -> PtyResult<PtyAttachment> {
    global_manager().attach(id, cursor)
}

pub fn write_pty_session(id: &str, data: &str) -> PtyResult<()> {
    global_manager().write(id, data.as_bytes())
}

pub fn write_pty_bytes(id: &str, data: &[u8]) -> PtyResult<()> {
    global_manager().write(id, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_state() -> Arc<PtyState> {
        Arc::new(PtyState {
            id: "pty-test".to_string(),
            cwd: "/tmp".to_string(),
            title: Mutex::new(String::new()),
            pid: 1,
            status: Mutex::new(PtySessionStatus::Running),
            exit_code: Mutex::new(None),
            rows: Mutex::new(24),
            cols: Mutex::new(80),
            output: Mutex::new(Output {
                buffer: String::new(),
                start: 0,
                cursor: 0,
                subscribers: HashMap::new(),
                next_token: 1,
                ended: None,
            }),
            master: Mutex::new(None),
            writer: Mutex::new(None),
            created_at: now(),
            last_activity: AtomicI64::new(now()),
            kill_request: AtomicBool::new(false),
            kill_reason: Mutex::new(None),
            exit_announced: AtomicBool::new(false),
            event_bus: None,
        })
    }

    fn seeded_inner(state: Arc<PtyState>) -> Mutex<ManagerInner> {
        Mutex::new(ManagerInner {
            sessions: HashMap::from([(state.id.clone(), state)]),
        })
    }

    #[test]
    fn replay_and_live_frames_never_overlap_or_drop() {
        let state = stub_state();
        append_output(&state, "alpha".into());
        let inner = seeded_inner(state.clone());

        let mut att = attach_session(&inner, "pty-test", Some(0)).unwrap();
        assert_eq!(att.replay, "alpha");
        assert_eq!(att.cursor, 5);

        // Output produced after the snapshot must arrive through the queue,
        // never duplicated inside the replay and never skipped.
        append_output(&state, "beta".into());
        assert!(matches!(
            att.rx.rx.try_recv(),
            Ok(PtyFrame::Data(ref t)) if t == "beta"
        ));
        assert!(att.rx.rx.try_recv().is_err());

        // Consuming credits the backlog counter.
        let queued = att.rx.queued.load(Ordering::Relaxed);
        att.rx.credit(4);
        assert_eq!(att.rx.queued.load(Ordering::Relaxed), queued - 4);
    }

    #[test]
    fn exited_attach_replays_then_delivers_terminal_frame_without_registration() {
        let state = stub_state();
        append_output(&state, "tail".into());
        finalize_exit(&state, Some(3), "process");
        let inner = seeded_inner(state.clone());

        let mut att = attach_session(&inner, "pty-test", Some(0)).unwrap();
        assert_eq!(att.replay, "tail");
        assert!(matches!(
            att.rx.rx.try_recv(),
            Ok(PtyFrame::Exited {
                exit_code: Some(3),
                reason: "process"
            })
        ));
        // The late attacher is not registered; the queue closes after the frame.
        assert!(att.rx.rx.try_recv().is_err());
        assert!(att.rx._guard.is_none());
        assert!(lock(&state.output).subscribers.is_empty());

        // Second finalize is a no-op: status stays and nothing re-announces.
        finalize_exit(&state, Some(9), "process");
        assert_eq!(*lock(&state.exit_code), Some(3));
    }

    #[test]
    fn replay_slice_clamps_mid_char_offset_to_boundary() {
        let buffer = "a\u{e9}bc".to_string();
        let end = buffer.len();
        let mid = 2;
        assert!(!buffer.is_char_boundary(mid));
        let out = replay_slice(&buffer, 0, end, Some(mid as i64));
        assert_eq!(out, "bc");
    }

    #[test]
    fn decode_complete_holds_split_multibyte_sequence() {
        let emoji = "\u{1f600}";
        let bytes = emoji.as_bytes();
        let mut carry: Vec<u8> = Vec::new();
        carry.extend_from_slice(&bytes[..2]);
        let first = decode_complete(&mut carry);
        assert!(first.is_empty());
        assert_eq!(carry.len(), 2);
        carry.extend_from_slice(&bytes[2..]);
        let second = decode_complete(&mut carry);
        assert_eq!(second, emoji);
        assert!(carry.is_empty());
    }
}
