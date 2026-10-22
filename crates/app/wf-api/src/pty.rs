use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::infra::context::ApiContext;

mod decode;
mod io;
mod session;
mod state;

pub use state::{
    global_manager, lock as state_lock, lock_inner, manager_inner, sweep, PtyError, PtyManager,
    PtyResult, PtySession, PtySessionStatus,
};
// Convenience re-export of common lock helper under original name for tests/internal.
pub use state::lock;

pub const MAX_BUFFER_BYTES: usize = state::MAX_BUFFER_BYTES;
pub const TRIM_SLACK_BYTES: usize = state::TRIM_SLACK_BYTES;
pub const MAX_EXITED_SESSIONS: usize = state::MAX_EXITED_SESSIONS;
pub const SUBSCRIBER_MAX_QUEUED_BYTES: usize = state::SUBSCRIBER_MAX_QUEUED_BYTES;
pub const IDLE_TIMEOUT_MS: i64 = state::IDLE_TIMEOUT_MS;
pub const SWEEP_INTERVAL: Duration = state::SWEEP_INTERVAL;
pub const EXIT_POLL_INTERVAL: Duration = state::EXIT_POLL_INTERVAL;

#[derive(Debug, Clone)]
pub enum PtyFrame {
    Data(String),
    Exited {
        exit_code: Option<i32>,
        reason: &'static str,
    },
    Overflow,
}

pub struct PtyReceiver {
    pub rx: mpsc::UnboundedReceiver<PtyFrame>,
    pub queued: Arc<AtomicUsize>,
    pub _guard: Option<io::SubscriberGuard>,
}

impl PtyReceiver {
    pub async fn recv(&mut self) -> Option<PtyFrame> {
        self.rx.recv().await
    }

    pub fn credit(&self, bytes: usize) {
        self.queued.fetch_sub(bytes, Ordering::Relaxed);
    }
}

pub struct PtyAttachment {
    pub replay: String,
    pub cursor: usize,
    pub rx: PtyReceiver,
}

// Re-export Output? not needed externally.

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
    use super::state;
    use super::io;
    use super::session;
    use super::decode;

    fn stub_state() -> Arc<state::PtyState> {
        Arc::new(state::PtyState {
            id: "pty-test".to_string(),
            cwd: "/tmp".to_string(),
            title: Mutex::new(String::new()),
            pid: 1,
            status: Mutex::new(PtySessionStatus::Running),
            exit_code: Mutex::new(None),
            rows: Mutex::new(24),
            cols: Mutex::new(80),
            output: Mutex::new(state::Output {
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

    fn seeded_inner(state: Arc<state::PtyState>) -> Mutex<state::ManagerInner> {
        Mutex::new(state::ManagerInner {
            sessions: HashMap::from([(state.id.clone(), state)]),
        })
    }

    #[test]
    fn replay_and_live_frames_never_overlap_or_drop() {
        let state = stub_state();
        io::append_output(&state, "alpha".into());
        let inner = seeded_inner(state.clone());

        let mut att = io::attach_session(&inner, "pty-test", Some(0)).unwrap();
        assert_eq!(att.replay, "alpha");
        assert_eq!(att.cursor, 5);

        io::append_output(&state, "beta".into());
        assert!(matches!(
            att.rx.rx.try_recv(),
            Ok(PtyFrame::Data(ref t)) if t == "beta"
        ));
        assert!(att.rx.rx.try_recv().is_err());

        let queued = att.rx.queued.load(Ordering::Relaxed);
        att.rx.credit(4);
        assert_eq!(att.rx.queued.load(Ordering::Relaxed), queued - 4);
    }

    #[test]
    fn exited_attach_replays_then_delivers_terminal_frame_without_registration() {
        let state = stub_state();
        io::append_output(&state, "tail".into());
        session::finalize_exit(&state, Some(3), "process");
        let inner = seeded_inner(state.clone());

        let mut att = io::attach_session(&inner, "pty-test", Some(0)).unwrap();
        assert_eq!(att.replay, "tail");
        assert!(matches!(
            att.rx.rx.try_recv(),
            Ok(PtyFrame::Exited {
                exit_code: Some(3),
                reason: "process"
            })
        ));
        assert!(att.rx.rx.try_recv().is_err());
        assert!(att.rx._guard.is_none());
        assert!(state::lock(&state.output).subscribers.is_empty());

        session::finalize_exit(&state, Some(9), "process");
        assert_eq!(*state::lock(&state.exit_code), Some(3));
    }

    #[test]
    fn replay_slice_clamps_mid_char_offset_to_boundary() {
        let buffer = "a\u{e9}bc".to_string();
        let end = buffer.len();
        let mid = 2;
        assert!(!buffer.is_char_boundary(mid));
        let out = io::replay_slice(&buffer, 0, end, Some(mid as i64));
        assert_eq!(out, "bc");
    }

    #[test]
    fn decode_complete_holds_split_multibyte_sequence() {
        let emoji = "\u{1f600}";
        let bytes = emoji.as_bytes();
        let mut carry: Vec<u8> = Vec::new();
        carry.extend_from_slice(&bytes[..2]);
        let first = decode::decode_complete(&mut carry);
        assert!(first.is_empty());
        assert_eq!(carry.len(), 2);
        carry.extend_from_slice(&bytes[2..]);
        let second = decode::decode_complete(&mut carry);
        assert_eq!(second, emoji);
        assert!(carry.is_empty());
    }
}