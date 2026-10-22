use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use wf_common::now;
use wf_core::EventBus;

use crate::infra::context::ApiContext;
use crate::ApiError;

pub const MAX_BUFFER_BYTES: usize = 2 * 1024 * 1024;
pub const TRIM_SLACK_BYTES: usize = 256 * 1024;
pub const MAX_EXITED_SESSIONS: usize = 25;
pub const SUBSCRIBER_MAX_QUEUED_BYTES: usize = 4 * 1024 * 1024;
pub const IDLE_TIMEOUT_MS: i64 = 600_000;
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
pub const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

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

pub struct Output {
    pub buffer: String,
    pub start: usize,
    pub cursor: usize,
    pub subscribers: HashMap<u64, super::io::Subscriber>,
    pub next_token: u64,
    pub ended: Option<(Option<i32>, &'static str)>,
}

pub struct PtyState {
    pub id: String,
    pub cwd: String,
    pub title: Mutex<String>,
    pub pid: u32,
    pub status: Mutex<PtySessionStatus>,
    pub exit_code: Mutex<Option<i32>>,
    pub rows: Mutex<u16>,
    pub cols: Mutex<u16>,
    pub output: Mutex<Output>,
    pub master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    pub writer: Mutex<Option<Box<dyn std::io::Write + Send>>>,
    pub created_at: i64,
    pub last_activity: AtomicI64,
    pub kill_request: AtomicBool,
    pub kill_reason: Mutex<Option<&'static str>>,
    pub exit_announced: AtomicBool,
    pub event_bus: Option<Arc<EventBus>>,
}

impl PtyState {
    pub fn touch(&self) {
        self.last_activity.store(now(), Ordering::Relaxed);
    }
}

pub struct ManagerInner {
    pub sessions: HashMap<String, Arc<PtyState>>,
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
    ) -> PtyResult<super::PtySession> {
        super::session::create_with_context(ctx, cwd, command, rows, cols, title, &self.inner)
    }

    pub fn list(&self) -> Vec<super::PtySession> {
        super::session::list_sessions(&self.inner)
    }

    pub fn get(&self, id: &str) -> PtyResult<super::PtySession> {
        super::session::get_session(&self.inner, id)
    }

    pub fn update(
        &self,
        id: &str,
        title: Option<String>,
        rows: Option<u16>,
        cols: Option<u16>,
    ) -> PtyResult<super::PtySession> {
        super::session::update_session(&self.inner, id, title, rows, cols)
    }

    pub fn remove(&self, id: &str) -> PtyResult<bool> {
        super::session::remove_session(&self.inner, id)
    }

    pub fn attach(&self, id: &str, cursor: Option<i64>) -> PtyResult<super::PtyAttachment> {
        super::io::attach_session(&self.inner, id, cursor)
    }

    pub fn write(&self, id: &str, data: &[u8]) -> PtyResult<()> {
        super::io::write_session(&self.inner, id, data)
    }
}

static PTY_MANAGER: std::sync::OnceLock<PtyManager> = std::sync::OnceLock::new();

pub fn global_manager() -> &'static PtyManager {
    let manager = PTY_MANAGER.get_or_init(PtyManager::new);
    manager.ensure_reaper();
    manager
}

pub fn manager_inner(m: &PtyManager) -> &Mutex<ManagerInner> {
    &m.inner
}

pub fn lock_inner(inner: &Mutex<ManagerInner>) -> std::sync::MutexGuard<'_, ManagerInner> {
    match inner.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

pub fn sweep(inner: &Mutex<ManagerInner>) {
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
            super::session::request_kill(&st, "idle");
        }
    }
}

