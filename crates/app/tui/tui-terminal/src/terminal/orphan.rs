//! Orphan detection: decide when the client loop should exit because no
//! input can ever arrive again.

/// True when a controlling terminal is reachable via `/dev/tty`.
pub fn has_controlling_terminal() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .is_ok()
}

/// Orphan rule: stdin hit EOF and no controlling terminal remains, so no
/// input will ever arrive again and the loop should exit instead of
/// spinning on background tasks.
pub fn client_orphaned_with(stdin_eof: bool, has_terminal: bool) -> bool {
    stdin_eof && !has_terminal
}

/// Live orphan check against the real `/dev/tty`.
pub fn client_orphaned(stdin_eof: bool) -> bool {
    client_orphaned_with(stdin_eof, has_controlling_terminal())
}
