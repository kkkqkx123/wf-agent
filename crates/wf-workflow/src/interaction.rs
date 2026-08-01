//! Asynchronous user interaction resolution for USER_INTERACTION nodes.
//!
//! A node publishes an interaction request (interaction id + prompt) on the
//! event bus and then waits for the response to be injected through
//! [`complete_interaction`]. External responders (applications, test
//! harnesses, human-in-the-loop UIs) resolve pending interactions by id.

use std::sync::{Arc, OnceLock};

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
pub fn register_interaction() -> (String, oneshot::Receiver<Value>) {
    let interaction_id = wf_common::generate_id();
    let rx = interaction_registry().register(interaction_id.clone());
    (interaction_id, rx)
}

/// Resolve a pending interaction from outside the workflow execution.
pub fn complete_interaction(interaction_id: &str, response: Value) -> bool {
    interaction_registry().complete(interaction_id, response)
}

/// Remove a pending interaction without resolving it (waiter-side cleanup).
pub fn remove_interaction(interaction_id: &str) -> bool {
    interaction_registry().remove(interaction_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_complete() {
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
    }

    #[tokio::test]
    async fn complete_unknown_returns_false() {
        assert!(!complete_interaction("no-such-interaction", Value::Null));
    }

    #[tokio::test]
    async fn remove_cancels_wait() {
        let (id, rx) = register_interaction();
        assert!(interaction_registry().remove(&id));
        assert!(rx.await.is_err());
    }
}
