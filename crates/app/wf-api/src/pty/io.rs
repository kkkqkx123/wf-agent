use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;


use super::state::{lock, PtyState};
use super::{PtyAttachment, PtyError, PtyFrame, PtyReceiver, PtyResult};

pub struct Subscriber {
    pub tx: mpsc::UnboundedSender<PtyFrame>,
    pub queued: Arc<AtomicUsize>,
}

pub struct SubscriberGuard {
    pub id: String,
    pub token: u64,
}

impl Drop for SubscriberGuard {
    fn drop(&mut self) {
        let mgr = super::state::global_manager();
        let inner = super::state::manager_inner(mgr);
        detach_session(inner, &self.id, self.token);
    }
}
pub fn append_output(state: &PtyState, text: String) {
    state.touch();
    let mut dead: Vec<u64> = Vec::new();
    let mut out = lock(&state.output);
    out.buffer.push_str(&text);
    let text_len = text.len();
    out.cursor += text_len;
    if out.buffer.len() > super::state::MAX_BUFFER_BYTES + super::state::TRIM_SLACK_BYTES {
        let excess = out.buffer.len() - super::state::MAX_BUFFER_BYTES;
        let mut trim = excess;
        while trim > 0 && !out.buffer.is_char_boundary(trim) {
            trim -= 1;
        }
        if trim > 0 {
            out.buffer.drain(..trim);
            out.start += trim;
        }
    }
    let mut text_owned = Some(text);
    let single = out.subscribers.len() == 1;
    for (token, sub) in out.subscribers.iter() {
        if sub.queued.load(Ordering::Relaxed) + text_len > super::state::SUBSCRIBER_MAX_QUEUED_BYTES {
            let _ = sub.tx.send(PtyFrame::Overflow);
            dead.push(*token);
            continue;
        }
        let payload = if single {
            text_owned.take().expect("single subscriber sends once")
        } else {
            text_owned
                .as_ref()
                .expect("multi subscriber retains text")
                .clone()
        };
        if sub.tx.send(PtyFrame::Data(payload)).is_err() {
            dead.push(*token);
        } else {
            sub.queued.fetch_add(text_len, Ordering::Relaxed);
        }
    }
    for token in dead {
        out.subscribers.remove(&token);
    }
}

pub fn attach_session(
    inner: &Mutex<super::state::ManagerInner>,
    id: &str,
    cursor: Option<i64>,
) -> PtyResult<PtyAttachment> {
    let state = super::session::lookup(inner, id)?;
    state.touch();

    let (tx, rx) = mpsc::unbounded_channel();
    let queued = Arc::new(AtomicUsize::new(0));
    let (replay, end_cursor, guard) = {
        let mut out = lock(&state.output);
        let replay = replay_slice(&out.buffer, out.start, out.cursor, cursor);
        let end = out.cursor;
        if let Some((code, reason)) = out.ended {
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

pub fn detach_session(inner: &Mutex<super::state::ManagerInner>, id: &str, token: u64) {
    let Ok(state) = super::session::lookup(inner, id) else {
        return;
    };
    lock(&state.output).subscribers.remove(&token);
    state.touch();
}

pub fn replay_slice(buffer: &str, start: usize, end: usize, cursor: Option<i64>) -> String {
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

pub fn write_session(inner: &Mutex<super::state::ManagerInner>, id: &str, data: &[u8]) -> PtyResult<()> {
    let state = super::session::lookup(inner, id)?;
    if *lock(&state.status) != super::state::PtySessionStatus::Running {
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