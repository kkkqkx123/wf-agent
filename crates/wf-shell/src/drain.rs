//! Store-level coordination between the output reader threads and the
//! completion monitor.
//!
//! Two primitives: [`MonitorWakeup`], a bounded condvar wakeup the readers
//! signal when a command's output reaches EOF so the monitor detects exits
//! without polling, and [`OutputDrain`], a reader counter a waiter uses to
//! wait for the session output buffer to be fully drained after the process
//! has exited.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Store-level wakeup for the monitor thread. The output reader threads signal
/// it when a command's output reaches EOF, so the monitor can detect a process
/// exit immediately instead of waiting out the full poll interval. The monitor
/// waits on it with a bounded timeout, so it doubles as the periodic poller for
/// the rare case where EOF lags the exit (a descendant holding the output pipe
/// open).
pub(crate) struct MonitorWakeup {
    lock: Mutex<()>,
    cv: Condvar,
}

impl MonitorWakeup {
    pub(crate) fn new() -> Self {
        Self {
            lock: Mutex::new(()),
            cv: Condvar::new(),
        }
    }

    /// Wake a waiting monitor (and any future waiter) early.
    pub(crate) fn notify(&self) {
        let _guard = self.lock.lock().unwrap();
        self.cv.notify_all();
    }

    /// Block until notified or `timeout` elapses. The bounded timeout keeps
    /// progress even when no reader ever signals (daemon-descendant case).
    pub(crate) fn wait(&self, timeout: Duration) {
        let guard = self.lock.lock().unwrap();
        let _ = self.cv.wait_timeout(guard, timeout).unwrap();
    }
}

/// Tracks the number of active output reader threads so a waiter (e.g.
/// `execute_in_session`) can wait for the session output buffer to be fully
/// drained after the process has exited.
pub(crate) struct OutputDrain {
    remaining: AtomicUsize,
    lock: Mutex<()>,
    cv: Condvar,
    /// Store-level monitor wakeup, notified when the last reader reaches EOF
    /// so the monitor detects the exit right away (EOF-triggered completion).
    wakeup: Arc<MonitorWakeup>,
}

impl OutputDrain {
    pub(crate) fn new(wakeup: Arc<MonitorWakeup>) -> Self {
        Self {
            remaining: AtomicUsize::new(0),
            lock: Mutex::new(()),
            cv: Condvar::new(),
            wakeup,
        }
    }

    /// Register one output reader thread.
    pub(crate) fn add_reader(&self) {
        self.remaining.fetch_add(1, Ordering::SeqCst);
    }

    /// Mark one reader as finished (reached EOF). When the last reader
    /// finishes, waiting threads are woken and the store monitor is nudged so
    /// it checks for the process exit without waiting out its poll interval.
    pub(crate) fn mark_reader_done(&self) {
        if self.remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _guard = self.lock.lock().unwrap();
            self.cv.notify_all();
            self.wakeup.notify();
        }
    }

    /// Block until every reader has drained, or `timeout` elapses. A
    /// descendant process holding the output pipe open delays EOF past the
    /// actual exit; the timeout bounds the wait and the caller returns the
    /// output collected so far.
    pub(crate) fn wait_drained(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mut guard = self.lock.lock().unwrap();
        while self.remaining.load(Ordering::SeqCst) > 0 {
            let now = Instant::now();
            if now >= deadline {
                return;
            }
            let (g, _) = self.cv.wait_timeout(guard, deadline - now).unwrap();
            guard = g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_wakeup_notify_wakes_waiter() {
        // A reader signalling EOF must wake the monitor immediately rather
        // than leaving it blocked for the full poll interval.
        let wakeup = Arc::new(MonitorWakeup::new());
        let w = Arc::clone(&wakeup);
        let thread = std::thread::spawn(move || {
            let start = std::time::Instant::now();
            w.wait(Duration::from_secs(10));
            start.elapsed()
        });
        std::thread::sleep(Duration::from_millis(50));
        wakeup.notify();
        let elapsed = thread.join().unwrap();
        assert!(
            elapsed < Duration::from_secs(1),
            "waiter stayed blocked after notify: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_monitor_wakeup_wait_bounded_by_timeout() {
        // Even with no notify (a daemon descendant holding the pipe open), the
        // monitor must keep making progress via the poll-interval timeout.
        let wakeup = MonitorWakeup::new();
        let start = std::time::Instant::now();
        wakeup.wait(Duration::from_millis(100));
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(80),
            "returned early: {:?}",
            elapsed
        );
    }
}
