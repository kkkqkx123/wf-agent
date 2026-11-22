//! Per-session write coordination and the single in-flight turn slot.
//!
//! The tree is one document: two concurrent surgeries would each write the
//! whole document and the later write would silently drop the earlier one. A
//! per-session mutex serialises them.
//!
//! Generation is different. A turn holds the lock only to write the user node
//! and to converge the reply; the model call runs outside it, otherwise a slow
//! provider would block every read of the session. While it runs, the slot
//! records what is being generated so a reconnecting client can be told
//! exactly that, and a second turn is refused instead of interleaving.

use std::sync::{Arc, Mutex, OnceLock};

use dashmap::DashMap;
use tokio::sync::{Mutex as AsyncMutex, MutexGuard};
use tokio_util::sync::CancellationToken;

use crate::infra::error::{ApiError, ApiResult};

/// A turn that is currently generating.
#[derive(Clone)]
pub struct InflightTurn {
    pub turn_id: String,
    pub node_id: String,
    pub cancel: CancellationToken,
}

/// What a snapshot reports about the in-flight turn: the ids and how much text
/// has arrived, so a reconnecting client shows the partial answer instead of an
/// empty bubble.
#[derive(Debug, Clone, Default)]
pub struct InflightSnapshot {
    pub turn_id: String,
    pub node_id: String,
    pub text: String,
    pub reasoning: String,
}

struct Slot {
    turn: InflightTurn,
    text: String,
    reasoning: String,
}

pub struct SessionRuntime {
    write: AsyncMutex<()>,
    slot: Mutex<Option<Slot>>,
}

impl Default for SessionRuntime {
    fn default() -> Self {
        Self {
            write: AsyncMutex::new(()),
            slot: Mutex::new(None),
        }
    }
}

impl SessionRuntime {
    /// Serialise a tree mutation. Hold it across the read-modify-write, not
    /// across a model call.
    pub async fn write(&self) -> MutexGuard<'_, ()> {
        self.write.lock().await
    }

    /// Claim the in-flight slot. A second concurrent turn is a conflict: the
    /// tree has one head and one generating answer.
    pub fn begin_turn(&self, turn: InflightTurn) -> ApiResult<()> {
        let mut slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(current) = slot.as_ref() {
            return Err(ApiError::Conflict(format!(
                "session is already generating turn '{}'",
                current.turn.turn_id
            )));
        }
        *slot = Some(Slot {
            turn,
            text: String::new(),
            reasoning: String::new(),
        });
        Ok(())
    }

    /// Append streamed text to the in-flight partial answer.
    pub fn push_text(&self, turn_id: &str, delta: &str) {
        let mut slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(current) = slot.as_mut() {
            if current.turn.turn_id == turn_id {
                current.text.push_str(delta);
            }
        }
    }

    /// Append streamed reasoning to the in-flight partial answer.
    pub fn push_reasoning(&self, turn_id: &str, delta: &str) {
        let mut slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(current) = slot.as_mut() {
            if current.turn.turn_id == turn_id {
                current.reasoning.push_str(delta);
            }
        }
    }

    /// Release the slot. Only the turn that owns it can release it, so a late
    /// finisher cannot clear a newer turn's claim.
    pub fn finish_turn(&self, turn_id: &str) {
        let mut slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        if slot
            .as_ref()
            .is_some_and(|current| current.turn.turn_id == turn_id)
        {
            *slot = None;
        }
    }

    pub fn inflight(&self) -> Option<InflightSnapshot> {
        let slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        slot.as_ref().map(|current| InflightSnapshot {
            turn_id: current.turn.turn_id.clone(),
            node_id: current.turn.node_id.clone(),
            text: current.text.clone(),
            reasoning: current.reasoning.clone(),
        })
    }

    /// Cancellation token of the in-flight turn, when it matches `turn_id`
    /// (or any turn when `None`).
    pub fn cancel_token(&self, turn_id: Option<&str>) -> Option<CancellationToken> {
        let slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        let current = slot.as_ref()?;
        match turn_id {
            Some(id) if current.turn.turn_id != id => None,
            _ => Some(current.turn.cancel.clone()),
        }
    }
}

/// Session id to runtime. One map per process: the tree lives in storage, so a
/// second process would need a distributed lock anyway, and this deployment
/// runs one server per storage.
fn runtimes() -> &'static DashMap<String, Arc<SessionRuntime>> {
    static RUNTIMES: OnceLock<DashMap<String, Arc<SessionRuntime>>> = OnceLock::new();
    RUNTIMES.get_or_init(DashMap::new)
}

/// Runtime of one session, created on first use.
pub fn runtime_for(session_id: &str) -> Arc<SessionRuntime> {
    if let Some(existing) = runtimes().get(session_id) {
        return existing.clone();
    }
    let fresh = Arc::new(SessionRuntime::default());
    runtimes()
        .entry(session_id.to_string())
        .or_insert(fresh)
        .clone()
}

/// Drop a session's runtime. Called when the session is deleted, so a long-lived
/// server does not accumulate one entry per deleted conversation.
pub fn forget(session_id: &str) {
    runtimes().remove(session_id);
}
