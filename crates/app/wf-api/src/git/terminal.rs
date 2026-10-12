use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use super::config::{command_timeout_ms, repo_root};
use super::runner::run_shell;
use super::types::{GitError, GitResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalSessionView {
    pub id: String,
    pub cwd: String,
    pub output_len: usize,
    pub closed: bool,
}

struct TerminalSession {
    cwd: String,
    output: String,
    closed: bool,
    updated: i64,
}

fn sessions() -> &'static Mutex<HashMap<String, TerminalSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, TerminalSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_sessions() -> std::sync::MutexGuard<'static, HashMap<String, TerminalSession>> {
    match sessions().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub fn create_terminal_session(
    ctx: &crate::infra::context::ApiContext,
) -> GitResult<TerminalSessionView> {
    let root = repo_root(ctx)?;
    let idle_secs = ctx
        .user_git_config()
        .map(|c| c.terminal_idle_secs)
        .unwrap_or(600);
    let mut guard = lock_sessions();
    let now = wf_common::now();
    guard.retain(|_, s| now - s.updated < (idle_secs.max(60) as i64) * 1000);
    let id = format!("gts-{}", wf_common::generate_id());
    guard.insert(
        id.clone(),
        TerminalSession {
            cwd: root.to_string_lossy().to_string(),
            output: String::new(),
            closed: false,
            updated: now,
        },
    );
    let session = guard.get(&id).expect("just inserted");
    Ok(TerminalSessionView {
        id,
        cwd: session.cwd.clone(),
        output_len: 0,
        closed: false,
    })
}

pub fn terminal_input(
    ctx: &crate::infra::context::ApiContext,
    id: &str,
    input: &str,
) -> GitResult<TerminalSessionView> {
    if input.len() > 65536 {
        return Err(GitError::Precondition("input too large".to_string()));
    }
    let root = repo_root(ctx)?;
    let timeout = command_timeout_ms(ctx, false);
    let mut guard = lock_sessions();
    let session = guard
        .get_mut(id)
        .ok_or_else(|| GitError::NoSession(id.to_string()))?;
    if session.closed {
        return Err(GitError::NoSession(id.to_string()));
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(TerminalSessionView {
            id: id.to_string(),
            cwd: session.cwd.clone(),
            output_len: session.output.len(),
            closed: false,
        });
    }
    let result = run_shell(&root, trimmed, timeout);
    let chunk = match result {
        Ok(out) => {
            if out.stderr.trim().is_empty() {
                format!("$ {trimmed}\n{}", out.stdout)
            } else {
                format!("$ {trimmed}\n{}{}", out.stdout, out.stderr)
            }
        }
        Err(e) => format!("$ {trimmed}\nerror: {e}\n"),
    };
    session.output.push_str(&chunk);
    if session.output.len() > 1_048_576 {
        let excess = session.output.len() - 1_048_576;
        session.output.drain(..excess);
    }
    session.updated = wf_common::now();
    Ok(TerminalSessionView {
        id: id.to_string(),
        cwd: session.cwd.clone(),
        output_len: session.output.len(),
        closed: false,
    })
}

pub fn terminal_output(
    _ctx: &crate::infra::context::ApiContext,
    id: &str,
    offset: usize,
) -> GitResult<String> {
    let guard = lock_sessions();
    let session = guard
        .get(id)
        .ok_or_else(|| GitError::NoSession(id.to_string()))?;
    if offset >= session.output.len() {
        return Ok(String::new());
    }
    Ok(session.output[offset..].to_string())
}

pub fn terminal_resize(
    _ctx: &crate::infra::context::ApiContext,
    id: &str,
    _rows: u32,
    _cols: u32,
) -> GitResult<TerminalSessionView> {
    let mut guard = lock_sessions();
    let session = guard
        .get_mut(id)
        .ok_or_else(|| GitError::NoSession(id.to_string()))?;
    session.updated = wf_common::now();
    Ok(TerminalSessionView {
        id: id.to_string(),
        cwd: session.cwd.clone(),
        output_len: session.output.len(),
        closed: session.closed,
    })
}

pub fn close_terminal_session(
    _ctx: &crate::infra::context::ApiContext,
    id: &str,
) -> GitResult<bool> {
    let mut guard = lock_sessions();
    if let Some(session) = guard.get_mut(id) {
        session.closed = true;
        guard.remove(id);
        Ok(true)
    } else {
        Err(GitError::NoSession(id.to_string()))
    }
}
