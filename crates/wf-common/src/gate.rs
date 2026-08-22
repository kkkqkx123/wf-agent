use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

/// How a [`ConcurrencyGate`] behaves when no permit is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AcquireStrategy {
    /// Block the caller until a permit becomes available.
    Wait,

    /// Reject the acquisition immediately (default). Prevents deadlocks
    /// from nested acquisition and keeps overflow predictable.
    #[default]
    Reject,
}

/// Gate-level failures only. Business errors are carried by the task return
/// value of the caller, not by this type.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum GateError {
    #[error("Gate closed or exhausted: {0}")]
    Closed(String),

    #[error("Acquire timed out: {0}")]
    Timeout(String),

    #[error("Cancelled: {0}")]
    Cancelled(String),
}

/// Snapshot of the gate counters. The accounting invariant is
/// `total_submitted = total_completed + total_failed + total_cancelled +
/// total_timed_out + active_count`.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GateStats {
    pub max_concurrent: usize,
    pub active_count: usize,
    pub available_permits: usize,
    pub total_submitted: u64,
    pub total_completed: u64,
    pub total_failed: u64,
    pub total_cancelled: u64,
    pub total_timed_out: u64,
}

/// A held permit occupies one concurrency slot; the slot is released when the
/// permit is dropped.
pub struct GatePermit {
    _permit: OwnedSemaphorePermit,
}

/// Decrements the gate's in-flight counter when dropped, keeping the run
/// accounting panic-safe.
struct ActiveGuard(Arc<AtomicUsize>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Permission-gated async execution primitive shared by the workflow and
/// agent layers. The gate only issues permits; queueing is an upper-layer
/// concern (e.g. the task scheduler).
pub struct ConcurrencyGate {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
    strategy: AcquireStrategy,
    active_count: Arc<AtomicUsize>,
    total_submitted: Arc<AtomicU64>,
    total_completed: Arc<AtomicU64>,
    total_failed: Arc<AtomicU64>,
    total_cancelled: Arc<AtomicU64>,
    total_timed_out: Arc<AtomicU64>,
}

impl ConcurrencyGate {
    /// Create a gate with the default strategy (`Reject`).
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            strategy: AcquireStrategy::default(),
            active_count: Arc::new(AtomicUsize::new(0)),
            total_submitted: Arc::new(AtomicU64::new(0)),
            total_completed: Arc::new(AtomicU64::new(0)),
            total_failed: Arc::new(AtomicU64::new(0)),
            total_cancelled: Arc::new(AtomicU64::new(0)),
            total_timed_out: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Override the acquisition strategy for this gate.
    pub fn with_strategy(mut self, strategy: AcquireStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Acquire a permit following the configured strategy.
    pub async fn acquire(&self) -> Result<GatePermit, GateError> {
        match self.strategy {
            AcquireStrategy::Wait => self.acquire_wait().await,
            AcquireStrategy::Reject => self.try_acquire(),
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
        Ok(GatePermit { _permit: permit })
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
        Ok(GatePermit { _permit: permit })
    }

    /// Acquire a permit, run `f` while holding it and return its output.
    /// A gate-level failure (closed/exhausted) is counted as `failed`; a
    /// successful run is counted as `completed`.
    pub async fn run<F, Fut, T>(&self, f: F) -> Result<T, GateError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        let _permit = match self.acquire().await {
            Ok(permit) => permit,
            Err(e) => {
                self.total_failed.fetch_add(1, Ordering::Relaxed);
                return Err(e);
            }
        };
        self.active_count.fetch_add(1, Ordering::Relaxed);
        let _guard = ActiveGuard(self.active_count.clone());
        let output = f().await;
        self.total_completed.fetch_add(1, Ordering::Relaxed);
        Ok(output)
    }

    /// Combinator: [`ConcurrencyGate::run`] with a wall-clock deadline.
    pub async fn run_with_timeout<F, Fut, T>(
        &self,
        duration: Duration,
        f: F,
    ) -> Result<T, GateError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        let _permit = match self.acquire().await {
            Ok(permit) => permit,
            Err(e) => {
                self.total_failed.fetch_add(1, Ordering::Relaxed);
                return Err(e);
            }
        };
        self.active_count.fetch_add(1, Ordering::Relaxed);
        let _guard = ActiveGuard(self.active_count.clone());
        match tokio::time::timeout(duration, f()).await {
            Ok(output) => {
                self.total_completed.fetch_add(1, Ordering::Relaxed);
                Ok(output)
            }
            Err(_) => {
                self.total_timed_out.fetch_add(1, Ordering::Relaxed);
                Err(GateError::Timeout(format!(
                    "task timed out after {duration:?}"
                )))
            }
        }
    }

    /// Combinator: [`ConcurrencyGate::run`] cancelled through an explicit
    /// token. Closing the gate never cancels in-flight tasks.
    pub async fn run_with_cancel<F, Fut, T>(
        &self,
        token: &CancellationToken,
        f: F,
    ) -> Result<T, GateError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        let _permit = match self.acquire().await {
            Ok(permit) => permit,
            Err(e) => {
                self.total_failed.fetch_add(1, Ordering::Relaxed);
                return Err(e);
            }
        };
        self.active_count.fetch_add(1, Ordering::Relaxed);
        let _guard = ActiveGuard(self.active_count.clone());
        tokio::select! {
            output = f() => {
                self.total_completed.fetch_add(1, Ordering::Relaxed);
                Ok(output)
            }
            _ = token.cancelled() => {
                self.total_cancelled.fetch_add(1, Ordering::Relaxed);
                Err(GateError::Cancelled("task cancelled".to_string()))
            }
        }
    }

    /// Reject all future permit acquisitions; already-held permits and
    /// in-flight tasks are unaffected.
    pub fn close(&self) {
        self.semaphore.close();
    }

    pub fn is_closed(&self) -> bool {
        self.semaphore.is_closed()
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    pub fn strategy(&self) -> AcquireStrategy {
        self.strategy
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Number of run-family executions currently in flight.
    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::Relaxed)
    }

    pub fn stats(&self) -> GateStats {
        GateStats {
            max_concurrent: self.max_concurrent,
            active_count: self.active_count.load(Ordering::Relaxed),
            available_permits: self.semaphore.available_permits(),
            total_submitted: self.total_submitted.load(Ordering::Relaxed),
            total_completed: self.total_completed.load(Ordering::Relaxed),
            total_failed: self.total_failed.load(Ordering::Relaxed),
            total_cancelled: self.total_cancelled.load(Ordering::Relaxed),
            total_timed_out: self.total_timed_out.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_default_strategy_is_reject() {
        let gate = ConcurrencyGate::new(1);
        let _first = gate.try_acquire().expect("first permit");
        assert!(matches!(
            gate.try_acquire(),
            Err(GateError::Closed(_))
        ));
        assert!(matches!(gate.acquire().await, Err(GateError::Closed(_))));
    }

    #[tokio::test]
    async fn test_reject_strategy_acquire_fails_immediately() {
        let gate = ConcurrencyGate::new(1);
        let _first = gate.acquire_wait().await.expect("first permit");
        assert!(matches!(
            gate.acquire().await,
            Err(GateError::Closed(_))
        ));
    }

    #[tokio::test]
    async fn test_wait_strategy_blocks_until_permit_available() {
        let gate = Arc::new(
            ConcurrencyGate::new(1).with_strategy(AcquireStrategy::Wait),
        );
        let first = gate.acquire().await.expect("first permit");

        let gate2 = gate.clone();
        let entered = Arc::new(AtomicUsize::new(0));
        let entered2 = entered.clone();
        let handle = tokio::spawn(async move {
            let _second = gate2.acquire().await.expect("permit after release");
            entered2.fetch_add(1, Ordering::SeqCst);
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(entered.load(Ordering::SeqCst), 0, "waiting, not entered");

        drop(first);
        handle.await.expect("waiter task");
        assert_eq!(entered.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_wait_strategy_rejects_when_gate_closed() {
        let gate = ConcurrencyGate::new(1).with_strategy(AcquireStrategy::Wait);
        gate.close();
        assert!(matches!(
            gate.acquire().await,
            Err(GateError::Closed(_))
        ));
    }

    #[tokio::test]
    async fn test_try_acquire_respects_capacity() {
        let gate = ConcurrencyGate::new(2);
        let _a = gate.try_acquire().expect("permit a");
        let _b = gate.try_acquire().expect("permit b");
        assert!(matches!(gate.try_acquire(), Err(GateError::Closed(_))));
        drop(_a);
        let _c = gate.try_acquire().expect("permit after release");
    }

    #[tokio::test]
    async fn test_close_keeps_in_flight_permits() {
        let gate = ConcurrencyGate::new(1);
        let permit = gate.try_acquire().expect("permit");
        gate.close();
        assert!(gate.is_closed());
        assert!(matches!(gate.try_acquire(), Err(GateError::Closed(_))));
        drop(permit);
        // A closed gate never admits new permits, even after release.
        assert!(matches!(gate.try_acquire(), Err(GateError::Closed(_))));
    }

    #[tokio::test]
    async fn test_run_success_counts_completed() {
        let gate = ConcurrencyGate::new(2);
        let out = gate.run(|| async { 42 }).await.expect("run");
        assert_eq!(out, 42);
        let stats = gate.stats();
        assert_eq!(stats.total_submitted, 1);
        assert_eq!(stats.total_completed, 1);
        assert_eq!(stats.total_failed, 0);
        assert_eq!(stats.active_count, 0);
    }

    #[tokio::test]
    async fn test_run_failure_counts_failed() {
        let gate = ConcurrencyGate::new(1);
        let _permit = gate.try_acquire().expect("permit");
        let err = gate.run(|| async { "unreachable" }).await;
        assert!(matches!(err, Err(GateError::Closed(_))));
        let stats = gate.stats();
        assert_eq!(stats.total_submitted, 1);
        assert_eq!(stats.total_failed, 1);
        assert_eq!(stats.total_completed, 0);
    }

    #[tokio::test]
    async fn test_run_with_timeout_success() {
        let gate = ConcurrencyGate::new(2);
        let out = gate
            .run_with_timeout(Duration::from_secs(1), || async { "done" })
            .await
            .expect("run");
        assert_eq!(out, "done");
        let stats = gate.stats();
        assert_eq!(stats.total_completed, 1);
        assert_eq!(stats.total_timed_out, 0);
    }

    #[tokio::test]
    async fn test_run_with_timeout_expired_counts_timed_out() {
        let gate = ConcurrencyGate::new(2);
        let err = gate
            .run_with_timeout(Duration::from_millis(20), || async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                "never returned"
            })
            .await;
        assert!(matches!(err, Err(GateError::Timeout(_))));
        let stats = gate.stats();
        assert_eq!(stats.total_timed_out, 1);
        assert_eq!(stats.total_completed, 0);
    }

    #[tokio::test]
    async fn test_run_with_cancel_success() {
        let gate = ConcurrencyGate::new(2);
        let token = CancellationToken::new();
        let out = gate
            .run_with_cancel(&token, || async { 7 })
            .await
            .expect("run");
        assert_eq!(out, 7);
        let stats = gate.stats();
        assert_eq!(stats.total_completed, 1);
        assert_eq!(stats.total_cancelled, 0);
    }

    #[tokio::test]
    async fn test_run_with_cancel_counts_cancelled() {
        let gate = Arc::new(ConcurrencyGate::new(2));
        let token = CancellationToken::new();
        let token2 = token.clone();
        let gate2 = gate.clone();
        let handle = tokio::spawn(async move {
            gate2.run_with_cancel(&token2, || async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                "never returned"
            })
            .await
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        token.cancel();
        let err = handle.await.expect("task");
        assert!(matches!(err, Err(GateError::Cancelled(_))));
        let stats = gate.stats();
        assert_eq!(stats.total_cancelled, 1);
        assert_eq!(stats.total_completed, 0);
    }

    #[tokio::test]
    async fn test_stats_invariant_holds() {
        let gate = ConcurrencyGate::new(1);
        let _permit = gate.try_acquire().expect("permit");
        // The raw permit holder is visible through available_permits but is
        // not a run-family execution, so it does not count as active.
        assert_eq!(gate.available_permits(), 0);
        let _ = gate.run(|| async { "rejected" }).await;
        let stats = gate.stats();
        assert_eq!(
            stats.total_submitted,
            stats.total_completed
                + stats.total_failed
                + stats.total_cancelled
                + stats.total_timed_out
                + stats.active_count as u64
        );
        assert_eq!(stats.total_submitted, 1);
        assert_eq!(stats.total_failed, 1);
        assert_eq!(stats.total_completed, 0);
        assert_eq!(stats.active_count, 0);
        assert_eq!(stats.available_permits, 0);
    }

    #[tokio::test]
    async fn test_concurrent_runs_respect_limit() {
        let gate = Arc::new(ConcurrencyGate::new(3).with_strategy(AcquireStrategy::Wait));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..10 {
            let gate = gate.clone();
            let peak = peak.clone();
            handles.push(tokio::spawn(async move {
                gate.run(move || async move {
                    let current = peak.fetch_add(1, Ordering::SeqCst) + 1;
                    assert!(current <= 3, "concurrency exceeded limit: {current}");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    peak.fetch_sub(1, Ordering::SeqCst);
                })
                .await
                .expect("run");
            }));
        }
        for handle in handles {
            handle.await.expect("task");
        }
        let stats = gate.stats();
        assert_eq!(stats.total_submitted, 10);
        assert_eq!(stats.total_completed, 10);
        assert_eq!(stats.total_failed, 0);
        assert_eq!(stats.active_count, 0);
    }
}
