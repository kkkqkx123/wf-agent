use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI64, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

/// Shared handle to a manually driven clock: tests set, advance, or fail
/// the time observed through the [`CheckpointClock`] built from it. Clones
/// share the same underlying state.
#[derive(Debug, Clone)]
pub struct ManualClock {
    state: Arc<ManualClockState>,
}

#[derive(Debug)]
struct ManualClockState {
    current_ms: AtomicI64,
    failed: AtomicBool,
}

impl ManualClock {
    pub fn new(start_ms: i64) -> Self {
        Self {
            state: Arc::new(ManualClockState {
                current_ms: AtomicI64::new(start_ms),
                failed: AtomicBool::new(false),
            }),
        }
    }

    /// Jump to an explicit timestamp (covers clock rewind scenarios).
    pub fn set(&self, ms: i64) {
        self.state.current_ms.store(ms, Ordering::SeqCst);
    }

    /// Move forward by `delta_ms` (saturates instead of wrapping).
    pub fn advance(&self, delta_ms: i64) {
        self.state
            .current_ms
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                Some(current.saturating_add(delta_ms))
            })
            .expect("saturating add always succeeds");
    }

    /// Inject a clock failure: reads return `None` until [`Self::restore`].
    pub fn fail(&self) {
        self.state.failed.store(true, Ordering::SeqCst);
    }

    /// Clear an injected failure.
    pub fn restore(&self) {
        self.state.failed.store(false, Ordering::SeqCst);
    }

    /// Current manual time (Unix milliseconds), ignoring failure injection.
    /// Tests use it to stamp rows that bypass the clocked creation paths.
    pub fn current_ms(&self) -> i64 {
        self.state.current_ms.load(Ordering::SeqCst)
    }

    fn now_ms(&self) -> Option<i64> {
        if self.state.failed.load(Ordering::SeqCst) {
            return None;
        }
        Some(self.state.current_ms.load(Ordering::SeqCst))
    }
}

/// Single injectable time source for checkpoint-semantic timestamps (Unix
/// milliseconds). Production code holds `System`; tests hold `Manual` and
/// drive time explicitly instead of sleeping. Monotonic `Instant`
/// measurements (debounce, timeouts, latency metrics) are intentionally out
/// of scope and stay on the system clock.
#[derive(Debug, Clone)]
pub enum CheckpointClock {
    System,
    Manual(ManualClock),
}

impl CheckpointClock {
    pub fn system() -> Self {
        Self::System
    }

    /// Build a manual clock starting at `start_ms` (Unix milliseconds).
    pub fn manual(start_ms: i64) -> Self {
        Self::Manual(ManualClock::new(start_ms))
    }

    /// The manual handle when this clock is manually driven; `None` for the
    /// system clock. Tests advance time through the returned handle.
    pub fn manual_handle(&self) -> Option<ManualClock> {
        match self {
            Self::System => None,
            Self::Manual(clock) => Some(clock.clone()),
        }
    }

    /// Current Unix milliseconds, or `None` when the system clock is
    /// unavailable/before the epoch or a manual failure is injected.
    /// Callers fail closed on `None` (miss, reject, or error — never a
    /// sentinel timestamp).
    pub fn now_ms(&self) -> Option<i64> {
        match self {
            Self::System => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as i64),
            Self::Manual(clock) => clock.now_ms(),
        }
    }
}

impl Default for CheckpointClock {
    fn default() -> Self {
        Self::system()
    }
}

/// Whether a clock reading is usable: present and not before the epoch.
pub fn clock_valid(now: Option<i64>) -> bool {
    now.is_some_and(|ms| ms >= 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_drives_reads() {
        let clock = CheckpointClock::manual(1_000);
        assert_eq!(clock.now_ms(), Some(1_000));
        let handle = clock.manual_handle().expect("manual clock has a handle");
        handle.advance(500);
        assert_eq!(clock.now_ms(), Some(1_500));
        handle.set(900);
        assert_eq!(clock.now_ms(), Some(900));
    }

    #[test]
    fn manual_failure_reads_none_until_restored() {
        let clock = CheckpointClock::manual(1_000);
        let handle = clock.manual_handle().expect("manual clock has a handle");
        handle.fail();
        assert_eq!(clock.now_ms(), None);
        handle.restore();
        assert_eq!(clock.now_ms(), Some(1_000));
    }

    #[test]
    fn system_clock_has_no_manual_handle() {
        assert!(CheckpointClock::system().manual_handle().is_none());
    }

    #[test]
    fn clock_valid_rejects_missing_and_negative() {
        assert!(clock_valid(Some(0)));
        assert!(!clock_valid(None));
        assert!(!clock_valid(Some(-1)));
    }
}
