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

use std::sync::{
    atomic::{AtomicI64, AtomicUsize, Ordering},
    Arc, Mutex, OnceLock,
};

use dashmap::DashMap;
use tokio::sync::{Mutex as AsyncMutex, MutexGuard, Notify};
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
    /// Present while a stepping job owns the slot across several turns.
    job_id: Option<String>,
}

pub struct SessionRuntime {
    write: AsyncMutex<()>,
    slot: Mutex<Option<Slot>>,
    subscribers: AtomicUsize,
    last_used_ms: AtomicI64,
    /// Woken whenever the in-flight slot is released, so callers that must
    /// wait for convergence (session deletion) can do so without holding
    /// the write lock — the converging driver needs that same lock.
    settled: Notify,
}

/// RAII handle for one live SSE subscriber. Created when a subscription stream
/// is built and dropped with it, so the idle-eviction count stays balanced even
/// when a client disconnects without an explicit teardown.
pub struct SubscriptionGuard {
    runtime: Arc<SessionRuntime>,
}

impl SubscriptionGuard {
    pub fn new(runtime: Arc<SessionRuntime>) -> Self {
        runtime.add_subscriber();
        Self { runtime }
    }
}

impl Drop for SubscriptionGuard {
    fn drop(&mut self) {
        self.runtime.remove_subscriber();
    }
}

/// An idle session is evicted from the process-local table. The tree is
/// durable, so eviction only drops locks and cached event text.
const IDLE_TTL_MS: i64 = 5 * 60 * 1000;

impl Default for SessionRuntime {
    fn default() -> Self {
        Self {
            write: AsyncMutex::new(()),
            slot: Mutex::new(None),
            subscribers: AtomicUsize::new(0),
            last_used_ms: AtomicI64::new(wf_common::now()),
            settled: Notify::new(),
        }
    }
}

impl SessionRuntime {
    fn touch(&self) {
        self.last_used_ms.store(wf_common::now(), Ordering::Relaxed);
    }

    /// Serialise a tree mutation. Hold it across the read-modify-write, not
    /// across a model call.
    pub async fn write(&self) -> MutexGuard<'_, ()> {
        self.touch();
        self.write.lock().await
    }

    /// Claim the in-flight slot. A second concurrent turn is a conflict: the
    /// tree has one head and one generating answer.
    pub fn begin_turn(&self, turn: InflightTurn) -> ApiResult<()> {
        self.touch();
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
            job_id: None,
        });
        Ok(())
    }

    /// Claim the slot for a whole stepping job. Individual steps reuse it, so
    /// a turn cannot interleave between two steps.
    pub fn begin_job(
        &self,
        job_id: impl Into<String>,
        node_id: impl Into<String>,
        cancel: CancellationToken,
    ) -> ApiResult<()> {
        self.touch();
        let job_id = job_id.into();
        let mut slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(current) = slot.as_ref() {
            return Err(ApiError::Conflict(format!(
                "session is already generating turn '{}'",
                current.turn.turn_id
            )));
        }
        *slot = Some(Slot {
            turn: InflightTurn {
                turn_id: job_id.clone(),
                node_id: node_id.into(),
                cancel,
            },
            text: String::new(),
            reasoning: String::new(),
            job_id: Some(job_id),
        });
        Ok(())
    }

    /// Point a job's slot at one step's generation. Partial text starts fresh
    /// for every step.
    pub fn advance_job(
        &self,
        job_id: &str,
        turn_id: impl Into<String>,
        node_id: impl Into<String>,
    ) -> ApiResult<()> {
        self.touch();
        let mut slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        match slot.as_mut() {
            Some(current) if current.job_id.as_deref() == Some(job_id) => {
                current.turn.turn_id = turn_id.into();
                current.turn.node_id = node_id.into();
                current.text.clear();
                current.reasoning.clear();
                Ok(())
            }
            _ => Err(ApiError::Conflict(format!(
                "stepping job '{job_id}' no longer owns the session"
            ))),
        }
    }

    /// Append streamed text to the in-flight partial answer.
    pub fn push_text(&self, turn_id: &str, delta: &str) {
        self.touch();
        let mut slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(current) = slot.as_mut() {
            if current.turn.turn_id == turn_id {
                current.text.push_str(delta);
            }
        }
    }

    /// Append streamed reasoning to the in-flight partial answer.
    pub fn push_reasoning(&self, turn_id: &str, delta: &str) {
        self.touch();
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
        let released = slot
            .as_ref()
            .is_some_and(|current| current.job_id.is_none() && current.turn.turn_id == turn_id);
        if released {
            *slot = None;
            drop(slot);
            self.settled.notify_waiters();
        }
    }

    /// Release a stepping job's slot. Only the owning job can do that.
    pub fn finish_job(&self, job_id: &str) {
        let mut slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        let released = slot
            .as_ref()
            .is_some_and(|current| current.job_id.as_deref() == Some(job_id));
        if released {
            *slot = None;
            drop(slot);
            self.settled.notify_waiters();
        }
    }

    /// Wait until no turn is in flight, without holding the write lock.
    ///
    /// Convergence takes the write lock after generation, so a deleter must
    /// release that lock before waiting or it deadlocks itself. The notify is
    /// registered *before* re-checking the slot to avoid missing a wakeup that
    /// lands between the check and the wait.
    pub async fn settle(&self) {
        loop {
            let notified = self.settled.notified();
            if !self.has_inflight() {
                return;
            }
            notified.await;
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

    fn has_inflight(&self) -> bool {
        let slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        slot.is_some()
    }

    fn add_subscriber(&self) {
        self.touch();
        self.subscribers.fetch_add(1, Ordering::Relaxed);
    }

    fn remove_subscriber(&self) {
        self.touch();
        self.subscribers.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.load(Ordering::Relaxed)
    }

    pub fn last_used_ms(&self) -> i64 {
        self.last_used_ms.load(Ordering::Relaxed)
    }

    /// Cancellation token of the in-flight turn, when it matches `turn_id`
    /// (or any turn when `None`).
    pub fn cancel_token(&self, turn_id: Option<&str>) -> Option<CancellationToken> {
        let slot = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        let current = slot.as_ref()?;
        match turn_id {
            Some(id) if current.turn.turn_id != id && current.job_id.as_deref() != Some(id) => None,
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
    let table = runtimes();
    let now = wf_common::now();
    let expired: Vec<String> = table
        .iter()
        .filter(|entry| {
            let runtime = entry.value();
            now - runtime.last_used_ms() > IDLE_TTL_MS
                && !runtime.has_inflight()
                && runtime.subscriber_count() == 0
        })
        .map(|entry| entry.key().clone())
        .collect();
    for id in expired {
        table.remove(&id);
    }
    if let Some(existing) = table.get(session_id) {
        existing.value().touch();
        return existing.clone();
    }
    let fresh = Arc::new(SessionRuntime::default());
    table.entry(session_id.to_string()).or_insert(fresh).clone()
}

/// Drop a session's runtime. Called when the session is deleted, so a long-lived
/// server does not accumulate one entry per deleted conversation.
pub fn forget(session_id: &str) {
    runtimes().remove(session_id);
}
