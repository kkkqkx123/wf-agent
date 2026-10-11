//! Generic circuit breaker shared by executors that need failure tripping.
//!
//! Has no REST-specific dependencies so any executor (or future transport)
//! can reuse it.

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CIRCUIT_CLOSED: u8 = 0;
const CIRCUIT_OPEN: u8 = 1;
const CIRCUIT_HALF_OPEN: u8 = 2;

/// Failure-count circuit breaker with a reset timeout.
///
/// Trips open after `failure_threshold` consecutive failures, then stays open
/// for `reset_timeout` before allowing a single half-open probe.
pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    state: AtomicU8,
    failures: AtomicU32,
    half_open_successes: AtomicU32,
    half_open_required: u32,
    last_failure: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            state: AtomicU8::new(CIRCUIT_CLOSED),
            failures: AtomicU32::new(0),
            half_open_successes: AtomicU32::new(0),
            half_open_required: 1,
            last_failure: Mutex::new(None),
        }
    }

    pub fn is_allowed(&self) -> bool {
        match self.state.load(Ordering::SeqCst) {
            CIRCUIT_CLOSED => true,
            CIRCUIT_OPEN => {
                let last = *wf_common::lock::lock_ok(self.last_failure.lock());
                if let Some(t) = last {
                    if t.elapsed() >= self.reset_timeout {
                        self.state.store(CIRCUIT_HALF_OPEN, Ordering::SeqCst);
                        self.half_open_successes.store(0, Ordering::SeqCst);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CIRCUIT_HALF_OPEN => true,
            _ => false,
        }
    }

    pub fn record_success(&self) {
        match self.state.load(Ordering::SeqCst) {
            CIRCUIT_HALF_OPEN => {
                let successes = self.half_open_successes.fetch_add(1, Ordering::SeqCst) + 1;
                if successes >= self.half_open_required {
                    self.state.store(CIRCUIT_CLOSED, Ordering::SeqCst);
                    self.failures.store(0, Ordering::SeqCst);
                }
            }
            CIRCUIT_CLOSED => {
                self.failures.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    pub fn record_failure(&self) {
        *wf_common::lock::lock_ok(self.last_failure.lock()) = Some(Instant::now());
        match self.state.load(Ordering::SeqCst) {
            CIRCUIT_HALF_OPEN => {
                self.state.store(CIRCUIT_OPEN, Ordering::SeqCst);
            }
            CIRCUIT_CLOSED => {
                let failures = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= self.failure_threshold {
                    self.state.store(CIRCUIT_OPEN, Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }

    pub fn state(&self) -> &str {
        match self.state.load(Ordering::SeqCst) {
            CIRCUIT_CLOSED => "closed",
            CIRCUIT_OPEN => "open",
            CIRCUIT_HALF_OPEN => "half_open",
            _ => "unknown",
        }
    }
}
