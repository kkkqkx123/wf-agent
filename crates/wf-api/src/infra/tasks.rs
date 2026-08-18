//! Registry of spawned execution driver tasks.
//!
//! Workflow driver tasks spawned by [`crate::workflow::workflow_execution::stream`]
//! are detached (`tokio::spawn` + no handle), so they can outlive their owner
//! for up to the full execution timeout. This registry keeps an abort handle
//! per execution so runtime teardown (`ApiContext::shutdown`) and stream
//! disconnects can hard-cancel the driver instead of letting it run to
//! completion.

use dashmap::DashMap;
use tokio::task::AbortHandle;

/// In-memory registry of in-flight execution driver tasks, keyed by
/// execution id. Tasks register on spawn and unregister when they finish;
/// `abort_all` (teardown) hard-cancels everything still running.
#[derive(Default)]
pub struct ExecutionTaskRegistry {
    tasks: DashMap<String, AbortHandle>,
}

impl ExecutionTaskRegistry {
    pub fn new() -> Self {
        Self {
            tasks: DashMap::new(),
        }
    }

    /// Record a spawned driver task under `id` (keeps only the abort handle;
    /// the task itself is detached).
    pub fn register(&self, id: impl Into<String>, handle: AbortHandle) {
        self.tasks.insert(id.into(), handle);
    }

    /// Forget a task that already finished. Returns whether it was tracked.
    pub fn unregister(&self, id: &str) -> bool {
        self.tasks.remove(id).is_some()
    }

    /// Abort the task for `id` and forget it. Returns whether it was tracked.
    pub fn abort(&self, id: &str) -> bool {
        match self.tasks.remove(id) {
            Some((_, handle)) => {
                handle.abort();
                true
            }
            None => false,
        }
    }

    /// Abort every tracked task (teardown path).
    pub fn abort_all(&self) {
        let ids: Vec<String> = self.tasks.iter().map(|entry| entry.key().clone()).collect();
        for id in ids {
            self.abort(&id);
        }
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_unregister_tracks_lifecycle() {
        let registry = ExecutionTaskRegistry::new();
        assert!(registry.is_empty());

        let handle =
            tokio::spawn(async { tokio::time::sleep(std::time::Duration::from_secs(5)).await });
        registry.register("exec-1", handle.abort_handle());
        assert_eq!(registry.len(), 1);

        assert!(registry.unregister("exec-1"));
        assert!(!registry.unregister("exec-1"));
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn abort_cancels_the_driver_task() {
        let registry = ExecutionTaskRegistry::new();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let _ = done_tx.send(());
        });
        registry.register("exec-1", handle.abort_handle());

        assert!(registry.abort("exec-1"));
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), done_rx)
            .await
            .expect("aborted task must be cancelled promptly");
        assert!(result.is_err(), "aborted task must not complete normally");
    }

    #[tokio::test]
    async fn abort_all_cancels_every_task() {
        let registry = ExecutionTaskRegistry::new();
        for i in 0..3 {
            let handle = tokio::spawn(async {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            });
            registry.register(format!("exec-{i}"), handle.abort_handle());
        }
        assert_eq!(registry.len(), 3);

        registry.abort_all();
        assert!(registry.is_empty(), "abort_all must clear the registry");
    }
}
