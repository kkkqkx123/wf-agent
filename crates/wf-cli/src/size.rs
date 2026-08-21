//! Terminal resize debouncing.
//!
//! A burst of `Event::Resize` (during a drag) is collapsed into a single
//! final size handed to the caller after the storm settles. The window is
//! configurable (default 75 ms); `now` is an injected millisecond clock so
//! the debounce timing is deterministic in tests. The *reflow* itself stays
//! with the caller: once [`ResizeDebouncer::settle_if_elapsed`] yields a
//! size, the event loop re-runs `HistoryLine::display_lines(width)`.

use std::time::Duration;

/// Debounce window used when the caller does not supply one (75 ms).
pub const DEFAULT_DEBOUNCE_MS: u64 = 75;

/// The final settled terminal size in columns × rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

impl Size {
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }
}

/// Collapses a burst of resize notifications into one final settled size.
#[derive(Debug, Clone)]
pub struct ResizeDebouncer {
    window_ms: u64,
    pending: Option<Size>,
    last_change: Option<u64>,
}

impl ResizeDebouncer {
    /// Default debouncer with a 75 ms window.
    pub fn default_window() -> Self {
        Self::new(Duration::from_millis(DEFAULT_DEBOUNCE_MS))
    }

    pub fn new(window: Duration) -> Self {
        Self {
            window_ms: u64::try_from(window.as_millis()).unwrap_or(u64::MAX),
            pending: None,
            last_change: None,
        }
    }

    /// Record the latest candidate size and stamp the current time.
    /// Repeated calls within the window replace the pending size.
    pub fn push(&mut self, size: Size, now: u64) {
        self.pending = Some(size);
        self.last_change = Some(now);
    }

    /// The size currently awaiting settlement, if any.
    pub fn pending(&self) -> Option<Size> {
        self.pending
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// If the last change is at least `window` old, return the final size
    /// (consuming it); otherwise `None`. Only the first call past the window
    /// yields the size — later calls report `None` until a new `push`.
    pub fn settle_if_elapsed(&mut self, now: u64) -> Option<Size> {
        let last = self.last_change?;
        if now.saturating_sub(last) >= self.window_ms {
            let size = self.pending.take();
            self.last_change = None;
            size
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_when_nothing_pending() {
        let mut d = ResizeDebouncer::default_window();
        assert_eq!(d.settle_if_elapsed(0), None);
        assert!(!d.is_pending());
    }

    #[test]
    fn settles_after_window_elapses() {
        let mut d = ResizeDebouncer::new(Duration::from_millis(75));
        d.push(Size::new(120, 40), 1_000);
        assert_eq!(d.settle_if_elapsed(1_050), None, "within window");
        assert_eq!(d.settle_if_elapsed(1_075), Some(Size::new(120, 40)));
        assert_eq!(d.settle_if_elapsed(1_100), None, "settled already");
    }

    #[test]
    fn later_push_wins() {
        let mut d = ResizeDebouncer::new(Duration::from_millis(75));
        d.push(Size::new(80, 24), 100);
        d.push(Size::new(140, 50), 110); // drag slop, 10 ms later
        assert_eq!(d.settle_if_elapsed(200), Some(Size::new(140, 50)));
    }

    #[test]
    fn settle_is_one_shot_per_push() {
        let mut d = ResizeDebouncer::default_window();
        d.push(Size::new(10, 10), 0);
        assert_eq!(d.settle_if_elapsed(75), Some(Size::new(10, 10)));
        assert_eq!(d.settle_if_elapsed(1000), None);
    }
}