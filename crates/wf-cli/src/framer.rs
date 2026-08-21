//! Frame scheduling: merge draw requests and cap the redraw rate
//! (120 FPS cap / 8.3 ms minimum interval).
//!
//! All time values are **millisecond integers** and the "clock" is an
//! injectable value advanced by the caller via [`FrameRequester::set_now`],
//! so the throttle math is fully unit-testable without a real timer. The
//! event loop polls [`FrameRequester::deadline`] and sleeps
//! until then; when it fires, it calls [`FrameRequester::frame_done`].

use std::time::Duration;

/// Nominal 120 FPS floor (≈ 8.3 ms); 9 ms is the next integer strictly
/// above 1000/120, so a real 1-ms ticker never trips the limiter early.
pub const MIN_FRAME_INTERVAL_MS: u64 = 9;

/// Pure rate-limit math: the earliest timestamp a *next* frame may be
/// emitted, given the last emitted frame time. Owns no state by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameRateLimiter {
    pub min_interval_ms: u64,
}

impl FrameRateLimiter {
    /// Default limiter enforcing the 120 FPS floor.
    pub const fn frame_rate() -> Self {
        Self::new(MIN_FRAME_INTERVAL_MS)
    }

    pub const fn new(min_interval_ms: u64) -> Self {
        Self { min_interval_ms }
    }

    /// The earliest allowed emission timestamp (ms) after `last_done`.
    /// Returns `None` when either no frame has been emitted yet, or the
    /// floor for `now` has already been reached (i.e. a frame is eligible
    /// *right now*).
    pub fn next_deadline(&self, now: u64, last_done: Option<u64>) -> Option<u64> {
        match last_done {
            Some(last) => {
                let floor = last.saturating_add(self.min_interval_ms);
                (floor > now).then_some(floor)
            }
            None => None,
        }
    }
}

/// Draw-request accumulator that merges many spontaneous requests into a
/// single next `deadline` and enforces the rate floor.
#[derive(Debug, Clone)]
pub struct FrameRequester {
    pending: Option<u64>,
    last_done: Option<u64>,
    limiter: FrameRateLimiter,
    now: u64,
}

impl FrameRequester {
    pub fn new(now: u64) -> Self {
        Self {
            pending: None,
            last_done: None,
            limiter: FrameRateLimiter::frame_rate(),
            now,
        }
    }

    /// Inject the current clock (ms). Tests advance this to simulate time
    /// passing; the real loop updates it before each [`Self::deadline`] poll.
    pub fn set_now(&mut self, now: u64) {
        self.now = now;
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    /// Ask for a frame as soon as the rate limiter allows.
    pub fn request_frame(&mut self) {
        self.pending = Some(self.pending.map_or(self.now, |p| p.min(self.now)));
    }

    /// Ask for a frame no earlier than `dur` from the current clock.
    pub fn request_frame_in(&mut self, dur: Duration) {
        let d = self.now.saturating_add(
            u64::try_from(dur.as_millis()).unwrap_or(u64::MAX),
        );
        self.pending = Some(self.pending.map_or(d, |p| p.min(d)));
    }

    /// Whether a draw has been requested but not yet emitted.
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The effective next draw time, being the *later* of the merged
    /// request and the rate-limit floor. `None` means nothing is due and a
    /// frame may run immediately (or not at all if nothing was requested).
    pub fn deadline(&self) -> Option<u64> {
        let floor = self.limiter.next_deadline(self.now, self.last_done);
        let eff = match (self.pending, floor) {
            (None, None) => return None,
            (Some(p), None) => p,
            (Some(p), Some(f)) => p.max(f),
            (None, Some(f)) => f,
        };
        (eff > self.now).then_some(eff)
    }

    /// Record that a frame was emitted; clears the pending request.
    pub fn frame_done(&mut self) {
        self.last_done = Some(self.now);
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_floor_before_first_frame() {
        let limiter = FrameRateLimiter::frame_rate();
        assert_eq!(limiter.next_deadline(100, None), None);
    }

    #[test]
    fn floor_kicks_in_after_a_frame() {
        let limiter = FrameRateLimiter::frame_rate();
        // Frame done at t=100; at t=105 the next admissible time is 109.
        assert_eq!(limiter.next_deadline(105, Some(100)), Some(109));
        // Reached the floor: eligible immediately.
        assert_eq!(limiter.next_deadline(109, Some(100)), None);
        assert_eq!(limiter.next_deadline(200, Some(100)), None);
    }

    #[test]
    fn request_merges_to_earliest() {
        let mut f = FrameRequester::new(0);
        f.request_frame_in(Duration::from_millis(50));
        // A second, later request must not push the deadline out.
        f.request_frame_in(Duration::from_millis(120));
        f.set_now(0);
        assert_eq!(f.deadline(), Some(50));
    }

    #[test]
    fn immediate_request_is_due_now() {
        let mut f = FrameRequester::new(10);
        f.request_frame();
        // Floor is absent (no prior frame), request is at 10 == now → None.
        assert_eq!(f.deadline(), None, "first frame is immediately eligible");
        assert!(f.pending());
    }

    #[test]
    fn rate_limiter_delays_second_frame() {
        let mut f = FrameRequester::new(0);
        f.frame_done(); // last_done = 0; floor = 9.
        f.request_frame();
        assert_eq!(f.deadline(), Some(9), "must wait for the 9 ms floor");
    }

    #[test]
    fn frame_done_clears_pending() {
        let mut f = FrameRequester::new(0);
        f.request_frame();
        assert!(f.pending());
        f.frame_done();
        assert!(!f.pending());
        // No new request: the only pending deadline is the rate-limit floor.
        assert_eq!(f.deadline(), Some(9));
    }

    #[test]
    fn empty_requester_has_no_deadline() {
        let mut f = FrameRequester::new(5);
        f.set_now(100);
        assert_eq!(f.deadline(), None);
    }
}