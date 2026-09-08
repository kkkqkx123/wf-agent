//! SIGINT double-press state machine: tracks two Ctrl+C presses within a
//! time window.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Window within which a second Ctrl+C counts as "interrupt".
pub const SIGINT_DOUBLE_PRESS_WINDOW: Duration = Duration::from_secs(5);

/// Monotonic origin for the real-clock press helper.
static PRESS_CLOCK_ORIGIN: OnceLock<Instant> = OnceLock::new();

/// Outcome of recording a press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressOutcome {
    /// First press inside a fresh window: warn ("again to interrupt") and
    /// e.g. clear the composer.
    FirstPress,
    /// Second press within the window: interrupt the turn / exit.
    SecondPress,
}

/// Pure two-press tracker with an injected millisecond clock, so synthetic
/// signals test the state machine without real SIGINT delivery.
#[derive(Debug, Clone)]
pub struct DoublePressTracker {
    window_ms: u64,
    last_press_ms: Option<u64>,
}

impl DoublePressTracker {
    /// New tracker with the given window (production code uses
    /// [`SIGINT_DOUBLE_PRESS_WINDOW`] / [`Self::default`]).
    pub fn new(window: Duration) -> Self {
        Self {
            window_ms: window.as_millis() as u64,
            last_press_ms: None,
        }
    }

    /// Record a press at `now_ms`; `SecondPress` only when the previous
    /// press is still inside the window.
    pub fn press(&mut self, now_ms: u64) -> PressOutcome {
        match self.last_press_ms {
            Some(last) if now_ms.saturating_sub(last) <= self.window_ms => {
                self.last_press_ms = None;
                PressOutcome::SecondPress
            }
            _ => {
                self.last_press_ms = Some(now_ms);
                PressOutcome::FirstPress
            }
        }
    }

    /// A first press is still pending (window not yet expired).
    pub fn pending(&self, now_ms: u64) -> bool {
        match self.last_press_ms {
            Some(last) => now_ms.saturating_sub(last) <= self.window_ms,
            None => false,
        }
    }

    /// Forget any pending press (e.g. after the turn finished).
    pub fn reset(&mut self) {
        self.last_press_ms = None;
    }

    /// Record a press using the real monotonic clock (process origin).
    pub fn press_now(&mut self) -> PressOutcome {
        let origin = PRESS_CLOCK_ORIGIN.get_or_init(Instant::now);
        self.press(Instant::now().duration_since(*origin).as_millis() as u64)
    }
}

impl Default for DoublePressTracker {
    fn default() -> Self {
        Self::new(SIGINT_DOUBLE_PRESS_WINDOW)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_press_tracker_transitions() {
        let mut t = DoublePressTracker::new(Duration::from_secs(5));
        assert_eq!(t.press(1_000), PressOutcome::FirstPress);
        assert!(t.pending(1_500));
        // Second press inside the window interrupts and consumes the state.
        assert_eq!(t.press(2_000), PressOutcome::SecondPress);
        assert!(!t.pending(2_000));
        // A third press starts a fresh window.
        assert_eq!(t.press(2_100), PressOutcome::FirstPress);
        // Outside the window it is a first press again.
        assert_eq!(t.press(2_100 + 5_001), PressOutcome::FirstPress);
        // The fresh first press is pending until its own window expires.
        assert!(t.pending(2_100 + 5_001 + 1));
        assert!(!t.pending(2_100 + 5_001 + 5_000 + 1));
    }

    #[test]
    fn double_press_tracker_reset_clears_pending() {
        let mut t = DoublePressTracker::new(Duration::from_secs(5));
        t.press(100);
        t.reset();
        assert!(!t.pending(101));
        assert_eq!(t.press(102), PressOutcome::FirstPress);
    }

    #[test]
    fn double_press_tracker_press_now_smoke() {
        let mut t = DoublePressTracker::default();
        assert_eq!(t.press_now(), PressOutcome::FirstPress);
        assert_eq!(t.press_now(), PressOutcome::SecondPress);
        assert_eq!(t.press_now(), PressOutcome::FirstPress);
    }
}
