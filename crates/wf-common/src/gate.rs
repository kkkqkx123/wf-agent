use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Gate-level failures. Only the gate-closed case is representable: business
/// errors are carried by the task return value of the caller, not by this
/// type.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum GateError {
    #[error("Gate closed or exhausted: {0}")]
    Closed(String),
}

/// Snapshot of the gate counters. The only in-gate maintained counter is
/// `active_count`, incremented on acquisition and decremented when the
/// permit is dropped. Outcome accounting (completed/failed/cancelled/...) is
/// owned by the upper layers that observe terminal states.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GateStats {
    pub max_concurrent: usize,
    pub active_count: usize,
    pub available_permits: usize,
}

/// A held permit occupies one concurrency slot; the slot and the active
/// counter are released when the permit is dropped.
pub struct GatePermit {
    _permit: OwnedSemaphorePermit,
    active: Arc<AtomicUsize>,
}

impl Drop for GatePermit {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Permission-gated async execution primitive shared by the workflow and
/// agent layers. The gate only issues permits; queueing is an upper-layer
/// concern (e.g. the task scheduler).
pub struct ConcurrencyGate {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
    active_count: Arc<AtomicUsize>,
}

impl ConcurrencyGate {
    /// Create a gate with the given concurrency limit.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            active_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Acquire a permit, waiting until one is available or the gate closes.
    pub async fn acquire_wait(&self) -> Result<GatePermit, GateError> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| GateError::Closed("gate is closed".to_string()))?;
        Ok(GatePermit {
            _permit: permit,
            active: self.active_count.clone(),
        })
    }

    /// Acquire a permit without waiting; fails when the gate is exhausted or
    /// closed.
    pub fn try_acquire(&self) -> Result<GatePermit, GateError> {
        let permit = self.semaphore.clone().try_acquire_owned().map_err(|e| {
            GateError::Closed(match e {
                tokio::sync::TryAcquireError::Closed => "gate is closed".to_string(),
                tokio::sync::TryAcquireError::NoPermits => {
                    format!("no permit available (max concurrent {})", self.max_concurrent)
                }
            })
        })?;
        Ok(GatePermit {
            _permit: permit,
            active: self.active_count.clone(),
        })
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub fn stats(&self) -> GateStats {
        GateStats {
            max_concurrent: self.max_concurrent,
            active_count: self.active_count.load(Ordering::Relaxed),
            available_permits: self.semaphore.available_permits(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_acquire_wait_blocks_until_permit_available() {
        let gate = Arc::new(ConcurrencyGate::new(1));
        let first = gate.acquire_wait().await.expect("first permit");

        let gate2 = gate.clone();
        let entered = Arc::new(AtomicUsize::new(0));
        let entered2 = entered.clone();
        let handle = tokio::spawn(async move {
            let _second = gate2.acquire_wait().await.expect("permit after release");
            entered2.fetch_add(1, Ordering::SeqCst);
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(entered.load(Ordering::SeqCst), 0, "waiting, not entered");

        drop(first);
        handle.await.expect("waiter task");
        assert_eq!(entered.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_try_acquire_respects_capacity() {
        let gate = ConcurrencyGate::new(2);
        let _a = gate.try_acquire().expect("permit a");
        let _b = gate.try_acquire().expect("permit b");
        assert!(matches!(gate.try_acquire(), Err(GateError::Closed(_))));
        drop(_a);
        let _c = gate.try_acquire().expect("permit after release");
    }

    #[test]
    fn test_active_count_tracks_held_permits() {
        let gate = ConcurrencyGate::new(3);
        assert_eq!(gate.stats().active_count, 0);
        let a = gate.try_acquire().expect("permit a");
        let b = gate.try_acquire().expect("permit b");
        assert_eq!(gate.stats().active_count, 2);
        drop(a);
        assert_eq!(gate.stats().active_count, 1);
        drop(b);
        assert_eq!(gate.stats().active_count, 0);
    }
}
