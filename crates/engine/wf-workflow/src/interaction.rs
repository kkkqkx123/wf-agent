//! Asynchronous user interaction resolution for USER_INTERACTION nodes.
//!
//! A node publishes an interaction request (interaction id + prompt) on the
//! event bus and then waits for the response to be injected through
//! [`complete_interaction`]. External responders (applications, test
//! harnesses, human-in-the-loop UIs) resolve pending interactions by id.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};

use dashmap::DashMap;
use serde_json::Value;
use tokio::sync::oneshot;

static INTERACTIONS: OnceLock<Arc<InteractionRegistry>> = OnceLock::new();

/// Registry of pending interaction channels keyed by interaction id.
pub struct InteractionRegistry {
    pending: DashMap<String, oneshot::Sender<Value>>,
}

impl InteractionRegistry {
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
        }
    }

    /// Register a pending interaction; the returned receiver resolves when
    /// [`InteractionRegistry::complete`] is called with the same id.
    pub fn register(&self, interaction_id: String) -> oneshot::Receiver<Value> {
        let (tx, rx) = oneshot::channel();
        self.pending.insert(interaction_id, tx);
        rx
    }

    /// Resolve a pending interaction with the supplied response.
    pub fn complete(&self, interaction_id: &str, response: Value) -> bool {
        match self.pending.remove(interaction_id) {
            Some((_, tx)) => tx.send(response).is_ok(),
            None => false,
        }
    }

    /// Remove a pending interaction without resolving it.
    pub fn remove(&self, interaction_id: &str) -> bool {
        self.pending.remove(interaction_id).is_some()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn is_pending(&self, interaction_id: &str) -> bool {
        self.pending.contains_key(interaction_id)
    }

    /// Clear all pending registrations. Test-only: the registry is a
    /// process-global static shared across unit tests; call in setup/teardown
    /// to isolate concurrent tests from each other's entries.
    #[cfg(test)]
    pub fn reset_for_tests(&self) {
        self.pending.clear();
    }
}

impl Default for InteractionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide interaction registry.
pub fn interaction_registry() -> Arc<InteractionRegistry> {
    INTERACTIONS
        .get_or_init(|| Arc::new(InteractionRegistry::new()))
        .clone()
}

/// Register a pending interaction and return its id plus the wait channel.
pub fn register_interaction() -> (String, InteractionWait) {
    let interaction_id = wf_common::generate_id();
    let rx = interaction_registry().register(interaction_id.clone());
    (
        interaction_id.clone(),
        InteractionWait::new(interaction_id, rx),
    )
}

/// A pending interaction wait that self-cleans its registry entry on drop.
///
/// The waiting side normally resolves via [`complete_interaction`], which
/// removes the entry. If the waiter is cancelled or aborted (task abort, node
/// timeout selected away, workflow cancellation), the entry would otherwise be
/// leaked in the process-global registry forever. Dropping the wait removes it.
pub struct InteractionWait {
    interaction_id: String,
    rx: oneshot::Receiver<Value>,
}

impl InteractionWait {
    pub fn new(interaction_id: String, rx: oneshot::Receiver<Value>) -> Self {
        Self { interaction_id, rx }
    }
}

impl Drop for InteractionWait {
    fn drop(&mut self) {
        interaction_registry().remove(&self.interaction_id);
    }
}

impl Future for InteractionWait {
    type Output = Result<Value, oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use futures::FutureExt;
        self.rx.poll_unpin(cx)
    }
}

/// Resolve a pending interaction from outside the workflow execution.
pub fn complete_interaction(interaction_id: &str, response: Value) -> bool {
    interaction_registry().complete(interaction_id, response)
}

/// Remove a pending interaction without resolving it (waiter-side cleanup).
pub fn remove_interaction(interaction_id: &str) -> bool {
    interaction_registry().remove(interaction_id)
}

/// Serializes every test that touches the process-global interaction
/// registry. The registry is a `OnceLock` static shared across parallel unit
/// tests, so tests that register pending entries must not overlap; resets or
/// pending-count assertions are only deterministic under this lock.
#[cfg(test)]
pub(crate) static REGISTRY_TEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

/// Acquire the registry test lock. Hold the guard for the whole test body.
#[cfg(test)]
pub(crate) async fn acquire_registry_test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    REGISTRY_TEST_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_complete() {
        let _guard = acquire_registry_test_lock().await;
        interaction_registry().reset_for_tests();
        let (id, rx) = register_interaction();
        assert!(interaction_registry().is_pending(&id));
        let id_clone = id.clone();

        let tx = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            complete_interaction(&id_clone, Value::from("ok"));
        });
        let value = rx.await.expect("response should arrive");
        assert_eq!(value, Value::from("ok"));
        tx.await.unwrap();
        assert!(!interaction_registry().is_pending(&id));
        assert_eq!(interaction_registry().pending_count(), 0);
        interaction_registry().reset_for_tests();
    }

    #[tokio::test]
    async fn complete_unknown_returns_false() {
        let _guard = acquire_registry_test_lock().await;
        interaction_registry().reset_for_tests();
        assert!(!complete_interaction("no-such-interaction", Value::Null));
        interaction_registry().reset_for_tests();
    }

    #[tokio::test]
    async fn remove_cancels_wait() {
        let _guard = acquire_registry_test_lock().await;
        interaction_registry().reset_for_tests();
        let (id, rx) = register_interaction();
        assert!(interaction_registry().remove(&id));
        assert!(rx.await.is_err());
        interaction_registry().reset_for_tests();
    }

    #[tokio::test]
    async fn dropping_waiter_cleans_registry() {
        let _guard = acquire_registry_test_lock().await;
        interaction_registry().reset_for_tests();
        let id = {
            let (id, _rx) = register_interaction();
            assert!(interaction_registry().is_pending(&id));
            id
        };
        // The wait was dropped without completing: the registry entry must
        // have been removed (no leak).
        assert!(!interaction_registry().is_pending(&id));
        assert_eq!(interaction_registry().pending_count(), 0);
        interaction_registry().reset_for_tests();
    }

    #[tokio::test]
    async fn complete_after_timeout_returns_false() {
        let _guard = acquire_registry_test_lock().await;
        interaction_registry().reset_for_tests();
        let id = {
            let (id, rx) = register_interaction();
            // Simulate the handler timeout path: drop the wait side.
            drop(rx);
            id
        };
        // The entry is already cleaned; a late responder cannot resolve it.
        assert!(!complete_interaction(&id, Value::from("late")));
        assert!(!interaction_registry().is_pending(&id));
        interaction_registry().reset_for_tests();
    }
}
