use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use wf_common::now;

use crate::infra::context::ApiContext;
use crate::{BaseEvent, EventType};

use super::state::{
    lock, lock_inner, PtySessionStatus, PtyState,
};
use super::{PtyError, PtyResult, PtySession};

pub fn create_with_context(
    ctx: Option<&ApiContext>,
    cwd: Option<String>,
    command: Option<String>,
    rows: Option<u16>,
    cols: Option<u16>,
    title: Option<String>,
    inner: &Mutex<super::state::ManagerInner>,
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
        output: Mutex::new(super::state::Output {
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
        last_activity: std::sync::atomic::AtomicI64::new(now()),
        kill_request: std::sync::atomic::AtomicBool::new(false),
        kill_reason: Mutex::new(None),
        exit_announced: std::sync::atomic::AtomicBool::new(false),
        event_bus: ctx.map(|c| c.event_bus.clone()),
    });

    let state_r = state.clone();
    std::thread::spawn(move || {
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
                            super::io::append_output(&state_r, text);
                        }
                    }
                    break;
                }
                Ok(n) => {
                    carry.extend_from_slice(&buf[..n]);
                    let text = super::decode::decode_complete(&mut carry);
                    if text.is_empty() {
                        state_r.touch();
                    } else {
                        super::io::append_output(&state_r, text);
                    }
                }
                Err(_) => {
                    if !carry.is_empty() {
                        let text = String::from_utf8_lossy(&carry).to_string();
                        carry.clear();
                        if !text.is_empty() {
                            super::io::append_output(&state_r, text);
                        }
                    }
                    break;
                }
            }
        }
    });

    let state_e = state.clone();
    std::thread::spawn(move || {
        let mut child = child;
        let mut killing = false;
        let code = loop {
            if !killing && state_e.kill_request.load(std::sync::atomic::Ordering::Acquire) {
                killing = true;
                let _ = child.kill();
            }
            match child.try_wait() {
                Ok(Some(status)) => break Some(status.exit_code() as i32),
                Ok(None) => std::thread::sleep(super::state::EXIT_POLL_INTERVAL),
                Err(_) => break None,
            }
        };
        let reason = lock(&state_e.kill_reason).unwrap_or("process");
        finalize_exit(&state_e, code, reason);
        *lock(&state_e.master) = None;
        *lock(&state_e.writer) = None;
    });

    lock_inner(inner).sessions.insert(id.clone(), state.clone());

    Ok(session_view(&state))
}

pub fn finalize_exit(state: &Arc<PtyState>, exit_code: Option<i32>, reason: &'static str) {
    if state.exit_announced.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    *lock(&state.status) = PtySessionStatus::Exited;
    *lock(&state.exit_code) = exit_code;
    let senders: Vec<tokio::sync::mpsc::UnboundedSender<super::PtyFrame>> = {
        let mut out = lock(&state.output);
        out.ended = Some((exit_code, reason));
        out.subscribers.drain().map(|(_, sub)| sub.tx).collect()
    };
    for tx in senders {
        let _ = tx.send(super::PtyFrame::Exited { exit_code, reason });
    }
    publish_exited(state, exit_code, reason);
}

pub fn publish_exited(state: &PtyState, exit_code: Option<i32>, reason: &'static str) {
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

pub fn request_kill(state: &PtyState, reason: &'static str) {
    *lock(&state.kill_reason) = Some(reason);
    state.kill_request.store(true, std::sync::atomic::Ordering::Release);
}

pub fn resolve_cwd(ctx: Option<&ApiContext>, cwd: Option<String>) -> PtyResult<PathBuf> {
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

pub fn session_view(state: &PtyState) -> PtySession {
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

pub fn list_sessions(inner: &Mutex<super::state::ManagerInner>) -> Vec<PtySession> {
    let mgr = lock_inner(inner);
    let mut out: Vec<PtySession> = mgr.sessions.values().map(|s| session_view(s)).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn lookup(inner: &Mutex<super::state::ManagerInner>, id: &str) -> PtyResult<Arc<PtyState>> {
    let mgr = lock_inner(inner);
    mgr.sessions
        .get(id)
        .cloned()
        .ok_or_else(|| PtyError::NotFound(id.to_string()))
}

pub fn get_session(inner: &Mutex<super::state::ManagerInner>, id: &str) -> PtyResult<PtySession> {
    let state = lookup(inner, id)?;
    Ok(session_view(&state))
}

pub fn update_session(
    inner: &Mutex<super::state::ManagerInner>,
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

pub fn remove_session(inner: &Mutex<super::state::ManagerInner>, id: &str) -> PtyResult<bool> {
    let state = {
        let mut mgr = lock_inner(inner);
        mgr.sessions
            .remove(id)
            .ok_or_else(|| PtyError::NotFound(id.to_string()))?
    };
    request_kill(&state, "removed");
    Ok(true)
}