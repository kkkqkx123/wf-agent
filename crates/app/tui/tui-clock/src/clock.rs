//! Injectable millisecond clock shared by animation, expiry and pacing.
//!
//! Production reads a monotonic process clock; tests enable a simulated
//! clock and advance it explicitly, so animation frames, notice expiry and
//! double-press windows become deterministic without sleeping. The free
//! [`now_ms`] helper is the single choke point: every direct
//! `Instant::now` read on an animation or expiry path must go through it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Width of one animation cache bucket in milliseconds. Animation moments
/// entering cache keys are discretized with [`anim_bucket`] so static
/// content stays cached while the spinner advances inside a bucket.
pub const ANIM_BUCKET_MS: u64 = 100;

/// Discretize a millisecond timestamp into an animation bucket.
pub fn anim_bucket(now_ms: u64) -> u64 {
    now_ms / ANIM_BUCKET_MS.max(1)
}

/// Process origin for the real clock.
static REAL_ORIGIN: OnceLock<Instant> = OnceLock::new();

/// Simulated millisecond value used while the test clock is enabled.
static TEST_NOW_MS: OnceLock<Mutex<u64>> = OnceLock::new();

/// Whether [`now_ms`] serves the simulated value.
static TEST_CLOCK_ENABLED: AtomicBool = AtomicBool::new(false);

fn real_now_ms() -> u64 {
    let origin = REAL_ORIGIN.get_or_init(Instant::now);
    Instant::now().duration_since(*origin).as_millis() as u64
}

/// Current millisecond timestamp. Serves the simulated clock while it is
/// enabled, otherwise the monotonic process clock.
pub fn now_ms() -> u64 {
    if TEST_CLOCK_ENABLED.load(Ordering::SeqCst) {
        return TEST_NOW_MS
            .get()
            .map(|m| *m.lock().unwrap_or_else(|e| e.into_inner()))
            .unwrap_or(0);
    }
    real_now_ms()
}

/// Enable the simulated clock starting at `start_ms`.
pub fn enable_test_clock(start_ms: u64) {
    let slot = TEST_NOW_MS.get_or_init(|| Mutex::new(start_ms));
    if let Ok(mut guard) = slot.lock() {
        *guard = start_ms;
    }
    TEST_CLOCK_ENABLED.store(true, Ordering::SeqCst);
}

/// Move the simulated clock forward by `delta_ms` (saturating).
pub fn advance_test_clock(delta_ms: u64) {
    if let Some(slot) = TEST_NOW_MS.get() {
        if let Ok(mut guard) = slot.lock() {
            *guard = guard.saturating_add(delta_ms);
        }
    }
}

/// Set the simulated clock to an absolute value.
pub fn set_test_clock(value_ms: u64) {
    if let Some(slot) = TEST_NOW_MS.get() {
        if let Ok(mut guard) = slot.lock() {
            *guard = value_ms;
        }
    }
}

/// Disable the simulated clock; [`now_ms`] serves the real clock again.
pub fn disable_test_clock() {
    TEST_CLOCK_ENABLED.store(false, Ordering::SeqCst);
}

/// Whether the simulated clock is currently enabled.
pub fn test_clock_enabled() -> bool {
    TEST_CLOCK_ENABLED.load(Ordering::SeqCst)
}

/// Injectable clock for structs that keep their own origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockKind {
    System,
    Test,
}

/// Small value-type clock: system reads the process clock, test reads the
/// simulated clock (which tests advance explicitly).
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    kind: ClockKind,
}

impl Clock {
    /// Clock reading the monotonic process time.
    pub fn system() -> Self {
        Self {
            kind: ClockKind::System,
        }
    }

    /// Clock reading the simulated test time.
    pub fn test() -> Self {
        Self {
            kind: ClockKind::Test,
        }
    }

    /// Current millisecond timestamp for this clock.
    pub fn now_ms(self) -> u64 {
        match self.kind {
            ClockKind::System => real_now_ms(),
            ClockKind::Test => TEST_NOW_MS
                .get()
                .map(|m| *m.lock().unwrap_or_else(|e| e.into_inner()))
                .unwrap_or_else(real_now_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that mutate the process-wide simulated clock.
    static CLOCK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_clock_is_deterministic() {
        let _lock = CLOCK_LOCK.lock().unwrap();
        enable_test_clock(1_000);
        assert_eq!(now_ms(), 1_000);
        advance_test_clock(250);
        assert_eq!(now_ms(), 1_250);
        set_test_clock(7);
        assert_eq!(now_ms(), 7);
        disable_test_clock();
    }

    #[test]
    fn anim_bucket_groups_within_a_hundred_ms() {
        assert_eq!(anim_bucket(0), anim_bucket(99));
        assert_ne!(anim_bucket(99), anim_bucket(100));
        assert_eq!(anim_bucket(10_000), 100);
    }

    #[test]
    fn test_clock_struct_reads_simulated_time() {
        let _lock = CLOCK_LOCK.lock().unwrap();
        enable_test_clock(42);
        assert_eq!(Clock::test().now_ms(), 42);
        disable_test_clock();
    }
}
