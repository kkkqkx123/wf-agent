use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::{watch, Notify};

/// Timeout manager semantics:
/// - wall-clock timeouts for the whole agent loop
/// - pause timeouts (max_pause_duration), fired while the loop is paused
/// - pauseable handles so approval waits do not consume the budget
#[derive(Clone, Default)]
pub struct AgentTimeoutManager {
    inner: Arc<AgentTimeoutManagerInner>,
}

struct AgentTimeoutManagerInner {
    paused: watch::Sender<bool>,
    resume_notify: Arc<Notify>,
    handles: DashMap<String, Arc<TimeoutHandleInner>>,
}

impl Default for AgentTimeoutManagerInner {
    fn default() -> Self {
        let (paused, _) = watch::channel(false);
        Self {
            paused,
            resume_notify: Arc::new(Notify::new()),
            handles: DashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeoutOutcome {
    Fired,
    Cancelled,
}

/// Handle to an active timeout; cancelling it prevents the callback.
#[derive(Clone)]
pub struct TimeoutHandle {
    id: String,
    inner: Arc<TimeoutHandleInner>,
}

struct TimeoutHandleInner {
    cancelled: std::sync::atomic::AtomicBool,
}

impl TimeoutHandle {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn cancel(&self) {
        self.inner
            .cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner
            .cancelled
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl AgentTimeoutManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_paused(&self) -> bool {
        *self.inner.paused.borrow()
    }

    pub fn pause(&self) {
        self.inner.paused.send_replace(true);
    }

    pub fn resume(&self) {
        self.inner.paused.send_replace(false);
        self.inner.resume_notify.notify_waiters();
    }

    pub fn pause_handle(&self) -> AgentPauseGuard<'_> {
        self.pause();
        AgentPauseGuard { manager: self }
    }

    pub fn cancel(&self, id: &str) -> bool {
        self.inner
            .handles
            .remove(id)
            .map(|(_, handle)| {
                handle
                    .cancelled
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                true
            })
            .is_some()
    }

    pub fn handle_count(&self) -> usize {
        self.inner.handles.len()
    }

    /// Register a timeout. `on_timeout` runs once when `duration` of *active*
    /// (non-paused) time elapses. While the manager is paused the budget does
    /// not decrease.
    pub fn register(
        &self,
        id: impl Into<String>,
        duration: Duration,
        on_timeout: impl FnOnce() + Send + 'static,
    ) -> TimeoutHandle {
        let id = id.into();
        let handle_inner = Arc::new(TimeoutHandleInner {
            cancelled: std::sync::atomic::AtomicBool::new(false),
        });
        self.inner.handles.insert(id.clone(), handle_inner.clone());
        let task_handle_inner = handle_inner.clone();

        let mut paused_rx = self.inner.paused.subscribe();
        let resume_notify = self.inner.resume_notify.clone();
        let manager_handles = self.inner.handles.clone();
        let task_id = id.clone();

        tokio::spawn(async move {
            let mut remaining = duration;
            loop {
                if task_handle_inner
                    .cancelled
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    return;
                }
                if *paused_rx.borrow() {
                    if paused_rx.changed().await.is_err() {
                        return;
                    }
                    continue;
                }
                let start = Instant::now();
                tokio::select! {
                    _ = tokio::time::sleep(remaining) => {
                        if task_handle_inner
                            .cancelled
                            .load(std::sync::atomic::Ordering::SeqCst)
                        {
                            return;
                        }
                        on_timeout();
                        manager_handles.remove(&task_id);
                        return;
                    }
                    _ = paused_rx.changed() => {
                        remaining = remaining.saturating_sub(start.elapsed());
                    }
                    _ = resume_notify.notified() => {
                        remaining = remaining.saturating_sub(start.elapsed());
                    }
                }
            }
        });

        TimeoutHandle {
            id,
            inner: handle_inner,
        }
    }
}

/// RAII guard pausing the timeout manager for the scope of an approval wait.
pub struct AgentPauseGuard<'a> {
    manager: &'a AgentTimeoutManager,
}

impl Drop for AgentPauseGuard<'_> {
    fn drop(&mut self) {
        self.manager.resume();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_timeout_fires() {
        let manager = AgentTimeoutManager::new();
        let fired = Arc::new(AtomicU32::new(0));
        let fired_clone = fired.clone();
        manager.register("t1", Duration::from_millis(30), move || {
            fired_clone.fetch_add(1, Ordering::SeqCst);
        });
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_timeout_cancelled() {
        let manager = AgentTimeoutManager::new();
        let fired = Arc::new(AtomicU32::new(0));
        let fired_clone = fired.clone();
        let handle = manager.register("t1", Duration::from_millis(30), move || {
            fired_clone.fetch_add(1, Ordering::SeqCst);
        });
        handle.cancel();
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(fired.load(Ordering::SeqCst), 0);
        assert!(manager.cancel("t1"));
    }

    #[tokio::test]
    async fn test_pause_stops_budget() {
        let manager = AgentTimeoutManager::new();
        let fired = Arc::new(AtomicU32::new(0));
        let fired_clone = fired.clone();
        manager.register("t1", Duration::from_millis(50), move || {
            fired_clone.fetch_add(1, Ordering::SeqCst);
        });

        manager.pause();
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(fired.load(Ordering::SeqCst), 0);

        manager.resume();
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_pause_guard_resumes_on_drop() {
        let manager = AgentTimeoutManager::new();
        {
            let _guard = manager.pause_handle();
            assert!(manager.is_paused());
        }
        assert!(!manager.is_paused());
    }

    #[tokio::test]
    async fn test_register_cancel_via_manager() {
        let manager = AgentTimeoutManager::new();
        let fired = Arc::new(AtomicU32::new(0));
        let fired_clone = fired.clone();
        manager.register("t1", Duration::from_millis(30), move || {
            fired_clone.fetch_add(1, Ordering::SeqCst);
        });
        assert!(manager.cancel("t1"));
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(fired.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_pause_timeout_stops_entity() {
        use wf_core::interruption::InterruptionSignal;
        use wf_execution_shared::types::execution_entity::IExecutionEntity;
        use wf_types::Id;

        let entity = crate::entity::AgentLoopEntity::new(Id::from("pause-timeout-1".to_string()))
            .with_max_pause_duration(30);
        entity.state.write().await.start().unwrap();

        entity.pause().await.expect("pause must succeed");
        tokio::time::sleep(Duration::from_millis(120)).await;

        // Pause timeout fired -> interruption is stopped.
        assert!(entity.interruption().is_interrupted());
        assert_eq!(
            entity.interruption().check(),
            Some(InterruptionSignal::Stop)
        );

        // Resume clears the stop signal for a subsequent run.
        entity.resume().await.expect("resume must succeed");
        assert!(!entity.interruption().is_interrupted());
    }

    #[tokio::test]
    async fn test_resume_cancels_pause_timeout() {
        use wf_execution_shared::types::execution_entity::IExecutionEntity;
        use wf_types::Id;

        let entity = crate::entity::AgentLoopEntity::new(Id::from("pause-timeout-2".to_string()))
            .with_max_pause_duration(40);
        entity.state.write().await.start().unwrap();

        entity.pause().await.expect("pause must succeed");
        // Resume before the pause budget expires.
        tokio::time::sleep(Duration::from_millis(10)).await;
        entity.resume().await.expect("resume must succeed");
        tokio::time::sleep(Duration::from_millis(120)).await;

        assert!(!entity.interruption().is_interrupted());
    }
}
