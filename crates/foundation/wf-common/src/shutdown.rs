//! Process-wide active-shutdown marker.
//!
//! The host runtime sets this flag when it begins an intentional teardown
//! (user quit, SIGINT, embedded runtime close). Engine dispatch consults it
//! so executions that are still settling while the host closes are not
//! recorded or mirrored as `Failed` runs that would later look like errors or
//! trigger background retries.

use std::sync::atomic::{AtomicBool, Ordering};

/// True once an active shutdown is underway. Never reset in production: a
/// process that began closing does not resume normal operation.
static ACTIVE_SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Mark an active shutdown as underway.
pub fn begin_active_shutdown() {
    ACTIVE_SHUTDOWN.store(true, Ordering::SeqCst);
}

/// Whether an active shutdown is underway.
pub fn is_active_shutdown() -> bool {
    ACTIVE_SHUTDOWN.load(Ordering::SeqCst)
}

/// Clear the marker. Only test code needs this: a process that began closing
/// does not resume normal operation in production.
pub fn clear_active_shutdown() {
    ACTIVE_SHUTDOWN.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_roundtrips() {
        clear_active_shutdown();
        assert!(!is_active_shutdown());
        begin_active_shutdown();
        assert!(is_active_shutdown());
        clear_active_shutdown();
        assert!(!is_active_shutdown());
    }
}
